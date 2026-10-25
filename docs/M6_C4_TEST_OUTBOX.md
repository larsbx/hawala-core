# Durable dispatch outbox — TEST ONLY

`hawala-commit-sim` now persists a synthetic outbox item in the original
admission transaction, alongside the reservation, accounting journal,
nonce/attempt tombstones, receipt and pinned reporting identity. No
external command, physical device integration or real custody adapter exists.

## State transitions

An admitted intent starts with one pending item. `claim_test_dispatch`
requires its exact transfer/attempt, authenticated cash-peer identity and
matching current authority epoch. It checks the admission timestamp and a
strict expiry cutoff: the earliest QR, interaction, eligibility, custody
assertion or machine-delegation expiry. These are synthetic inputs already
verified during admission, not current production operating grants.

One SQLite transaction changes pending → claimed and reserved → uncertain
before returning a non-Clone `SyntheticDispatch` value. Eight competing
workers therefore receive at most one value. A synthetic dispatch is not a
financial authorization or hardware capability; this crate has no executor.
Copying its identifiers into another application cannot confer authority.

A crash after claiming, even before handing the value to a mock, leaves
claimed/uncertain. There is no lease expiry, reclaim or retry API. This
chooses loss of availability over a second ambiguous dispatch. Pending items
can survive restart and be claimed once if still current. Inventory lists
pending, claimed/uncertain and held-before-dispatch states without producing
new dispatch values. Pending is only a storage status; it is not a statement
that current evidence, epoch or time checks will pass.

Expired items become permanently held/uncertain when a claim is attempted;
clock rollback cannot revive them. Reports or local uncertainty also block a
pending dispatch. Invalid peer/attempt rejects; wrong epoch or a clock before
admission holds. Historical intents are not backfilled into this outbox.

## Limits and recovery

Claim-once is not exactly-once external execution. A future independent
custody/device protocol must query and reconcile the permanent attempt ID,
verify current operational authority and actual asset control, and resolve
unknown outcomes without blindly resending commands. No customer claim or
collateral is released here. Signed delivery reports remain evidence only.
There is no production epoch handoff, backup rollback defense, distributed
consensus, or real cash command support.

Tests cover atomic enqueue rollback, atomic claim rollback, concurrent
workers, wrong scope/epoch, expiry, clock rollback, prior signed reports,
restart before/after claiming and abrupt process exit after a durable claim.
The existing Iroh/QR identity and delegated machine checks remain in place;
financial permissions remain separate from reusable identity/reputation.
