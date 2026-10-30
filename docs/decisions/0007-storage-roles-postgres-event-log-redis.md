# ADR 0007: TigerBeetle financial commit, PostgreSQL business history and Redis operations

Date: 2026-10-30 (America/New_York).
Status: **Selected storage-boundary specification; adapters and production cutover gated.**

Companion: [biz-kit PostgreSQL event-log and Redis consolidation ADR](https://github.com/larsbx/biz-kit/blob/main/docs/architecture/2026-10-30-postgres-event-log-redis-consolidation.md).

## Context

The selected [node-owned books specification](../NODE_BOOKS_TIGERBEETLE_SPEC.md)
places financial acceptance in TigerBeetle and independently persists outcomes
afterward. Biz-kit's existing Elixir EventStore is already PostgreSQL-backed.
Replacing that library is a host implementation change, not a change of financial
authority or a second financial commit.

This decision supplements [ADR 0006](0006-biz-kit-extension-host-boundary.md)
and the node-books specification. It preserves ADR 0004 D1–D6, the separately
Proposed ADR 0005 N1–N7 and unresolved G1–G11 obligations. It neither closes
those obligations nor activates financial execution. Older SQL-first proposals
remain explicitly historical.

## Decision

### S1. Preserve the financial commit boundary

The gateway verifies current signatures, participant consent, scope/policy,
required evidence and effective fencing. TigerBeetle alone decides native
financial acceptance and balances for its declared authority scope. Submit the
complete linked operation, including every supported spend-critical uniqueness,
reservation and nonce guard, under permanent original operation/transfer IDs.

A confirmed complete linked operation is the financial commit. The financial
reply MUST NOT depend on a PostgreSQL intent/journal, business-log append,
SQL outbox, Redis write, mirror verification or downstream ACK. Retain exact
original request/evidence bytes durably with their designated owner before
submission; a hash or pointer is insufficient. Refuse unsupported guard/evidence
requirements rather than hiding a SQL-first financial step inside this profile.

A missing response or absent lookup after interruption remains Held/Unknown.
Recover or retry the identical original IDs/body under current authority; do
not manufacture a fresh payout ID. Financial acceptance does not prove custody,
cash delivery, business fulfillment, execution or final settlement.

### S2. PostgreSQL can replace the host EventStore library behind signed admission

Biz-kit selects a staged, narrow PostgreSQL log behind `CoopSubstrate.Log`.
Until its conformance gates pass, the existing EventStore backend stays active.
All signed business outcomes must still pass canonical Log admission; neither
a Rust sink nor a SQL projection may insert unsigned event rows directly.

The host adapter must preserve:

- Exact canonical full-record bytes, original IDs/signatures/profile versions,
  global/per-stream hash chains and independently verifiable checkpoints.
- Current admission/guard checks bound to the committed head, atomic batches,
  expected-head concurrency refusal and effective stale-writer fencing.
- Permanent source-ID/content binding: identical replay returns the original
  admitted outcome; changed content under that identity retains a conflict.
- Bounded replay of a verified complete committed prefix; raw SQL sequence
  allocation, timestamps and maximum observed IDs do not prove completeness.
- Atomic complete consumer bundle, explicit processed receipt and cursor where
  co-located; reviewed no-op types, gap refusal and home/consumer isolation.

Preserve the existing ordered business ledger per home. Removing its canonical
serialization or changing its bytes requires a separate protocol decision.
Keep indexed transactions short; do not perform remote effects under a log-head
lock. Sharing a database name does not give separate connections shared rollback.

This specifies the biz-kit host. Hawala's transport-independent
`FinancialPort`, `CommitSource` and `DurableSink` contracts remain independent
of PostgreSQL, Redis and the Elixir library. Non-biz-kit peers and the existing
opt-in durable SQLite TEST archive remain supported within their stated profiles;
this ADR does not force a new database dependency into the financial core.

### S3. Each downstream destination owns durable recovery

Replay complete native operations with original scope/epoch, cluster/generation,
registry/compiler context, leg count/order and retained signed evidence.
Neither one CDC leg nor an equal net balance establishes the complete operation.
Destination cursors remain scoped to `(destination, cluster_id, generation)`.

Commit the whole derived bundle, immutable source binding, processed receipt and
cursor atomically in the destination where possible. If signed Log admission
and cursor update cross transactions, source-bound idempotency must recover an
append-before-cursor crash before acceptance. ACK only after durable completion.
Do not advance past partial groups or infer a prefix from a maximum timestamp.

Native-source recovery must cover missed messages, Redis loss, destination
outages, restart and stale backup restore. A destination may lag independently;
ordinary lag does not block unrelated TB admission. Explicit evidence-retention
or capacity policy can apply scoped backpressure. Replayed financial outcomes
never resubmit financial commands or write stale SQL balances back into TB.

Consolidating PostgreSQL event/history and projection tables does not produce
independent retained copies. Preserve required independent destinations,
checkpoints and backups. Privileged database rewrite/rollback requires external
anchors and continuity checks, not just append-only application permissions.

### S4. Redis carries operational work and derived views

Redis may provide Streams delivery/reclaim, atomic due-work claiming, delayed
retry scheduling, throttling, presence, geospatial discovery and shared
versioned views within one installation's trust domain. It is not required
for financial submission. Online status does not prove liquidity, custody,
consent or permission to execute.

Biz-kit migrates covered jobs away from Oban/AshOban and covered Kafka/custom
delivery paths to one scheduler/worker contract. It retires Eatsbot and
SpruceGoose hosting after capability/history/client and writer cutovers. Those
are host consolidation obligations, not Hawala protocol dependencies.

Durable intent and receipts reconstruct outstanding work after broker loss or
lost publication. Redis leases cannot establish permanent replay consumption,
financial locks, authorization or at-most-once external effects. Stable effect
IDs, current authorization, original attempt binding and real execution fencing
remain required at the owning authority. Notifications only wake durable replay.

### S5. SQL optimization cannot weaken financial or business gates

The host should fix surviving audit-head indexing/lock duration, batched status
reads, bounded board/trip queries, location-batch prefetching, identity/consent
lookups and bounded retention cleanup. Record query counts/plans, lock waits,
latency and recovery cost before claiming improvements.

These are host SQL concerns. Do not migrate balance decisions into PostgreSQL,
cache away consent/revocation checks or remove durable effect/idempotency records
to obtain fewer queries. Signed financial reports disclose their source
watermark and remain nonauthoritative for spending.

## Consequences and alternatives

A narrow PostgreSQL host log can remove library/configuration overhead and make
local transactions and indexes explicit. Today's EventStore already supplies
PostgreSQL storage, expected-version checks, persistent subscriptions and
transaction participation, so replacing it alone promises no performance gain.
The host assumes ownership of relevant replay/concurrency/recovery semantics.

Keep the existing library until the replacement proves compatibility; do not
replace it with an equally large general framework. Redis-only business history
and PostgreSQL-first financial commit are rejected. Required independently
durable evidence cannot disappear during database consolidation.

## Acceptance obligations

| Gate | Evidence required for the actual adapter |
| --- | --- |
| S-A1: history compatibility | Byte-identical migration/export; signatures, both chains, original schema/profile and external checkpoint verification |
| S-A2: admission/concurrency | Independent writer races, stale heads/grants, batch failure, duplicate IDs and changed-body refusal; no partial admitted batch |
| S-A3: complete replay | Reordered SQL allocation/commit, unknown types, cursor races, partial native groups and explicit no-op receipts preserve complete prefixes |
| S-A4: crash recovery | Inject failure around append, destination commit, cursor and ACK; restart/stale-backup recovery under the declared durability/failover profile |
| S-A5: Redis destruction | Flush/restart, lost/duplicate/reordered wakeups and backlog reconstruction preserve financial outcomes and external-effect deduplication |
| S-A6: independence | One destination unavailable while another catches up; no projection can submit a payment or authorize payout |
| S-A7: refinement/cutover | Map the actual Rust/SQL/Elixir transitions to Lean/Quint and run exact-revision checks; verify anchored history, fence old writers and demonstrate rollback without losing a new prefix |

Existing formal and synthetic TEST acceptance applies only to its recorded
implementation and assumptions. This ADR does not prove PostgreSQL refinement,
authenticated production native-history completeness, consent retention,
spend-critical guards, effective execution fencing or live custody. Preserve
all existing production release gates. Cross-repository source pins must be
updated in implementation PRs when their pinned source actually changes; a
documentation decision does not silently upgrade a host dependency.

## References

- [Node-owned books specification](../NODE_BOOKS_TIGERBEETLE_SPEC.md).
- [Elixir EventStore API](https://eventstore.hexdocs.pm/EventStore.html).
- [PostgreSQL NOTIFY](https://www.postgresql.org/docs/18/sql-notify.html).
- [Redis Streams](https://redis.io/docs/latest/develop/data-types/streams/).
