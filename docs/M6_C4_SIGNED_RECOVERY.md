# M6-C4 — signed peer observation and held-intent recovery (TEST ONLY)

This increment adds a bounded execution-observation frame and a live two-peer
Iroh conformance harness to the synthetic authority. It does not implement an
external payout command, hardware driver, release, settlement or production
custody. It exercises report transport and recovery after an already durable
test intent, not execution of a financial withdrawal.

## Scope established at admission

The same admission transaction that reserves the synthetic liability now
records the exact transfer, permanent attempt ID, authority epoch and QR
cash-peer identity. Reports cannot select a new execution principal or inherit
permission from reputation. Machine cash-peer identities still require the
existing signed delegation verified at admission. The origin and disbursing
dars remain the accepted obligors; no network-wide server is introduced.

Older intents without this persisted reporting scope fail closed. This change
does not infer their original cash peer from incoming reports or backfill it
from caller assertions. Existing intent and uncertainty records stay intact.

## Signed observation protocol

`SignedObservation` signs domain-separated bytes binding transfer, attempt,
observation ID, reporter identity, authority epoch, observation time, expiry,
and kind. The wire format has fixed magic, exact length (225 bytes), explicit
integer encoding and strict Ed25519 verification. Unknown kinds, malformed
identity keys, truncation and trailing bytes reject. New reports have a
maximum five-minute validity window and must be current at ingestion.

The receiving application supplies the identity established by the existing
QR/Iroh handshake. The report must match both that authenticated peer and the
cash peer pinned in the durable admission. Transport identity alone does not
grant financial permissions; the signed report independently proves origin.

All report kinds — Timeout, ReportedDelivery, ReportedNoDelivery — transition
the test intent to uncertain/Held. A delivery report does not establish cash
delivery, and a no-delivery report does not release collateral. Neither can
create another attempt. There is no automatic financial response.

## Durable ingestion and recovery

One SQLite transaction verifies the pinned attempt/epoch/peer, stores the
full signed frame, and sets uncertainty. Exact signed replay returns
IdenticalReplayHeld, including after expiry; it retrieves previously accepted
evidence rather than authorizing new action. Conflicting signed frames under
one observation ID are retained in full and return ConflictHeld. Both preserve
the hold. Bad signatures, wrong peers, unknown intents and stale epochs reject
without state changes. Invalid signatures are not retained as controller
conflict evidence.

Restart preserves the same attempt and reservations. Report ingestion never
changes account balances, authority version, custody allocations, ledger
postings or QR nonce tombstones. Failure during uncertainty update rolls
back the evidence insertion too. Existing crash-recovery tests still apply
to the underlying admission transaction.

## Verification

The live harness exchanges reports over authenticated Iroh QUIC for
human/human, human/machine, machine/human and machine/machine fixtures. It
uses explicit loopback addresses to avoid relay/discovery dependencies in
these tests. After connection closure and journal reopen, report replay stays
Held and one accounting event remains. Separate deterministic tests cover
forgery, wrong peers, wrong attempt/epoch, uncommitted intents, future/expired
reports, wire corruption, conflicting delivery reports and atomic rollback.

Remaining work includes durable execution outboxes, independently verified
custody assets and operational grants, epoch handoff, independently authorized
adjudication, sensor/human evidence reconciliation and deployment acceptance.
No test result in this milestone enables real money or ATM commands.
