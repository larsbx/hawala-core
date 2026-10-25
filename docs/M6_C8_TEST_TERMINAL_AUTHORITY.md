# M6-C8: synthetic terminal execution authority and held evidence

Status: **test-only prerequisite for future authorized resolution**. No bank,
vault, escrow, federated asset controller, customer payout or ATM command is
implemented. A mock effect is one SQLite counter increment, not delivered cash.
Even a verified no-effect receipt leaves financial balances and collateral held.
Read with [M6-C6](M6_C6_COORDINATION.md) and
[coordinated admission](M6_C7_COORDINATED_ADMISSION.md).

## One mock effect or one permanent fence

`TestExecutionRail` is a separate durable SQLite authority for one scoped test
attempt. `create` exclusively creates a new file; `open` never creates, resets or
reenrolls missing state. Memory/URI paths fail closed. It uses verified WAL mode,
`synchronous=FULL`, and `BEGIN IMMEDIATE` for its state and command journal.

| State | Execute a matching synthetic dispatch | Fence no effect |
| --- | --- | --- |
| Pending, version 1 | Commit one mock effect and terminal receipt, version 2 | Commit a terminal no-effect fence, version 2 |
| Mock effect, version 2 | Return the original terminal status; no second effect | Return the original mock-effect status; cannot certify no effect |
| Fenced no effect, version 2 | Return the original fence; no effect, even for a delayed valid dispatch | Return the original fence |

The terminal signature, mock counter, state version and command result commit
together before any result is returned. A lost acknowledgment is recovered by
querying the original terminal receipt. Pending/timeout is not a no-effect
receipt. Command responses report the attempt's terminal status; they do not
claim that a conflicting later command was performed.

Each nonzero command ID permanently pins its canonical operation, full rail
context and expected version. Identical replay returns its original terminal or
held response. A changed payload conflicts. Stale pending commands are retained
as held and do not become executable on retry under the same ID. A new command
against an already terminal attempt can only observe its original final status.

## Committed scope and independently pinned test policy

The financial simulator derives `TestExecutionScope` from its existing committed
coordinated decision, admission receipt, event, exact debit posting and QR-bound
recovery subject. It contains transfer, attempt, complete coordinated operation
digest, authenticated cash-peer identity, financial epoch, currency and exact
minor-unit amount. The operation digest binds accepted terms, both custody
requests, QR scope, quote, postings and version references.

A claimed `SyntheticDispatch` now carries that optional full scope and the
persisted terminal policy (including rail audience/controller/epoch). Scope is
captured in its existing claim transaction, which marks uncertainty before
returning the value. A pending/preflight/aborted operation cannot obtain scope;
legacy dispatches have none. `execute` requires the nonconstructible dispatch
value and an exact full-scope and policy match. Claiming before policy enrollment
cannot produce an executable rail ticket. Scope metadata alone is not a dispatch or
execution permission. QR-only participant presentation and Iroh are unchanged.

`pin_test_terminal_authority` separately pins a local TEST policy before new
enrollment becomes stale: network, rail ID, controller key and rail epoch. This
is trusted fixture configuration, never chosen from an incoming peer frame, and
it is not a deployment-acceptance or operational-authority API. The policy cannot
be replaced, including by a new epoch. Reopen must match the complete original
rail policy and scope. There is no controller/key migration or restore API.

The financial epoch and rail epoch are distinct. Historical evidence can be
retained after financial epoch fencing. Changing a financial epoch does not
revoke an already returned external dispatch or fence an external rail; the
actual execution authority must establish that fence. This test rail's explicit
no-effect transition is its only execution fence.

## Signed, bounded terminal frame

`SignedTestTerminal` is a fixed 340-byte frame:

| Field | Bytes |
| --- | ---: |
| `HWTTERM1` magic | 8 |
| Test policy: network, rail, controller, epoch | 104 |
| Full execution scope | 147 |
| Outcome: mock effect or fenced no effect | 1 |
| Terminal state version, always 2 | 8 |
| Mock effect count, exactly 1 or 0 for that outcome | 8 |
| Ed25519 signature | 64 |

The signature covers `hawala/TEST-terminal-execution/v1\0` followed by the
276-byte body. Fixed-width integers are big-endian. Parsing rejects suffixes,
truncation, unknown outcomes and inconsistent version/count encodings; parsing
alone does not authenticate anything. No JSON, floating point or physical
execution assertion is involved. These frames cannot be used as the existing
signed peer-observation format or a real custody command receipt.

The pluggable `TestTerminalAuthority` verifies against the simulator's persisted
policy and expected scope. `SignedTestRailAuthority` verifies the canonical
signature and context. Its opaque result is only test evidence.
`UnsupportedRealTerminalAuthority` always holds, even for a correctly signed test
frame; no real-provider fallback is available.

## Held financial evidence

`record_test_terminal_evidence` accepts only a committed coordinated scope under
the stored test policy. It retains the complete original frame, exact replay
identity and any authenticated conflicting frames. Evidence retention, permanent
uncertainty and blocking a pending outbox claim share one financial transaction.
An already claimed outbox row keeps its historical claim marker.

No terminal receipt changes ledger balances, postings, nonce/lot consumption,
custody checkpoints or the committed bilateral decision. It cannot reverse an
intent, permit another dispatch, discharge a liability, settle a claim or release
collateral. A no-delivery peer report remains ordinary held evidence; it cannot
be parsed or upgraded into this terminal-receipt type.

The fault test deliberately forks a rail into two independent stores with the
same key, policy and scope. One signs a mock effect; the other signs no effect.
Both signatures verify. The financial journal retains the contradiction and
keeps all funds held. This demonstrates why signatures alone cannot establish
an honest authority, unique external ordering, asset control or actual delivery.

## Formal and adversarial evidence

`TestExecution.lean` proves safety for arbitrary finite modeled histories:
at most one mock effect, a required valid scoped ticket, permanent terminal
outcomes/effect counts, and no late execution after a no-effect fence. Valid
ticket/scope Booleans abstract the concrete checks; cryptography and actual
financial commit are not proved inside this rail model.

Quint/TLC exhausts five states. Separate duplicate-effect, execute-after-fence
and missing-ticket mutations must yield counterexamples, not tool failures.
A ghost monitor remembers the no-effect fence for the second check. Lean checks
alphabet completeness and graph closure and exports 35 transition paths. Quint
replays them independently; Rust replays every prefix for all four participant
pairings (140 fixtures) using actual committed scopes, dispatch claims and durable
rail operations. The invalid-ticket adapter uses a rejected peer claim; executing
without a dispatch value stutters because Rust cannot construct that capability.
Trace restart reopens the rail while its client still holds the original value.

Additional tests cover independent-connection execute/fence races, original
command-ID conflicts and stale tombstones, complete transaction rollback,
malformed/wrong-scope/stale-authority signatures, legacy/preflight rejection,
held evidence/replay/conflict preservation, and abrupt exit after either terminal
outcome. Each trace prefix and evidence-ingestion scenario compares the original
financial records, postings, balances, nonce/lot and custody versions unchanged.

The financial evidence transition has the same hold-only projection as the
existing `Observations.protected_unchanged` specification. This is a specification
mapping plus executable regression evidence, not a universal Rust refinement
proof or a proof of cross-model/implementation equivalence.

The trusted rail assumptions are one accepted durable store per scoped attempt,
honest non-rolled-back storage, protected signing authority and serialized state.
The mock counter is itself the whole modeled effect. A real physical execution
cannot be made atomic with SQLite by this design. File deletion, backup rollback,
host compromise, split authorities, actual custody/federation agreement and key
succession require independently verified operational controls. This increment
provides none of that live authority and therefore cannot authorize financial
resolution. A future resolution simulator needs a separately pinned test
resolution mandate and verified test terminal evidence. Live resolution additionally
requires independently verified custody/execution control, operational authority
and deployment acceptance; a test mandate cannot authorize it.

Reproduce: `lake build` in `formal/lean`; at repository root run
`bash tools/check_trace_artifacts.sh`, `bash tools/check_test_execution.sh`, and
`cargo test -p hawala-commit-sim --test atomic terminal::`. Full unfiltered CI uses
`cargo test --workspace --all-targets` and retains the existing live Iroh tests.
