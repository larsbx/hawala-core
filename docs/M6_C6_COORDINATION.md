# M6-C6: bilateral custody coordination and resolution contract

Status: normative design plus an isolated **TEST-ONLY** coordination journal.
No real custody adapter, financial certificate, financial admission integration,
collateral release, customer payout or ATM command is implemented here.
This is the first roadmap dependency toward live operations, not acceptance for
live operations. Read with [custody-interface.md](custody-interface.md) and
[M6_C_AUTHORITY.md](M6_C_AUTHORITY.md).

## 1. One decision for both reservations

For each financial operation, pin the complete accepted exchange terms, both
responsible hawaladars, funding/credit-lot provenance, both custody accounts and
allocation IDs, exact currency/amount on each side, mandate/policy digests,
epochs, versions, and the financial prepare/operation digest. A single durable
coordination order must own the final decision for this binding. Independent
escrow and federated custody may supply either allocation; neither signatures
nor the mode name establish actual asset control.

| State/event | Required behavior |
| --- | --- |
| Preparing, zero or one confirmed reservation | No spendable new credit or financial commit |
| Preparing, both independently verified reservations | Eligible for financial admission; all other admission requirements still apply |
| Commit wins the decision order | Financial commit and decision tombstone share the authoritative transaction/order; both allocations remain encumbered |
| Final abort wins the decision order | Commit is permanently impossible for this binding; no nonce or financial postings may later commit for it |
| Missing acknowledgement, partition or timeout | Query the original IDs; retain locks, without assuming abort |
| Reservation confirms after final abort | Retain the late confirmation; release only against the same fenced abort |
| Commit after final abort, or abort after commit | Reject/stutter; a terminal decision cannot change |
| Release before final abort | Forbidden by this preparation protocol |
| Committed allocation | Remains locked until a separately authorized discharge/resolution protocol applies |

Reservations must be durable, exclusively allocated and non-expiring while a
commit can still finalize. An account snapshot, quorum of signed claims, local
timer or cached absence of a commit is not a reservation or abort certificate.
There is no fallback that turns one dar's reservation into coverage for both.

The common order can be implemented by an independently accepted authority or
agreement protocol; it need not be a global centralized server. Its fault model,
partition behavior, persistence and anti-rollback protection must be explicit.
Iroh remains the peer transport; transport success is not financial finality.

## 2. What a final abort must certify

A real abort certificate must bind the full operation/prepare and reservation
references, the responsible decision authority and policy/epoch, its ordered
terminal decision, and retrievable authenticated evidence of that decision.
Custodians must independently verify that no delayed financial commit or external
execution can become valid for that binding. Conflicting decisions must be
impossible under the accepted ordering/fault assumptions, not merely detectable
after assets have been released.

If decision state and financial postings live in one store, their commit belongs
in the same transaction as the existing admission verifier, nonce and ledger
postings. If stores or custodians are separate, specify and verify the common
coordination protocol; two successful SQLite commits are not distributed atomicity.
The current separate test journal is **not** such an integration.

Decision and request tombstones are permanent, including rejected/held requests.
Changing payload, expected version or epoch requires a new request ID and does
not reopen an aborted operation or create a second payout attempt. Recovery must
restore the complete terminal history. Backup rollback, authority succession and
old-epoch execution require fencing at the actual authority/custody boundary;
changing a local epoch number alone is insufficient.

## 3. Authorized resolution after financial commit

Preparation abort does not resolve an already committed payout. The existing
admission simulator intentionally keeps that uncertainty permanent. Before adding
a terminal resolution API, define independently accepted resolution authorities,
their mandates, evidence rules, disputes/stays, and customer claims for each live
deployment. Identity/reputation evidence is reusable; it is not a financial grant
or authority to release funds. Participant identity presentation remains QR-only.

| Proposed resolution | Evidence and required transition |
| --- | --- |
| Confirmed delivered payout | Independently authorized final execution evidence binds the original attempt, exact destination/currency/amount and final external execution ID; discharge covered liability once |
| Certified non-execution | Authoritative no-effect evidence plus a fence against all delayed execution of the original attempt; only then may authorized reversal/release occur |
| Delivery/no-delivery report or timeout | Evidence only; preserve the hold, with no automatic retry or release |
| Conflicting evidence or dispute | Preserve affected allocation, retain all evidence and route to the named resolver; no seizure by the observer |
| Refund while original execution is unknown | Establish the refund obligation and its funding independently; do not manufacture free collateral by clearing the original hold |
| Award or replacement security | Verify final/stay-cleared authority, exact residual claim, cumulative recoveries across all adapters and enforceable replacement/assignment before releasing original coverage |

Every accepted terminal resolution needs a permanent idempotency record bound
to the original operation, attempt, allocation, evidence and authority epoch.
Financial reversal/discharge and resolution consumption must commit atomically
using the existing exact per-currency ledger. Repeated evidence cannot produce
another debit, refund, recovery or release. Distinct award IDs must still share
one aggregate remaining-claim limit. Exact accounting never establishes actual
cash delivery, asset control or legal finality by itself.

These are implementation prerequisites, not implemented payout permissions.
No development mock, self-issued certificate or deployment flag may bypass them.

## 4. Executable test slice

`hawala_commit_sim::coordination::TestCoordinator` owns an isolated SQLite file
with one immutable test binding: transfer, terms digest, two distinct responsible
dar identities, two distinct allocation IDs and epoch. Its inputs are explicitly
synthetic local events. `ConfirmLeft/Right` set historical reservation flags;
they neither call a provider nor verify/control funds. `ReleaseLeft/Right` only
set test flags after final abort. Returned receipts are local test records and
cannot serve as custody or financial certificates.

State and per-request receipts commit in one `BEGIN IMMEDIATE` transaction with
WAL and `synchronous=FULL`. Every nonzero request ID pins its exact canonical
command bytes and original result, including wrong-scope, stale and held results.
Identical replay returns the original result marked `replayed`; callers query
`state()` for current state. Payload changes conflict. New changes require the
pinned binding/epoch and current version. A rejected request cannot be edited and
retried under its old ID. This has no epoch migration/re-enrollment API.

Two independent connections can race, but at most one terminal decision wins.
Failed receipt insertion rolls back the state write. Reopen and abrupt exit after
commit preserve the decision and receipt. Restore rollback, disk corruption,
host compromise and distributed consensus are outside the test guarantee.

There are no asset amounts or financial postings in this protocol slice; the
existing admission verifier and reference ledger remain the only test admission
and accounting implementations. This module is deliberately not wired into them.
Human/machine QR verification and Iroh behavior are unchanged. A production
integration must prove that reservations cover the exact admission binding;
this simulator cannot authorize that integration.

## 5. Proofs and validation limits

`formal/lean/Coordination.lean` proves safety for arbitrary finite reachable
event sequences: commit requires both reservations, committed reservations cannot
be released, release requires final abort, terminal decisions never change, and
abort fences commit. Timeout stutters. No progress/liveness is claimed under a
partition, and no signature or custody-control assumption is proved.

Quint/TLC exhausts the 14-state abstract protocol. Three deliberately unsafe
mutations (commit after abort, release on timeout, commit on one reservation)
must produce counterexamples; infrastructure failure does not pass that gate.
Lean checks graph closure and event-alphabet completeness and exports 126 edge
paths. Quint independently replays them; Rust replays every prefix through the
durable journal, with exact command replay and version checks. This is finite
projection conformance, not universal Rust refinement or semantic equivalence.

Additional Rust tests cover a partition with late confirmation, permanent scope/
epoch/version rejections, request conflicts, competing commit/abort connections,
transaction rollback and abrupt process exit after either terminal decision.
Existing admission, balanced accounting, uncertain execution, signed recovery and
live Iroh tests remain mandatory. No test executes a real external payout.

Reproduce: `lake build` in `formal/lean`, then from repository root
`bash tools/check_trace_artifacts.sh`, `bash tools/check_coordination.sh`, and
`cargo test -p hawala-commit-sim --test coordination`. Install the pinned Quint
dependencies with `npm ci --prefix formal --ignore-scripts` first.
