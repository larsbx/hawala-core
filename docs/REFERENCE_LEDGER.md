# Reference double-entry accounting — deliberately not a money ledger

The `hawala-ledger` Rust crate is an **in-memory model** for exact balanced
accounting operations, not an application for minting credits, operating a
regulated wallet, settling Hawala obligations, or controlling ATM cassettes.

## Data model

Accounts have immutable 32-byte IDs, an explicit 3-byte currency and either
a debit-normal or credit-normal unsigned natural balance. Balances start at
zero and cannot become negative. Every posting specifies **exact nonzero**
minor-unit debit *or* credit, never both. All arithmetic is integer, and
intermediate currency totals use `u128` to detect overflow.

Every journal entry:
- has a unique nonzero event ID and 2..256 distinct account postings;
- references only registered immutable accounts;
- must balance `sum(debits) == sum(credits)` **for each currency separately**;
- must leave all account natural balances in `0..=u64::MAX`;
- is applied atomically **in memory**, after all checks pass;
- is permanently idempotent by event ID + canonical hash of sorted postings;
  identical replay is a no-op, different bytes with a reused ID fail.

A successful pair of entries can, for example, describe an asset-backed
customer liability and then its full or partial redemption. But the ledger
does not know whether the booked asset **actually exists**: a fraudulent
operator can invent balanced records. No proof of a cash deposit follows from
balanced accounting.

## Currency, FX, legal and custody boundaries

An amount of 100 USD can NEVER be added directly to a 100 EUR debit to
make a "balanced" entry. Cross-currency exchange needs independent debit and
credit legs in each currency, actual FX liquidity/control accounts, an exact
accepted rational-rate quote, fees and rounding, and a legal settlement
arrangement. This crate checks only the first mathematical invariant.

Double entry is not source-of-funds proof, liquidity or FX risk management,
jurisdictional permission, tax compliance, money-transmitter licensing,
beneficiary consent, agent deposit sufficiency or payment finality.

## What still blocks real funds

1. Persist balances and append-only journal transactionally with durable
   replay fencing and monotonic state versions.
2. Independently reconcile real cash/bank/escrow/federated custody with
   commitments from a trusted authority. Both custody modes remain supported
   by the existing separate `custody-interface.md` specification.
3. Atomically join signed transfer acceptance, rate/fee agreement, funding
   finality, current dar grants and limits, reserved collateral, the temporary
   QR nonce, and the single exact disbursement attempt.
4. Detect forged/suppressed bank or device results and reconcile ambiguous
   external execution before authorizing a release, reversal or retry.
5. Prove correctness across partitions and multiple devices under a declared
   quorum/authority fault model, with a security/legal review.

## Tests

Checks cover nominal issuance/redemption, exact replay after a balance is
fully spent, inconsistent duplicate event ID, invalid account/currency,
unbalanced journal rejection, multi-currency independence, overspend,
overflow and rollback of all balances on failure.

This crate is intentionally not wired into `begin_qr_cashout_intent`: doing
so prematurely would create a false impression of a financially backed
withdrawal. The actual next milestone is one appropriately audited,
durable custody/ledger authority boundary, not two independent databases
claiming one transaction is atomic.
