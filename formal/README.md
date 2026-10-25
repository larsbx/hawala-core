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
- an uncertain attempt cannot be reclaimed.

`Reachable` includes arbitrary finite event sequences. Proofs contain no
placeholders or user axioms. The printed axiom dependencies are Lean's standard
`propext` and `Quot.sound`, not assumptions about banking or cryptography.

Quint 0.32.0/TLC checks the same abstract lifecycle with funding 3 and reservation
2, including valid/invalid scope, admission, claim, hold and crash/replay/reject
stuttering. TLC exhausts this finite instance (four reachable states); this is
not random simulation or a bounded-only claim. The deliberately unsafe claim
rule MUST produce a counterexample with two claims. The CI script distinguishes
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

This table is a reviewed correspondence, NOT a machine-checked refinement proof.
Lean and Quint are separately authored models; their equivalence is not proved.
The model abstracts one currency, one lot and one permanent attempt. Concurrent
workers are serialized into claim interleavings; a valid-scope Boolean summarizes
signature, peer, policy, epoch and expiry checks. There is no claim that these
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
