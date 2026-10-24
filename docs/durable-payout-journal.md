# M5-A — local durable Hawala payout intent journal

Status: **experimental safety building block**, not a money transmitter,
cash dispenser, account ledger, or system of record for real customer funds.

`hawala-journal` provides a **single-authority** crash-recoverable SQLite
record of one bilateral Hawala transfer's payout intent, keyed by the
unique transfer ID from a doubly signed `SignedTerms`. It does not create
funds, debit balances, or provide external financial authority.

## State transitions

```text
 dual-signed SignedTerms
          |
      registered --------------------> cancelled
          |
   [one durable attempt ID]
          |
      dispensing
          |
  crash / uncertain hardware outcome ---> uncertain
          |                                  |
          +--------- dual payout attestation -+
                                         |
                                   payout_attested
```

Payout **attested** means the disburser and beneficiary signed evidence;
it does NOT by itself prove physical cash delivery or bank finality. No
`settled` phase is inferred, and no automatic second payout can be
initiated from dispensing/uncertain/attested/cancelled state.

## Mandatory crash boundary

1. A separate Hawala authorization engine must verify live compliance,
   agent eligibility, bilateral trust, funded assets and credit exposure.
2. The adapter allocates a unique opaque attempt ID and calls
   `begin_attempt()`; the journal commits `dispensing` *before* any
   external cash-dispense command is submitted.
3. The adapter may submit exactly one hardware command with that attempt ID.
   The journal itself exposes **no** hardware adapter and never calls it.
4. A process restart migrates residual `dispensing` states to
   `uncertain` with append-only event records.
5. On `uncertain`, externally verified physical-dispenser outcomes or
   authorized dispute resolution are REQUIRED before any new command.
   The journal intentionally provides no `retry_cash` API.

## Operational cautions

- A sibling `.lock` file must be exclusively held by the one active local
  authority. Non-local shared filesystems are unsupported.
- SQLite WAL and synchronous FULL require functioning fsync and correct
  storage. Backup rollback is a protocol risk; immutable audit checkpoints
  and storage encryption are future work.
- Crash-migrated states remain uncertain until explicitly reconciled, even
  if no cash was physically dispensed.
- The journal does not implement a double-entry financial ledger, actual
  escrow/custody, verified dispenser outcomes, cross-node consensus,
  transaction-bound wallet authorization, partial withdrawals, AML/KYC or FX
  execution. None of these is inferred from `SignedTerms`.
- This SQLite store is NOT the identity nonce guard from M4-C. Identity replay
  protection and money execution idempotency must remain separate.
- The `events` table records local state changes but is not itself a
  cryptographically tamper-evident externally audited log.

## Verification

Tests simulate interruption between durable dispense intent and any
hardware acknowledgment, reopen the DB and reject a second attempt.
Further tests cover duplicate registration, modified signed terms,
cancelled-intent replay, signed payout attestations, and exclusive
single-process authority locking.

**Next gate:** real dispenser adapter must return trustworthy
`no_cash_dispensed`, `cash_delivered` or `uncertain` outcomes, with
operator-signed evidence and durable reconciliation; a double-entry
obligation ledger and policy engine must precede real funds.
