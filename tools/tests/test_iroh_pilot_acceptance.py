"""Deterministic tests of receipt validation, NOT network/hardware acceptance."""
import sys
from pathlib import Path
import unittest
from datetime import datetime, timezone

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from iroh_pilot_acceptance import SCENARIOS, evaluate

NOW = datetime(2026, 11, 8, 18, 0, tzinfo=timezone.utc)


def observation(scenario, **overrides):
    record = {
        "scenario": scenario,
        "environment": SCENARIOS[scenario],
        "observed_at": "2026-11-08T17:59:00Z",
        "iroh_version": "1.4.1",
        "trace_sha256": "a" * 64,
        "privacy_redacted": True,
        "behavior_observed": True,
        "dispatches": 1,
        "financial_authority": False,
        "ambiguous": True,
        "held": True,
        "status_advanced": False,
        "status_signature_verified": False,
    }
    record.update(overrides)
    return record


class PilotEvidenceTests(unittest.TestCase):
    def gate(self, runs):
        return evaluate({"schema": 1, "runs": runs}, now=NOW)

    def test_pending_is_not_passed(self):
        result = self.gate([])
        self.assertEqual(result["status"], "pending")
        self.assertEqual(len(result["missing"]), len(SCENARIOS))
        self.assertFalse(result["source_authenticated"])

    def test_all_hypothetical_receipts_still_not_certified(self):
        result = self.gate([observation(s) for s in SCENARIOS])
        self.assertEqual(result["status"], "observations-present-not-certified")
        self.assertEqual(result["missing"], [])
        self.assertFalse(result["source_authenticated"])

    def test_duplicate_dispatch_rejected(self):
        r = self.gate([observation("relay-drop-after-request", dispatches=2)])
        self.assertEqual(r["status"], "rejected")

    def test_ambiguous_dispatch_cannot_release_hold(self):
        r = self.gate([observation("relay-drop-after-request", held=False)])
        self.assertEqual(r["status"], "rejected")

    def test_unverified_status_cannot_advance(self):
        r = self.gate([observation("relay-drop-after-request", status_advanced=True)])
        self.assertEqual(r["status"], "rejected")

    def test_privacy_and_source_profile_required(self):
        r = self.gate([observation("cellular-gso-enabled", privacy_redacted=False,
                                   environment="local-lab")])
        self.assertEqual(r["status"], "rejected")

    def test_duplicate_scenario_and_extra_fields_rejected(self):
        a = observation("deployed-relay-tls")
        r = self.gate([a, dict(a)])
        self.assertEqual(r["status"], "rejected")
        r = self.gate([dict(a, endpoint_id="sensitive")])
        self.assertEqual(r["status"], "rejected")

    def test_stale_or_wrong_version_rejected(self):
        r = self.gate([observation("relay-free-lan-refresh", observed_at="2026-07-01T00:00:00Z",
                                   iroh_version="1.3.0")])
        self.assertEqual(r["status"], "rejected")

    def test_boolean_strictness_and_refusal_of_financial_authority(self):
        r = self.gate([observation("deployed-dns-tls", dispatches=True,
                                   financial_authority=1)])
        self.assertEqual(r["status"], "rejected")


if __name__ == "__main__":
    unittest.main()
