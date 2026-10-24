# Security conformance and review checklist

These are deterministic protocol vectors. “Required integration” is not a claim
of executed tests. Implement each against the eventual ledger/custody adapter.

| ID | Fixture / action | Required result | Evidence here |
| --- | --- | --- | --- |
| Q01–Q12 | Shared TSV: fees, rounding, zero, overflow, maximum intermediates | Exact expected outputs/errors | Rust fixture test + independent Python oracle |
| C01 | Deposit 100; reserve 60; identical retry | 60 reserved, 40 free | Rust unit test |
| C02 | Reuse reservation ID with changed amount | Conflict, unchanged state | Rust unit test |
| C03 | Deposit 100; competing reservations of 60 in either serial order | Exactly one succeeds | Rust serial-order test, not distributed concurrency |
| C04 | Reserve 60; withdraw preview 41 | Insufficient; no mutation | Rust unit test |
| C05 | Release after verified discharge; replay original reservation | Closed ID, no reopening | Rust unit test |
| C06 | Reserve u64 maximum from same deposit | No overflow; next reserve rejected | Rust unit test |
| A01 | Fresh QR for terms T; replace recipient/rate/amount/audience | Reject each mutation | Required integration |
| A02 | Same valid QR submitted concurrently twice | One ledger mutation; retry status only | Required integration |
| A03 | Expired/future proof, generic TOTP, untrusted signer | No authorization | Required integration |
| L01 | Two issuances reference same funding ID | One credit only | Required integration |
| L02 | Reused transaction ID with different signed payload | Conflict, retain evidence | Required integration |
| L03 | Crash after commit before response; retry after restart | Same result, no new credit | Required integration |
| L04 | Partitioned peers submit conflicting spends | No two final commits; unavailable authority -> HOLD | Required integration |
| L05 | Settle dars while customer credit remains outstanding | Customer claim/backing not erased | Required integration |
| P01 | Cash release authorized; receiver acknowledgment times out | DISPUTED; no auto-refund/unlock | Required integration |
| P02 | Device retries release after uncertain cash execution | Status only, no second release | Required integration |
| P03 | Partial cash acknowledgment | Remaining claim preserved; no full success | Required integration |
| D01 | Allegation without final authorized award | No collateral confiscation | Required integration |
| D02 | Replayed award or overlapping recovery cases | No duplicate compensation | Required integration |
| D03 | Award 80, already recovered 30 | At most remaining 50 recoverable | Required integration |
| R01 | Old key signs two successors | Both signatures valid; dispute, no automatic winner | Rust continuity test; application HOLD integration still required |
| R02 | Lost key, witnesses supplied solely by claimant | No established continuity | Required integration |
| R03 | Forge witness signature or mutate context/scope | Reject cryptographic evidence | Rust continuity test |
| R04 | Offline views contain different succession claims | Preserve union and conflict; no history-completeness claim | Required integration |
| R05 | Valid continuity claim submitted to redirect payout | Reject absent separate financial authority | Required integration |
| REP01 | Colluding keys sign fictitious successful receipts | Signatures verify; no independence/physical-cash guarantee | Required adversarial scenario |

## Review findings addressed in the draft

- Time-limited QR alone does not prevent replay: bind exact terms/audience and
  consume nonce in the financial commit.
- Collateral cannot be freed just because dars settled while customer credit
  still exists. Each allocation tracks its actual covered duty.
- Cash timeout cannot unlock funds after possible delivery.
- Withdrawal is a pure preview in the kernel; no replay-unsafe external cash or
  custody action is represented as an idempotent in-memory method.
- Recovery cannot establish authority from a newly invented witness policy.
- Generic star ratings on main are not represented as financial payout reputation.
- Network-wide redemption preserves named obligors; this is not blanket
  mutual liability by every participant.

Review is an author security pass, not independent audit or religious approval.
Hosted Rust tests, ledger concurrency/crash tests, custodian enforceability,
Hanbali contract review and concrete wire schemas remain launch gates.

## Custody-mode conformance

Both supported custody modes must satisfy CU01–CU13 in
[custody-interface.md](custody-interface.md): replay/conflicting payloads,
reservation/withdrawal races, orphaned reservations, uncertain external payouts,
cross-adapter duplicate recovery, federation approval rules, old-epoch replay,
migration omissions, duplicate asset backing and independently anchored mandates.
These are required integration scenarios; no live adapter test is claimed.

## Continuity encoding fixture

Public test seeds are the byte n repeated 32 times (NEVER operational keys).
Predecessor uses seed 1; successor uses seed 2; scope is UTF-8 `test-peer`;
context is byte 3 repeated 32; nonce is byte 4 repeated 32. Signed bytes are
the existing `hawala-identity-core/v1` prefix and NUL, followed by u32
big-endian length-prefixed domain `continuity-claim-v1`, predecessor key,
successor key, scope, context and nonce, in that order. Existing identity
encoding is preserved. Tests construct expected bytes and verify deterministic
signing, missing successor rejection, deduplication, signer forgery, mutation,
malformed inputs and symmetric competing claims. No recovery-policy approval
or human identity proof is inferred from those tests.
