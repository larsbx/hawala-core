# M6: human cash handoff and ATM proof share a conservative journal

Status: **experimental signed evidence**; no real money execution.

## Principle

All transaction modes use an accepted, dual-hawaladar signed Hawala
obligation, persistent QR identities + readable names/codes, and a
short-lived mutually signed interaction. The `hawala-flow` crate binds
the identities of human and/or machine parties to exact principals
under the accepted terms.

For a human cash payout, `hawala-human` provides a **separate** pair of
signatures, one from the beneficiary and one from the disbursing hawaladar,
over the same full terms digest, transaction ID, currency and amount,
one previously durably reserved attempt ID, a distinct receipt ID and
the parties' asserted handoff time. A machine's terminal report uses the
same attempt/obligation binding but remains device-specific telemetry.

No username, account number, phone number or card is required for
presentation. QR plus a readable name or code is required. A copied QR,
public short code, or static QR is not transaction-specific authorization.

## Durable journal behavior

- A human-cash attempt uses `begin_attempt` (no bound ATM).
- An ATM-cash attempt uses `begin_atm_attempt` (terminal bound).
- A co-signed human handoff may be recorded ONLY for the exact previously
  persisted non-ATM attempt, after the independent signatures verify.
- The record is immutable and idempotent; two conflicting signed handoffs
  for one transfer are rejected. Rebooting does not reopen an attempt.
- Recording even a mutually signed handoff moves an in-progress attempt
  to **UNCERTAIN** rather than marking it settled or enabling another
  cash release. Additional custody reconciliation and dispute procedures
  determine real fulfillment and any balance changes.
- An ATM-bound attempt cannot accept a human handoff instead of its
  device evidence. An unbound human attempt cannot accept ATM events.

This deliberately preserves a conservative separation between
*someone asserted cash changed hands* and *the network has verified
its economically and legally final outcome*.

## Present limitations

Neither signature proves the person's physical presence, honest cash
counting, KYC, regulatory eligibility, third-party witness, nor that a
cryptographic key is hardware-protected or presently possessed by the
rightful owner. Two colluding actors can sign fictitious receipts.

The `hawala-interaction` signed proposal nonce and a human QR-authenticator
challenge still need atomic replay consumption together with the financial
reservation. The current journal's low-level methods do not validate
custody, deposits, liquidity or legal permissions. Until those gates exist,
this is NOT a live money transmitter or ATM application.

## Next integration gates

1. Current signed operator/device/agent authority and verified collateral
   policies; fee/currency beneficiary acceptance.
2. Atomic ledger reservation plus single-use authenticator QR and financial
   idempotency with a recovery/fencing protocol.
3. Human and ATM callback reconciliation into independently checked
   financial claims and custody inventory, including partial outcomes.
4. End-to-end Iroh simulated cash acceptance and withdrawal tests with
   human/human, mixed and machine/machine sessions.
