# Iroh network pilot: evidence and transport-fault verification scaffold

Status: **implementation scaffold, not pilot acceptance** (2026-11-08).
Baseline: `hawala-core` main already merged [PR #60](https://github.com/larsbx/hawala-core/pull/60)
with live QUIC loopback identity/ALPN tests, a local pinned-release TLS relay,
a relay-loss handshake test, durable lost-ACK observation recovery, and direct
8 MiB GSO-on/off checks. Do not duplicate or reinterpret those as cellular,
DNS discovery, or deployed-infrastructure evidence.

## Proof-first safety boundary

`formal/lean/TransportFaultFence.lean` models **one already scoped synthetic
request** with enqueue, one mutation dispatch, outage, timeout, reconnection,
restart, replay, and independently verified status reconciliation.
It proves that reconnect/restart/replay stutter; invalid/incomplete status
cannot release a hold; a dispatched request cannot be dispatched again;
transport events preserve the dispatch count; every abstract reachable state
has at most one dispatch and remains unpaid.

`formal/quint/transportFaultFence.qnt` independently models the same
transition vocabulary and has a finite TLC safety invariant and mandatory
counterexample controls for duplicate dispatch, premature ACK/hold release,
and equating timeout with paid. These are **abstract models**: the `valid`
status parameter assumes independently checked service signature, exact
request/attempt/binding, authority epoch, freshness, and complete durable
effects. There is **no verified refinement from the models to Rust/SQLite**,
no packet-delivery liveness proof, no financial authority and no deployment.
A status saying "complete" is still not physical cash delivery or settlement.

Run:
```bash
(cd formal/lean && lake build TransportFaultFence)
npm ci --prefix formal --ignore-scripts
bash tools/check_transport_fault_fence.sh
```

## Environment-dependent acceptance matrix

| Scenario ID | Environment | Reproducible setup and required observation |
| --- | --- | --- |
| `cellular-gso-enabled` | `live-mobile` | Android over carrier-grade NAT, direct bulk upload and signed report, GSO on; record path events, byte hash and held-on-ambiguity behavior. |
| `cellular-gso-disabled` | `live-mobile` | Same device, peer, carrier and payload with offload disabled; compare throughput and selected paths; no assumed improvement. |
| `dual-stack-under-load` | `live-mobile` | Sustained v4/v6 cellular transfer through >2 path-selection intervals; count selected-path switches, timeouts and recoveries. |
| `wifi-to-cellular` | `live-mobile` | Change interface while signed request is unresolved; reconnect and require fresh transport/QR binding, no duplicate mutation. |
| `relay-drop-after-request` | `local-lab` | Fault sole relay after a scoped signed request but before ACK; restart outbox/journal; require Held unless an independently signed exact status resolves it. |
| `relay-free-lan-refresh` | `local-lab` | WAN/relay blocked; change LAN address and verify discovery refresh and mutual identity rebind on the chosen resolver. |
| `dns-many-behind-nat` | `local-lab` | 250 synthetic endpoints behind one egress IP; record per-endpoint publish/findability times, 429s, and rate-limit profile. |
| `relay-file-descriptor-exhaustion` | `local-lab` | Disposable nonproduction relay with low file-descriptor limit; exhaust/release and measure CPU, log growth, accept recovery. |
| `deployed-relay-tls` | `deployed-artifact` | Independently inventory running relay binary SHA-256, Iroh/rustls lock, TLS trust and current release. |
| `deployed-dns-tls` | `deployed-artifact` | Same inventory for DNS/Pkarr service; verify resolver configuration and TLS trust. |

These are **pending**, not executed by the new scaffold. Upstream concerns:
[Iroh #4647](https://github.com/n0-computer/iroh/issues/4647) (Android GSO),
[#4644](https://github.com/n0-computer/iroh/issues/4644) (mobile flapping),
[#4613](https://github.com/n0-computer/iroh/issues/4613) (NAT-concentrated
Pkarr limits), and [#4636](https://github.com/n0-computer/iroh/issues/4636)
(relay EMFILE). The bridge currently pins Iroh 1.4.1; rebaseline both the
tests and the exact source/binary inventory before another version is used.

## Machine-readable observed-run metadata

`tools/iroh_pilot_acceptance.py` admits **only** the ten exact scenario IDs
and a narrow, privacy-conscious JSON schema. The initial checked-in fixture
`tools/fixtures/iroh_pilot_pending.json` contains **zero observations**.

```bash
python3 -m unittest discover -s tools/tests -p 'test_iroh_pilot_acceptance.py' -v
python3 tools/iroh_pilot_acceptance.py tools/fixtures/iroh_pilot_pending.json
# --require-complete rejects incomplete manifests; only real observed runs belong here.
python3 tools/iroh_pilot_acceptance.py pilot-private.json --require-complete
```

Each item under `runs` must contain **exactly** these keys:
```json
{
  "scenario": "relay-drop-after-request",
  "environment": "local-lab",
  "observed_at": "2026-11-08T18:00:00Z",
  "iroh_version": "1.4.1",
  "trace_sha256": "<64 lowercase hex characters of a separately retained redacted trace>",
  "privacy_redacted": true,
  "behavior_observed": true,
  "dispatches": 1,
  "financial_authority": false,
  "ambiguous": true,
  "held": true,
  "status_advanced": false,
  "status_signature_verified": false
}
```

The receipt checker rejects unknown fields (such as identities/addresses),
duplicate scenarios, inappropriate environment, wrong dependency baseline,
missing trace digest, stale/future metadata, non-Boolean status flags,
multiple mutation dispatches, non-Held ambiguous dispatched effects, claimed
financial authority, and status advancement without a separately verified
signature. A successful `--require-complete` validation reports
`observations-present-not-certified`, **never** proof of authenticated
trace origin or production readiness. A digest alone does not authenticate
the recorded trace. Traces must remain in an access-controlled evidence store;
do not put account data, endpoint IDs, keys, exact locations or counterparties
in this manifest or public CI logs.

## Remaining integration and acceptance work

1. Build a real mobile runner using `Connection::path_events()` and
   `Connection::paths()` telemetry, with bounded sampling, per-session
   correlation IDs, and privacy filtering; preserve the existing protocol
   and ALPN. Do not accept peer-supplied verification results.
2. Build isolated disposable DNS/NAT and relay EMFILE experiments. Never
   exhaust production infrastructure or publish financial records on Pkarr.
3. Bind every reported status to the exact signed request, epoch, and
   completed durable actor effects. Reuse Hawala's existing outbox/journal;
   never introduce an alternate payment or retry authority.
4. Add independent source/trace authentication and artifact provenance.
   The metadata checker presently performs **shape and safety-field checks
   only**; a self-asserted value is not trusted evidence.
5. Run actual hardware/network experiments and attach exact-head receipts,
   failures, counterexamples, and runner configuration before relaxing gates.

No newly added formal theorem or passing metadata unit test constitutes an
end-to-end Hawala execution, settlement, security, or liveness proof.
