# Durable integrated admission — TEST ONLY

The `Simulator::admit` entry point integrates M6-C1/C2/C3 without adding a
network server or changing Iroh, identity presentation, or the two signed
dar obligations. `Admission::Committed` records a synthetic reservation,
never an execution capability or proof of cash delivery. There is no real
custodian implementation, hardware command, release or settlement method.

## Ordered state and atomicity

One SQLite BEGIN IMMEDIATE transaction owns the account snapshot, pinned
financial issuer comparison, signed admission revalidation, exact reference
Book validation, custody checkpoint comparison, test lot reservation,
custody version advancement, ledger postings, QR nonce, transfer/event/attempt
tombstones and receipt. A failed receipt write rolls back every earlier
write. WAL plus FULL synchronous preserves committed local transactions.
The existing process lock excludes another Simulator opening the same path;
threads serialize through the connection mutex. This is local authority,
not distributed consensus, replication or backup rollback protection.

Test lots bind their available/reserved liability accounts to exact bilateral
terms and a custody account/version/epoch. Each lot is reserved whole once;
split/merge and partial reuse are unsupported. Multiple lots cannot consume
the same custody checkpoint. Epoch handoff is deliberately unsupported:
stale epochs hold, and provisioning cannot overwrite existing checkpoints.

Exact economic replays return the original permanent receipt, including
after evidence expiry. This is retrieval, not renewed authorization.
Different economics under a committed transfer ID are retained as conflict
digests and rejected. Uniqueness constraints fence nonce/event/attempt reuse.
Replay does not clear `uncertain`, advance a version, or repeat postings.

`mark_uncertain` durably moves reserved → uncertain; repeated uncertainty is
idempotent. There is no transition from uncertain to released or delivered,
and no function that dispatches a payout. A restart retrieves the same
intent and preserves the liability hold. All observations of external
execution remain outside the authority until a future adjudication design.

## Custody boundary

`CustodyAuthority` is trusted local adapter configuration, never peer-selected.
`SignedTestCustody` uses the existing custody-model verifier for either pinned
independent escrow or fault-intersecting federation policies. It adds bounded
test capacity, expiry and current version checks. Insufficient quorum or a
partition holds; invalid signatures reject. `UnsupportedRealCustody` always
holds. Neither adapter proves actual assets, controller independence or a
legally effective mandate. Controllers must not perform side effects in
`verify`; all simulated ordering is in the journal transaction.

The caller must locally pin `Authorities` separately from the envelope.
Existing admission/QR verifiers validate signatures, terms, fees, scope,
expiry and machine delegations again at admission. Identity/reputation is
not elevated into financial authority. ExternalReviews are still synthetic
control reports, not independently verified production grants or quote
acceptance. Do not expose this API to customers as a financial service.

## Scope and release gates

This increment handles one destination-currency liability reservation with
synthetic funding/provenance. It does not implement source-currency FX
settlement, transferable live credit, real custody reconciliation, current
legal/operational grants, authenticated authority receipts, epoch succession,
external command outbox, or end-to-end Iroh fault testing. Those remain
required before production acceptance. The older `record_simulated_hold`
entry point remains a low-level compatibility experiment and cannot create
integrated admission receipts or attach provenance to an earlier legacy hold.

Tests cover integrated signed envelope checking, unsupported custody, missing
quorum, stale versions/epochs, invalid quote, concurrent identical requests,
conflicts, expired replay retrieval, durable uncertainty, restart recovery,
and rollback after accounting/nonce writes but before receipt completion.
