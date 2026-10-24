# M6-C2 — synthetic atomic reservation journal (NO REAL FUNDS)

**Status: experimental simulation, never a wallet or production authority.**

`hawala-commit-sim` demonstrates a single crash-durable SQLite commit
covering a *synthetic* customer liability hold, a temporary QR nonce,
unique transfer/event/attempt IDs, and a monotonically increasing local
authority version. It deliberately exposes NO cash execution function.

## What the demonstration proves

A prepared `PreviewPlan` from M6-C1 is tied to an exact before-balance
checkpoint of the involved accounts (the plan type now uses private fields
and read-only accessors). On the local authority's SQLite file, one
`BEGIN IMMEDIATE` transaction checks:

- one fixed local state epoch and expected account state version;
- exact account identifiers, currency and normal sides;
- available customer liability account DEBIT, reserved liability account
  CREDIT, exact same positive payout amount; asset account is never debited;
- the pre-reservation account-balance checkpoint;
- one unique event, QR nonce and one unique disbursement attempt;
- conflict/fork detection, permanent idempotency on identical replays;
- all entry rows, synthetic balances, nonce/event tombstones and new
  authority version commit **together**.

The state on restart is `reserved_no_execution`: the prototype cannot
call hardware, disburse cash, credit a merchant, or mark a beneficiary paid.
The single local lock prevents a second active process operating the same
file. It does NOT solve distributed quorum consensus or backup rollback.

## How it differs from real customer accounting

`seed_synthetic` installs arbitrary TEST balances. They may be internally
balanced but are NOT real deposits, spendable claims, customer funds,
financial licences, cash cassette inventories or escrow-controlled assets.
The test book contains a synthetic asset, an available liability and a
reserved liability, all denominated in a single currency. The hold simply
moves part of an existing *simulated* liability to a restricted subaccount;
it does **not** extinguish the customer claim or discharge a hawaladar.

The SQLite connection is local and no Iroh replication/consensus is
performed. Its `record_simulated_hold` consumes a typed preflight plan
but does **not** independently recheck signed controls at a real financial
commit or reconcile custody and funding. Never expose its result as a
withdrawal/ATM execution capability.

The independent escrow and federated-dar custody interface remains an
unimplemented operational dependency. Cross-currency transfer execution
requires booked FX inventory/clearing and exact source/destination legs;
this example handles only a single-currency payout reserve.

## Verification and limitations

Tests cover all human/machine modes, fresh commit and exact replay after
restart, double-attempt prevention through unique IDs, stale authority epoch,
changed account balances, unbalanced postings, wrong account roles,
single active writer and no partially committed failed operations.

This is **not** the production solution to M6-C: it demonstrates the local
atomicity requirement while intentionally refusing to claim authoritative
assets or externally executable money.

## Next gate

Implement a signed, independently anchored custody-confirmation adapter
with genuine mandate/asset-control checks, controlled issuer enrolment,
federated ordering where elected, fenced epoch succession, recovery from
persisted outbox commands, and end-to-end simulated Iroh human↔human,
human↔machine and machine↔machine flows. Independently audit those controls
before connecting any real cash terminal.
