# Node-owned books: TigerBeetle first, persistence afterward

Selected architecture: 2026-10-28. Implementation status: **port-level core and
synthetic conformance tests; native financial writer and durable sink adapters
not implemented or enabled**. This replaces the PostgreSQL-first proposal in
[the historical profile](NODE_BOOKS_POSTGRES_FIRST_ARCHIVED.md). Existing SQLite
simulators, production gates and financial authorities are unchanged.

## 1. One financial record, then downstream persistence

**Record the financial operation securely in TigerBeetle once. Its accepted
complete linked chain is the financial commit. Persist financial outcomes to
EventStore, PostgreSQL projections and other destinations afterward.**

No per-operation PostgreSQL intent append, SQL financial fold, outbox insert,
EventStore outcome append or downstream acknowledgement is required before
reporting TigerBeetle financial acceptance. The synchronous path checks current
business authorization and builds the exact native request, then waits for the
TigerBeetle decision. It does not wait for downstream persistence.

"Once" means one financial effect under permanent original IDs. The cluster
still replicates durably; retries and downstream at-least-once delivery are
expected. Do not replace durable recording with a memory queue, a socket send,
a broker acknowledgement, or an unconfirmed `exists` response.

| Data / decision | Authoritative owner |
| --- | --- |
| Financial account state, reservation/posting and acceptance | TigerBeetle for the declared authority scope |
| Business grants, participant consent, signed terms and nonfinancial facts | Their existing signed authorities and Log/EventStore admission path |
| Signed record of a confirmed financial outcome | Existing Log/EventStore, referencing the original TigerBeetle commit |
| Financial SQL reports, indexes and workflow views | Derived persistence; may lag without changing financial acceptance |
| Actual execution and settlement | Separately authorized executors, observers and attestors |

PostgreSQL may physically back EventStore. These are not two additional
financial commits. A missing signed outcome is a recoverable persistence lag,
not permission to submit another payment. Business workflows needing that
signed outcome wait locally; financial acceptance itself does not wait.

## 2. Authority, registration and evidence

Each independently governed installation retains its own history, financial
data and local copies. One effective named authority controls each scope,
legal book, lot and collateral allocation. Operating a node, mirror, consumer
or TigerBeetle replica confers no spending authority. Replica membership stays
inside one declared operational/confidentiality trust domain. Independent
authorities retain separate clusters/books and the existing Hawala bilateral
coordination contract; there is no network-wide shared spending balance.

The trusted financial gateway MUST reverify current participant consent,
grants, quote, expiry, obligation/custody context and effective writer epoch.
TigerBeetle enforces native accounting constraints; it does not interpret
signatures, business policies or physical custody. It is reachable only through
the trusted gateway. Revocation/succession MUST fence actual access; a cached
grant or higher epoch number alone is insufficient.

Registry and compiler configuration are provisioned durably before use and
versioned immutably. Full canonical identities map injectively to valid native
IDs; never truncate hashes. Account creation is separate from transfer creation
and MUST be verified before dependent transfers. This provisioning is not a
per-transaction PostgreSQL commit. No arbitrary opening balances are accepted.

The caller retains original request IDs and exact request bytes before send.
This client recovery requirement does not make a server-side SQL write the
financial boundary. The same business operation MUST reuse its permanent
operation and transfer IDs across calls, restarts and business retries.

**Evidence limit:** TigerBeetle has a fixed financial schema, not arbitrary
signed-payload storage. CDC cannot recreate consent/signature bytes never
stored there. Exact original business evidence MUST already be durably retained
by an existing evidence owner, or be losslessly recoverable from the native
record under a reviewed bounded encoding. A hash/pointer alone is not retention.
The TB-first fast path MUST refuse operations that cannot satisfy this rule;
do not secretly add a required PostgreSQL prewrite and call it record-once.
An EventStore signer may later attest the observed accounting outcome; that
attestation is not a reconstructed participant signature or native TB certificate.

## 3. Complete financial commit

Build one immutable operation plan identifying authority/scope/epoch, native
cluster/generation, registry/compiler versions, operation and original attempt,
and every transfer in its deterministic order. The supported core initially
accepts ordinary posted transfers only; Hawala holds are posted
reclassifications between available and held accounts, with no native timeout.

The whole operation, including any spend-critical nonce, lot or uniqueness
guards, MUST succeed or fail as one TigerBeetle linked chain. Do not allocate a
second authority in PostgreSQL to consume a nonce before/after posting. If a
required predicate cannot be enforced by the native state/guard-account design
and fenced authority boundary, that operation is unsupported until refined.
The new core does not implement that guard-account design or consent validation.

Keep the prior exact accounting requirements: integer minor units in
`0..=9223372036854775807`, positive posting amounts, checked u128 arithmetic,
immutable currency/scale mappings, per-currency conservation, distinct-account
postings, no cross-book balancing and no implicit FX. Use the deterministic
ledger/account sorted debit-credit pairing from the historical compiler
profile, retaining the entire multi-currency journal as one chain. Account
balance flags, profile caps and cumulative capacity MUST be checked by the
native adapter. Never split an oversized journal into independent requests.

The Rust core bounds groups to 255 transfers; a deployment pins its actual
server/client capacity and rejects smaller unsupported limits before send.
Every native transfer retains:

| Native field | Record-once representation |
| --- | --- |
| `id` | Original immutable injectively allocated transfer ID |
| First transfer `id` | Also the permanent native operation ID / group anchor |
| `user_data_128` | Permanent native operation ID shared by all group members |
| `user_data_64` | Authority epoch under the pinned cluster/scope registry |
| `user_data_32` | `total_transfer_count * 65536 + zero_based_index` |
| Account IDs, ledger, code and amount | Complete native accounting consequences under the pinned immutable registry |

Set `linked` on every transfer except the last. TigerBeetle does not retain
the linked flag, so the persistent group/count/index binding is essential for
recovery. The operation's first-transfer anchor MUST be in every retry chain;
`user_data_128` is not a unique constraint. Never mix unrelated operations into
one group, reuse an ID for a changed body, or treat an RPC batch as atomic
without linking its transfers.

Return `Committed` only after a confirmed complete accepted chain. Distinguish
`Rejected` (known no financial effect) from `Held/Unknown` (unresolved reply).
Native accounting acceptance does not imply payout, delivery, or settlement.

## 4. Persistence after commit

Trusted consumers read committed TigerBeetle history directly or through CDC.
CDC is a transport, not an authorization or financial-commit authority. Pin
supported native versions and authenticate/isolate the native reader and broker.
The documented CDC route uses AMQP 0.9.1, is at-least-once, supports replay and
requires compatible cluster/CDC versions [T1]. No new broker is mandated here.

Reassemble and validate the complete persisted operation group before publishing
an outcome: every unique index in `0..count`, identical group/epoch context,
exact registry-bound native payloads and strictly increasing native timestamps.
Never advance a consumer past a partial group. The trusted source MUST provide
complete native-history coverage, not silently skip unknown or missing groups.
Linked-chain membership cannot be inferred from a flag that was not retained.

Every destination has an independent cursor keyed by destination, cluster and
generation. In one destination transaction, persist the complete derived bundle,
original group/body binding and processed receipt, and advance its cursor.
For the signed business ledger, use the existing Log admission/signature path;
do not insert unsigned rows or re-emit a financial command. Use original source
identity for idempotency even if the local signed envelope has its own ID/order.
If business append and operational cursor cannot share one transaction, append
idempotently under a source binding and advance the cursor only after durable
append; demonstrate recovery across that boundary before accepting the adapter.

Broker ACK follows durable consumer completion. An exact duplicate is a no-op;
changed payload under the same source identity is a retained conflict. A cursor
tracks a complete source prefix, not merely the largest timestamp received.
Backfill from native history covers broker outages, lost notifications, process
death and new destinations. One destination's lag does not block another.

Ordinary persistence lag does not block new valid TB financial operations.
Explicit capacity/evidence-retention limits may backpressure the affected scope;
that is an operational safety policy, not a second financial commit.
Reports must disclose their source watermark; spending never relies on them.

## 5. Recovery, disagreement and external effects

| Failure | Required behavior |
| --- | --- |
| No confirmed TB reply | Held; retain original plan and query/retry only the same IDs |
| Lookup finds no transfers after interruption | Not proof of rejection; original request may still commit; retry identically under the valid authority boundary |
| Exact complete group exists | Recover financial commit; continue independent persistence without reposting |
| Partial group, changed native body or missing registry context | Preserve conflict, hold affected scope; do not patch with fresh financial IDs |
| TB commit precedes all sink writes | Financial commit survives; source replay discovers and persists it |
| Sink commits but ACK/reply is lost | Read durable sink cursor/receipt; deduplicate replay |
| EventStore/PostgreSQL unavailable | Financial result stays committed; dependent business transitions may wait; copies catch up |
| Destination restored from stale backup | Replay from its verified prefix; never write stale SQL balances back into TB |
| Native cluster loss or uncertain restore | Follow pinned TB recovery/anti-rollback procedure; HOLD until financial history continuity is established |
| Payout/settlement uncertain | Retain original attempt/encumbrance; no timeout release or duplicate execution |

One operation may be financially committed while execution is blocked. Release
requires current authorization, fencing, original attempt binding and the
business workflow's required evidence. Downstream consumers never create
execution permission merely by processing a ledger event. Business rejection
after financial acceptance requires an explicitly authorized correcting
operation, not a retroactive abort or deletion of the accepted transfer.

Full inventory/counter/registry comparisons at consistent native checkpoints
remain required for startup, restore and reconciliation. Net balances alone
are insufficient. SQL/EventStore mirror disagreement is investigated against
native financial history and independently retained evidence; do not silently
overwrite either side. Rebuilds need complete financial source continuity,
declared generation and effective fenced cutover, not blind replay of a stale
business projection. Separate legal books and bilateral prepared holds retain
their existing coordinator/recovery contracts.

## 6. Implemented core and acceptance boundary

[`hawala-tigerbeetle-flow`](../crates/hawala-tigerbeetle-flow/src/lib.rs) contains
transport-independent `record`, `recover` and `persist_once` operations.
Its `replay::HistorySource` implements `CommitSource` over a trusted native
read port, with bounded complete-group decoding and destination-cursor restart.
No SDK-specific reader or real durable database sink is connected yet.
Financial submission has no sink argument. Trusted FinancialPort, CommitSource
and DurableSink contracts make native authentication/fencing, source completeness
and destination atomic durability explicit. Structural validation does not
establish those trust properties. Tests use synthetic native and sink ports;
they are not real TB, PostgreSQL or signed EventStore tests and are not a
Lean/Quint financial-storage refinement. The added abstract Lean sources and
exporter still require pinned Lean compilation: local startup failed before
elaboration on sandbox process-path access. Quint/TLC exhausts the separate
100-state projection; 1700 certificate edges match Quint and the Rust port
harness. The independently constructed certificate must be reproduced by Lean
in CI before any Lean-generated or kernel-checked certificate claim.

Required before runtime integration: pin native SDK/server/deployment versions;
implement and test the scoped gateway and guard/account compiler; define complete
evidence retention and immutable registry bytes; adapt real native outcomes and
source replay; implement real durable sinks and signed outcome admission; test
concurrent nonce/lot use, lost replies, all commit/ACK crash boundaries, same-ID
changed-body refusal, partial CDC groups, cursor races, generation changes and
effective stale-writer fencing. Extend generated-trace/formal refinement for
TB financial commit and independent persistence prefixes. Existing D1–D6,
N1–N7 and G1–G11 are not silently discharged by this new storage direction.

## 7. Vendor references

Behavior checked 2026-10-28. These references establish vendor primitives, not
the complete financial authority/evidence protocol proposed above.

- [T1 Change data capture](https://docs.tigerbeetle.com/operating/cdc/)
- [T2 Linked events](https://docs.tigerbeetle.com/coding/linked-events/)
- [T3 Transfer schema](https://docs.tigerbeetle.com/reference/transfer/)
- [T4 Reliable submission](https://docs.tigerbeetle.com/coding/reliable-transaction-submission/)
- [T5 System architecture](https://docs.tigerbeetle.com/coding/system-architecture/)
- [T6 Recovery](https://docs.tigerbeetle.com/operating/recovering/)
