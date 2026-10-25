# Proof-driven financial state machine — first verification layer

This layer specifies the merged TEST authority before adding reconciliation,
adjudication or release behavior. A Rust change that alters admission, claiming,
holds or recovery must first update this specification, prove its invariants,
check its finite behaviors and add a corresponding implementation regression.
No reconciliation feature or production financial permission is added here.

## Checked claims

Lean 4.28.0, using only the bundled standard library, proves for every reachable
state and arbitrary natural-number funding/reservation amounts:

- exact conservation of available plus reserved liabilities;
- atomic equality of nonce consumption and committed-intent status;
- at most one dispatch claim;
- claiming requires a committed intent and results in uncertainty;
- uncertainty is permanent under all modeled steps;
- an uncertain attempt cannot be reclaimed;
- expiry is permanent and expired intents cannot be claimed, including after
  a clock rollback represented by a later valid-scope claim;
- a mismatched authority epoch blocks new admission and new claims, and the
  fence remains in force under all modeled events.

`Reachable` includes arbitrary finite event sequences. Proofs contain no
placeholders or user axioms. The printed axiom dependencies are Lean's standard
`propext` and `Quot.sound`, not assumptions about banking or cryptography.

Quint 0.32.0/TLC checks the same abstract lifecycle with funding 1000 and reservation
900, including valid/invalid scope, admission, claim, hold and crash/replay/reject
stuttering. TLC exhausts this finite instance (ten reachable states); this is
not random simulation or a bounded-only claim. The deliberately unsafe claim
rule MUST produce a counterexample with two claims. A second mutation that
allows exactly one claim after expiry MUST also produce a counterexample. A third mutation that allows one claim under
a mismatched authority epoch must fail too. The CI script distinguishes
that counterexample from tool failures. Apalache 0.56.1 is the Quint-pinned
translation tool; TLC performs the finite-state search.

## Model-to-implementation obligations

| Model transition or property | Rust surface | Existing regression evidence |
|---|---|---|
| Atomic admission | `Simulator::admit` | `storage_failure_after_nonce_and_balance_writes_rolls_back_everything`; `outbox_enqueue_and_claim_have_no_visible_partial_commits` |
| Exact reserve conservation | `Book::post`, admission transaction | `one_sqlite_commit_moves_only_synthetic_liability_to_reserved_for_all_peer_modes` |
| Guarded claim | `claim_test_dispatch` | `outbox_scope_expiry_and_prior_reports_fail_closed` |
| Claim once | `claim_test_dispatch` | `competing_workers_receive_only_one_synthetic_dispatch` |
| Crash/replay stutter | durable journal and tombstones | `abrupt_exit_after_claim_never_reissues_dispatch`; `durable_outbox_claims_once_then_stays_uncertain_after_restart` |
| Permanent uncertainty | `mark_uncertain`, `record_observation` | `signed_observation_replay_conflict_and_restart_never_release_or_repeat` |

This table documents the obligations; the trace harness below checks a finite
projection correspondence, NOT a universal machine-checked refinement proof.
Lean and Quint are separately authored models; their equivalence is not proved.
The model abstracts one currency, one lot and one permanent attempt. Concurrent
workers are serialized into claim interleavings; a valid-scope Boolean summarizes
signature, peer and policy checks; authority-epoch fencing is explicit and an expiry event models the
persisted cutoff failure. There is no claim that these
checks themselves have been formally verified.

The trusted atomic/crash assumptions are: one authority serialization order,
a committed transaction persists across crash, failed transactions expose no
partial state, and backing storage is not rolled back or tampered with. SQLite,
Rust code, cryptographic signatures, Iroh, real custody, current legal/operational
authority and physical delivery are outside the proof. Backups, epoch handoff,
multiple lots/currencies, individual nonces, observations and conflicting evidence
still require richer models and a checked implementation refinement.

## Reproduce

From `formal/lean`, run `lake build`; `lean-toolchain` pins the compiler.
From the repository root, run `npm ci --prefix formal --ignore-scripts` and
`bash tools/check_quint.sh` with Node 24 and Java 17. The workflow independently
runs both proof gates on every PR and main push. Existing Rust tests remain
mandatory. Neither model verification nor passing tests enables real money,
payouts, collateral release or settlement.


## Lean-generated trace conformance

`lean/Export.lean` enumerates ten witnessed reachable states with canonical
paths. Kernel-evaluated checks establish the state count and closure under the
ten-event alphabet; a theorem proves the alphabet covers all Event constructors
and both Boolean guard values. It exports 100 transition-cover paths and their
expected ten-field state projections directly from `Hawala.step`.

CI regenerates `traces/lifecycle.tsv` and `quint/conformance.qnt` and rejects any
drift. Quint runs the generated paths against its own transition definitions.
The Rust test `lean_generated_transition_cover_matches_durable_rust_for_all_peer_modes`
replays each path through the real SQLite-backed methods for all four participant
modes and compares every prefix against the Lean-generated projection. It also
checks unchanged synthetic assets and version advancement. Returned dispatch
values are counted independently of the outbox row, preventing a duplicate
return from being hidden by a single persisted row.

Event adapters: admit uses the signed fixture envelope; denied changes the
external-control report; claim uses the pinned peer/epoch; stale uses a wrong
epoch; expire attempts dispatch at the stored cutoff and records permanent expiry; hold calls
mark_uncertain; crash closes/reopens the SQLite connection; replay retrieves the
existing admission result or checks absence; reject submits an unrelated peer.
The crash adapter is a graceful restart: existing abrupt-exit and rollback tests
remain separate evidence for persistence assumptions. No abstract event claims
to exercise every concrete rejection, report, timeout or failure path.

This covers every outgoing edge of the finite abstract graph using one canonical
path per source. It does not enumerate all concrete states, all histories,
concurrent schedules, custody/grant values, or SQL failures. Equality of sampled
projections is executable conformance evidence, not a compiler proof, universal
refinement, or proof that Rust, Lean and Quint are semantically equivalent.


The expiry event represents an authenticated pending dispatch attempt at/after
its cutoff, not mere passage of wall-clock time. Rust persists `held` only when
that check runs; unrelated holds and already-claimed attempts do not acquire an
expiry marker. The eighth projection field reads that durable marker. Generated
paths include expiry followed by claim at the earlier fixture time, repeated
expiry, replay, and reopen. Expiry threshold calculation and clock authenticity
remain implementation obligations rather than facts proved by the abstract model.


## Authority epoch fence

`epochCurrent` records whether this fixture's pinned epoch still matches the
persisted authority epoch. `fence` changes that predicate from true to false;
there is deliberately no restoration, enrollment or authority-succession event.
A claim made before fencing may remain recorded afterward, so it would be
incorrect to require every historical claim to have a currently matching epoch.
Instead, a ghost counter `fencedClaims` tracks new claims returned while fenced
and is proved zero. Separate Lean theorems show the fenced admission/claim
transitions stutter and the mismatch remains permanent in this model.

The finite graph doubles to ten states with 100 outgoing-event paths. Rust
conformance injects epoch 4 into the TEST database, whose signed fixture and
stored attempt remain pinned to epoch 3. This SQL injection is test setup only,
not an operational handoff or an authorization mechanism. Each returned dispatch
is counted independently, including whether the persisted epoch was fenced when
it returned. The projection includes both current-epoch matching and this count.
Restart, old receipt replay and repeated fencing cannot produce another claim.

The fence event assumes an externally established durable epoch change. It does
not prove who may change epochs, quorum succession, controller membership,
legally effective mandates, restoration of service, or resistance to database
rollback/tampering. Existing claims and collateral remain reserved. Actual
authority handoff requires its own policy, proofs and independent acceptance;
this increment changes no runtime API or financial permission.
