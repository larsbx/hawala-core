#!/usr/bin/env python3
"""Fail-closed metadata gate for Iroh pilot observations (NOT source attestation).

Only controlled scenario IDs and safe metadata are admitted. A complete manifest
is still self-reported evidence, not proof that tests actually ran or passed.
"""
import argparse
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import re
import sys

SCENARIOS = {
    "cellular-gso-enabled": "live-mobile",
    "cellular-gso-disabled": "live-mobile",
    "dual-stack-under-load": "live-mobile",
    "wifi-to-cellular": "live-mobile",
    "relay-drop-after-request": "local-lab",
    "relay-free-lan-refresh": "local-lab",
    "dns-many-behind-nat": "local-lab",
    "relay-file-descriptor-exhaustion": "local-lab",
    "deployed-relay-tls": "deployed-artifact",
    "deployed-dns-tls": "deployed-artifact",
}
FIELDS = frozenset({
    "scenario", "environment", "observed_at", "iroh_version", "trace_sha256",
    "privacy_redacted", "behavior_observed", "dispatches", "financial_authority",
    "ambiguous", "held", "status_advanced", "status_signature_verified",
})
HEX64 = re.compile(r"[0-9a-f]{64}\Z")


def evaluate(manifest, now=None):
    """Return a deterministic report; this never authenticates the trace hash."""
    now = now or datetime.now(timezone.utc)
    errors = []
    if not isinstance(manifest, dict) or set(manifest) != {"schema", "runs"}:
        return {"status": "rejected", "errors": ["expected exactly schema and runs"],
                "observed": [], "missing": sorted(SCENARIOS), "source_authenticated": False}
    if type(manifest["schema"]) is not int or manifest["schema"] != 1:
        errors.append("unsupported schema; expected integer 1")
    records = manifest["runs"]
    if not isinstance(records, list):
        return {"status": "rejected", "errors": errors + ["runs must be a list"],
                "observed": [], "missing": sorted(SCENARIOS), "source_authenticated": False}
    seen = set()
    for index, record in enumerate(records):
        prefix = f"runs[{index}]"
        if not isinstance(record, dict):
            errors.append(f"{prefix}: expected object")
            continue
        extra = set(record) - FIELDS
        missing_fields = FIELDS - set(record)
        if extra or missing_fields:
            errors.append(f"{prefix}: unknown fields {sorted(extra)}; missing fields {sorted(missing_fields)}")
            continue
        scenario = record["scenario"]
        if not isinstance(scenario, str) or scenario not in SCENARIOS:
            errors.append(f"{prefix}: unknown scenario")
            continue
        if scenario in seen:
            errors.append(f"{prefix}: duplicate scenario {scenario}")
            continue
        seen.add(scenario)
        if record["environment"] != SCENARIOS[scenario]:
            errors.append(f"{prefix}: environment mismatch for {scenario}")
        if record["iroh_version"] != "1.4.1":
            errors.append(f"{prefix}: not the pinned Iroh 1.4.1 baseline")
        if not isinstance(record["trace_sha256"], str) or not HEX64.fullmatch(record["trace_sha256"]):
            errors.append(f"{prefix}: missing lowercase SHA-256 trace digest")
        for name in ("privacy_redacted", "behavior_observed", "financial_authority",
                     "ambiguous", "held", "status_advanced", "status_signature_verified"):
            if type(record[name]) is not bool:
                errors.append(f"{prefix}: {name} must be boolean")
        if record["privacy_redacted"] is not True or record["behavior_observed"] is not True:
            errors.append(f"{prefix}: redacted, actually observed evidence required")
        if type(record["dispatches"]) is not int or record["dispatches"] not in (0, 1):
            errors.append(f"{prefix}: duplicate or invalid mutation dispatch count")
        if record["financial_authority"] is not False:
            errors.append(f"{prefix}: financial authority must remain false")
        if record["ambiguous"] is True and record["dispatches"] == 1 and record["held"] is not True:
            errors.append(f"{prefix}: ambiguous dispatched request must remain held")
        if record["status_advanced"] is True and record["status_signature_verified"] is not True:
            errors.append(f"{prefix}: status advancement without verified signed status")
        try:
            raw = record["observed_at"]
            if not isinstance(raw, str) or not raw.endswith("Z"):
                raise ValueError("not a UTC timestamp")
            stamp = datetime.fromisoformat(raw.replace("Z", "+00:00"))
            if stamp < now - timedelta(days=30) or stamp > now + timedelta(minutes=5):
                errors.append(f"{prefix}: stale or future observation")
        except (ValueError, TypeError, OverflowError):
            errors.append(f"{prefix}: invalid UTC observation timestamp")
    missing = sorted(set(SCENARIOS) - seen)
    status = "rejected" if errors else ("pending" if missing else "observations-present-not-certified")
    return {"status": status, "errors": errors, "observed": sorted(seen),
            "missing": missing, "source_authenticated": False}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--require-complete", action="store_true")
    args = parser.parse_args(argv)
    try:
        report = evaluate(json.loads(args.manifest.read_text(encoding="utf-8")))
    except (OSError, ValueError) as exc:
        report = {"status": "rejected", "errors": [str(exc)], "source_authenticated": False}
    print(json.dumps(report, sort_keys=True, indent=2))
    return 1 if report["status"] == "rejected" or (
        args.require_complete and report["status"] != "observations-present-not-certified") else 0


if __name__ == "__main__":
    sys.exit(main())
