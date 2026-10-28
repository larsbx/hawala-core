# Cross-authority TEST host conformance

Status: **test-only implementation increment**, 2026-10-28. Canonical semantics
remain [CROSS_AUTHORITY_V1](CROSS_AUTHORITY_V1.md), `CrossAuthority.lean` and
`FinancialEvidence.lean`. Architecture ADR 0005 remains **Proposed**.

## Implemented boundary

Rust `hawala-cross-authority-test` and biz-kit's test-only
`HawalaCapability.TestHost` independently implement the finite model's event
boundary, not a live financial protocol. Each host has a coordinator journal
and **two separate participant journals**. Each actor persists only its own
model fields plus request receipts; no shared database transaction commits two
authorities. The harness supplies the model's authenticated-message, fence and
complete-history assumptions as explicit test events. It does not validate
these assumptions against real financial evidence, legal mandates or hardware.

Each command is parsed against the fixed event alphabet. A durable request ID
binds its exact command in its receiving actor's journal; an exact duplicate
returns the original receipt, including a held/stuttering result. A payload
change conflicts permanently. Replayed old receipts cannot replace current
state. Version checking uses the local snapshot from which the candidate state
was derived, so a stale writer cannot overwrite a newer terminal record.
This fixed TEST profile contains no balances, participant keys or monetary
postings and is not a substitute for real manifest/nonce admission.

Rust uses independent SQLite transactions with WAL and synchronous FULL. The
Elixir adapter uses three independent GenServers and local OTP `disk_log`
journals: each command's resulting local state and replay receipt occupy one
record, explicitly synced before acknowledgment. It never truncates or
silently auto-repairs a log. An uncertain write stops that actor and returns
HOLD; a missing, mismatched, unclean or corrupt journal cannot enroll an empty
replacement. Its owner registry only prevents duplicate owners within the
same test BEAM VM; cross-VM/process ownership fencing is **not implemented**.
See the [OTP disk_log contract](https://www.erlang.org/docs/25/man/disk_log.html).

Availability/partition flags are a separate volatile fault schedule. The formal
`crash` events are driver controls; clean store reopen is tested at every
prefix. Additional tests exercise a Rust subprocess exit after durable commit
without an ACK, and killing an Elixir worker. The latter either restores its
complete retained decision or returns HOLD on an unclean log; it does not
manufacture a peer ACK or unlock reservations. These are bounded storage and
lifecycle tests, not a two-machine Iroh/network implementation or a proof of
power-loss behavior.

## Canonical corpus and negative checks

- `formal/traces/crossAuthority.tsv`: 232 prefixes, 13 scenarios, every one of
  the 24 projected fields compared after every event. The TSV and existing
  JSON/Quint witnesses are generated from the same Lean exporter.
- `formal/traces/financialGates.tsv`: 72 Lean-generated combinations of six
  evidence states, six workflow actions and verified/unverified input. The
  verification Boolean remains an **external assumption**; the test adapters
  are not cryptographic/custody/finality verifiers.
- Both hosts persist/reopen during replay, check exact duplicate receipts and
  reject unknown/malformed events, changed request payloads, stale writers,
  missing/wrong-role stores and deliberately false commit projections.
- Rust injects failure between state and receipt writes to require rollback,
  and races commit/abort across separate SQLite connections. Elixir fills a
  journal to force a write failure, races coordinator calls and checks failed
  worker behavior.
- The existing five Quint counterexamples, model agreement proof and signed
  evidence vectors remain mandatory and unchanged.

The biz-kit fixture copies must be byte-identical to these canonical TSVs at
its pinned immutable Hawala source commit. Its pairing CI verifies the source,
copy hashes and unchanged identical N1–N7/G1–G11 text. Editing either copy or
following unpinned `main` is rejected.

Rust's default build excludes the test API; CI explicitly enables its
`conformance` feature for these tests. Elixir compiles both adapter modules only
from `test/support` in `MIX_ENV=test`; CI also checks that a production build
contains neither module. No live endpoint or business workflow calls either
adapter. Production authority/custody semantics remain solely Hawala-owned.

## Reproduce and remaining obligations

```sh
(cd formal/lean && lake build)
bash tools/check_trace_artifacts.sh
cargo test -p hawala-cross-authority-test --features conformance
# From a paired biz-kit checkout:
(cd apps/hawala_capability && MIX_ENV=test mix test --no-start)
python3 tools/check_hawala_architecture_pin.py --source ../hawala-core
```

This closes only the **selected persistent TEST-adapter conformance subtask**.
It does not close G3 or D5.5 for an actual financial writer. Outstanding gates
include mutually authenticated ports/Iroh delivery, durable real attempt/QR
consumption, exact signed-envelope and mandate verification, financial prepare
admissibility under changing policy, actual authority/custody boundary fencing,
accounting and real finality verification. Independent formal/security review
and G4–G8 remain prerequisites for synthetic checkout orchestration. No live
money, payment finality, escrow release or distributed settlement readiness is
established by these tests.
