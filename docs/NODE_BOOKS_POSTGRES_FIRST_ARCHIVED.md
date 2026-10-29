> **Superseded storage proposal.** Retained for historical review only.
> The selected direction is [TigerBeetle first, asynchronous persistence](NODE_BOOKS_TIGERBEETLE_SPEC.md).
> PostgreSQL-first admission and its accounting-copy gate below are not the selected design.

# Node owned books with PostgreSQL and distributed TigerBeetle

Status: **Proposed implementation specification**, 2026-10-28. This profile is
not implemented or formally refined. It grants no financial authority and
does not change the existing synthetic SQLite adapters. Normative words below
describe requirements for this proposed profile, not current runtime behavior.

Each independently governed biz-kit installation maintains its own canonical
signed business history and deterministically derives its own books. Each
installation retains a complete local copy of its history, PostgreSQL journal
and projections, and its TigerBeetle accounting copy. Financial authority is
named per scope; operating a node does not confer that authority.

The proposed storage choice is **journal first, verified accounting copy**:
one local authority transaction orders financial facts; distributed
TigerBeetle reproduces and checks their accounting consequences. Independent
businesses communicate through Hawala over Iroh, without sharing a universal
database or gaining write access to each other's books.

## 1. Authority and compatibility

This specification extends [M6-C](M6_C_AUTHORITY.md),
[M6-C6](M6_C6_COORDINATION.md), [M6-C7](M6_C7_COORDINATED_ADMISSION.md),
[reference accounting](REFERENCE_LEDGER.md), and ADRs
[0004](decisions/0004-biz-kit-shared-identity-scoped-authority.md) and
[0005](decisions/0005-biz-kit-federated-hawala-integration.md).
It does not amend D1–D6, N1–N7 or close their paired acceptance gaps.

Exactly one authority orders each financial scope, credit lot and collateral
allocation. The authority may be the installation itself only after every D5
prerequisite is satisfied. Otherwise its local business ledger retains Hawala
evidence and a source-attributed mirror of the externally authoritative scope;
that mirror cannot issue, reserve or discharge financial claims.

Every authoritative financial record MUST identify `authority_id`, `scope_id`,
`authority_epoch` and the applicable mandate/policy digests. A peer's imported
record retains its source authority and source sequence. Local import order
does not become source order. No generic cross-ledger bridge is introduced.

## 2. Copies and deployment topology

| Component | Retained data | Write authority |
| --- | --- | --- |
| Canonical signed ledger | Exact event bytes, signatures, ordered hashes, accepted financial transitions and checkpoints | Existing serialized append gate for this installation and explicitly admitted scopes |
| Local PostgreSQL | Durable canonical ledger backing, atomic financial journal, replay tombstones, custody references, outbox, read models and verification checkpoints | Authority transaction for canonical facts; projectors for derived tables |
| Local TigerBeetle accounting copy | Immutable account registry and compiled accounting transfers for the installation's own books; external-scope mirrors remain explicitly separate | One fenced projector from accepted canonical events |
| TigerBeetle replicas | Copies of the same cluster database within an explicitly administered trust domain | TigerBeetle's cluster ordering, not independent local writers |
| Peer evidence store | Authorized signed obligations, receipts and source checkpoints | Local retention only; no mutation of the issuer's financial facts |

The canonical signed ledger is a logical history, not a third independently
committed database. For this profile PostgreSQL durably backs its atomic append
and reference financial fold. An exported signed copy is independently
replayable; an export cannot bypass the append gate.

Each installation MUST retain its own history and accounting data rather than
rely exclusively on a remote SaaS database. Distributed TigerBeetle means a
cluster serving that installation/authority domain, with a locally retained
replica and additional replicas in declared independent failure domains.
It does not mean one replica per independent business in a network-wide
cluster. Every replica carries its cluster's data, so replica operators belong
inside the same confidentiality and operational trust boundary.

TigerBeetle documents a recommended six-replica production topology and
cluster-wide ordered operations [T1]. Its current deployment documentation
fixes replica count at creation [T2]. The deployment manifest MUST pin actual
server/client versions, cluster ID, replica membership, locations, recovery
policy and fault assumptions. A single replica is a development profile only.
No per-node Byzantine consensus or independent financial authorization is
inferred from TigerBeetle replication.

PostgreSQL durability, backup and writer fencing MUST be specified independently
of TigerBeetle availability. Replicated accounting data cannot reconstruct
missing participant consent, custody evidence or canonical authority decisions.

## 3. One canonical financial commit

For a locally authorized scope, one serialized PostgreSQL transaction MUST:

1. Lock/fence the authority scope and read its current sequence and epoch.
   Require a verified accounting checkpoint at the current financial head and
   no recovery/divergence flag. The initial adapter permits at most one
   unverified committed financial event per scope; a concurrent next admission
   holds until that event is verified. Nonfinancial business events are not
   blocked by this accounting gate.
2. Reverify participant signatures, current grants, authority/custody versions,
   accepted quote, expiry, provenance, exposure and any required coordinator
   certificate against the exact proposed operation.
3. Apply the pure Hawala reference transition and per-currency accounting fold.
4. Atomically retain the event ID/body binding, nonce and attempt tombstones,
   reservations, postings, custody checkpoint advances and terminal decision
   where this authority owns that decision.
5. Append the exact signed canonical event and hash-chain link, allocate its
   immutable TigerBeetle mapping/transfer IDs, and persist the projection work
   and non-executable execution-intent record.
6. Advance the scope version and commit durably before returning a financial
   intent receipt. Signatures generated for an abandoned transaction MUST NOT
   escape as accepted receipts.

The existing append gate remains the sole writer; a new PostgreSQL service
cannot become a competing financial gate. The pure fold's balances used for
admission belong in this transaction. Async SQL read models, cached balances
and TigerBeetle queries MUST NOT substitute for those locked reference values.

TigerBeetle is not enlisted in this PostgreSQL transaction. Its later commit
does not decide whether the canonical intent happened. This preserves M6-C's
single financial boundary without claiming distributed ACID across stores.
Changing to TigerBeetle as the sole admission authority would require another
profile proving atomic nonce, policy, custody, obligation and decision handling;
it is outside this proposal.

A locally committed intent can be returned as `Committed` while accounting
verification is pending, but MUST carry an explicit non-executable verification
status. No execution capability is released at that point.

## 4. Deterministic events and money

The complete journal history and pinned rule version determine the book.
Identical state plus identical accepted event bytes MUST produce identical
postings, account mappings and next-state commitments. Network arrival order,
local wall-clock reads, database iteration order and fresh randomness are not
inputs to replay. Commit-time clock observations and externally verified
decisions are recorded facts; replay verifies their recorded bindings rather
than reevaluating history using today's policy or clock.

Amounts and natural account balances in this profile are integers in
`0..=9223372036854775807` minor units. Each posting is nonzero on exactly one
side. New native numeric fields use this common i64-safe range; unsigned
128-bit intermediates detect overflow. Rates use positive integer numerator
and denominator, with explicit rounding and independently recorded fee legs.
No binary floating point or implicit currency conversion is admitted.

The currency registry fixes currency code and minor-unit scale. Changing a
scale creates a new registry identity and an explicit migration, not an edit.
Existing signed Hawala bytes MUST remain verbatim; do not reserialize them to
fit this profile. An older object outside the profile's range is retained as
evidence but cannot be admitted through this adapter.

Wire and cross-language canonical-byte fixtures are a required implementation
gate. This document chooses the range but does not claim to close G10 or define
a replacement encoding for existing signed messages.

Canonical IDs retain their full bytes. A persisted injective registry maps
32-byte account/event identities to valid TigerBeetle u128 IDs; truncation is
forbidden. Allocate IDs deterministically from checked counters in canonical
order, persist before submission, exclude TigerBeetle's reserved values, and
retain mappings permanently. Retries and rebuilds reuse exactly those IDs.
Allocated IDs are identifiers, never secrets or authorization tokens.

## 5. TigerBeetle accounting compiler

The compiler MUST preserve the `hawala-ledger` model: immutable accounts,
2..256 distinct-account postings per entry, balance per currency, nonnegative
natural balances, exact event/hash idempotency, and all-or-nothing application.
General business accounts that need negative natural balances require a
separately specified chart/profile; they are not silently added here.

Every entry belongs to exactly one authority, scope and legal book; all its
accounts MUST match that tuple. Balancing two different legal books together
is forbidden even when their currency matches. A business operation spanning
legal books uses separate authorized entries and the applicable coordination
contract.

| Reference concept | TigerBeetle representation |
| --- | --- |
| Authority, legal book, currency and scale | Immutable registry mapping to a nonzero numeric `ledger` |
| Debit-normal account | `credits_must_not_exceed_debits` |
| Credit-normal account | `debits_must_not_exceed_credits` |
| Journal debit and credit | One or more equal-amount debit-account/credit-account transfers on the same ledger |
| One complete journal event | One explicitly linked transfer chain, including all currency legs |
| Held customer liability | Posted reclassification between available and held liability accounts |
| FX | Separate same-currency legs through the declared FX accounts, linked as one event |
| Original event association | Immutable mapping plus reference in supported `user_data` fields |

TigerBeetle's account flags and balances are documented in [T3], linked event
semantics in [T4], and multi-currency composition in [T5]. Business holds use
ordinary posted transfers in this profile. Native pending-transfer timers are
disabled: cluster timeouts must not release collateral or a liability held by
the Hawala protocol. A transfer posting does not establish physical delivery.

Compile each valid journal deterministically:

1. Group postings by registry ledger; sort groups by numeric ledger ID.
2. Within each group sort debit postings and credit postings separately by
   full canonical account ID in unsigned byte order.
3. Pair the current debit and credit with `min(debit_remaining,
   credit_remaining)`, emit that positive amount, subtract it and advance
   each exhausted side. Continue until both sides are exhausted.
4. Assign the event's already allocated transfer IDs in emitted order.
   Set `linked` on every transfer except the last; send the entire event in
   one request. Never split an oversized event across independent requests.
5. Verify the exact committed transfers and resulting balances before
   advancing the accounting checkpoint.

Each reference account appears on only one side of an entry. Its balance
therefore moves monotonically toward the validated final balance during
compilation; pairing cannot create a transient negative natural balance.
Per-currency totals establish that pairing exhausts both sides. Proving these
claims against the actual adapter and pinned TigerBeetle version is required.

Account registrations are projected and verified before dependent transfers.
Creation of accounts and creation of transfers are separate operations; an
unused registered zero-balance account has no financial effect. Registration
conflicts stop projection. No arbitrary opening balances are accepted.

The adapter MUST enforce its pinned batch limits, reference balance cap and
TigerBeetle cumulative-counter capacity. No wrapping, automatic netting of
cumulative counters, flag substitution or silent schema conversion is allowed.
Plan an explicitly fenced migration before capacity exhaustion.

## 6. Projection and independent verification

One effective fenced projector writes each accounting cluster. It processes
accepted financial events in source order and persists the exact request
bytes before submission. Uncertain responses are recovered by lookup and
identical retry using the original IDs [T6]. `exists` is not sufficient by
itself: compare all semantically relevant fields against the retained request.
Retain mismatches as conflict evidence and enter `HOLD`.

At checkpoint K the verifier MUST establish:

- The canonical signed chain is complete through K, including authority,
  epoch and account-registry transitions, with no conflicting accepted fork.
- Replaying its financial events with the pinned reference rules produces
  the retained journal and PostgreSQL financial projection through K.
- Every expected TigerBeetle account and transfer matches its compiled
  representation, with no extra accounting mutations through that checkpoint.
- Posted debit/credit counters and natural balances match the reference fold;
  pending counters are zero for this profile.
- The compared histories use the same K, compiler version, registry digest,
  authority epoch and projection-cluster generation.

Pause the projector at K while taking a consistent accounting snapshot, or use
a proved historical-snapshot mechanism. Comparing current TigerBeetle balances
against an older journal prefix is invalid. Startup/recovery performs a full
inventory comparison; uninterrupted operation may use incremental verification
from a previously verified checkpoint under exclusive fenced write access.
If that access assumption is lost, require full inventory again.

A signed verification record binds `authority_id`, `scope_id`, epoch, source
sequence/hash, journal digest, registry/compiler digests, cluster ID/generation,
accounting-state digest, verifier identity and the previous verification
record. It is an application attestation, not a native TigerBeetle consensus
certificate or proof of actual assets. Exact signing bytes, domain and golden
vectors MUST be specified before accepting such records across implementations.

An independent relying party verifies the pinned authority and verifier mandate,
exact signed evidence, source continuity and the state required by its workflow.
Hashes commit to disclosed bytes; they do not prove hidden postings are valid.
Full verification requires the authorized history, or a separately defined
proof system. Routing peers receive neither private books nor counterparties.

TigerBeetle CDC may assist monitoring and reconciliation, but its documented
at-least-once delivery requires deduplication [T7]. CDC arrival, broker delivery
and a PostgreSQL mirror update are not financial decisions or verification
certificates.

## 7. State and execution gate

Track financial state, accounting-copy status and external execution separately.

| Dimension | States and meaning |
| --- | --- |
| Canonical decision | Proposed; Prepared; Committed intent; terminal Aborted before commit |
| Accounting copy | Pending; Verified through K; Diverged; Recovery required |
| External execution | Not authorized; Authorized for original attempt; Observed or uncertain; independently resolved |
| Operational response | Rejected for provably no effect; Held for unresolved safety; Committed with explicit intent and accounting status |

`Held` does not rewrite a committed decision into abort. Payout observations do
not imply settlement. Settlement, discharge, refunds and compensation retain
the separate authorities and evidence required by M6-C6 and ADR 0003.

Before releasing a synthetic execution capability, the authority MUST use its
serialized gate to recheck current epoch, mandate and unresolved disputes, and
require verification through the committed intent's sequence. Persist release
of the same attempt-bound capability durably. Later business events cannot
erase the reservation. Unknown external execution remains encumbered.
No real executor is enabled by this proposed gate.

When the configured TigerBeetle copy is unavailable or diverged, new financial
admissions in that affected scope MUST hold. Already committed intents remain
committed and reserved while their projection catches up. Unrelated local
business events and independently healthy authority scopes can continue.
Projection lag never permits a fresh payout ID or automatic financial rollback.

Distinguish a command request ID from the permanent operation/attempt ID.
An identical command replay returns its originally retained result; a status
query reports the current operation state. Advancing a held workflow uses an
explicit authorized transition with a fresh command ID bound to the original
operation and attempt, never a replacement payout attempt or edited request.

## 8. Cross authority obligations

Node A cannot post into B's authoritative books. Both retain the exact mutually
accepted obligation and source-attributed evidence. A's due-to-B and B's
due-from-A are entries in different legal books, linked by the same obligation
digest, amount, currency and allocation references; they are not one globally
balanced database transaction. Disagreement remains visible and held.

Cross-authority preparation, commit, abort and resolution follow M6-C6's common
decision order. A scoped coordinator requires a pinned mandate, participant set,
keys, epochs and recovery/fencing rules. After prepare, participant timeout
cannot release its reservation unilaterally. Coordinator failure preserves
`HOLD` until the authorized decision or recovery process supplies verifiable
evidence. TigerBeetle replication supplies neither this coordinator mandate nor
atomic commit between separate clusters.

Authority succession MUST fence old PostgreSQL writers, projectors and actual
custody/execution adapters. The new epoch binds an accepted history checkpoint
and outstanding attempts. A higher local number alone is not fencing.
Restoring a backup requires comparison against independently retained/pinned
checkpoints; uncertain rollback enters `HOLD`. Two authorities cannot both
inherit the same unencumbered lot.

## 9. Recovery cases

| Failure point | Required recovery |
| --- | --- |
| Before canonical transaction commits | No accepted financial change; retry with the original bound ID |
| PostgreSQL commits before TigerBeetle submission | Intent and reservations survive; replay retained projection work; execution stays held |
| TigerBeetle commits but reply/checkpoint is lost | Look up exact original IDs, compare payloads and resume; no new transfers |
| Verification record is lost | Reverify the same prefix and retain an idempotent record; no duplicate execution capability |
| Conflicting transfer or unmatched balances | Preserve evidence; stop affected financial activity; investigate before repair |
| PostgreSQL restore loses a known accepted suffix | Fence the writer and recover accepted history; never overwrite newer accounting state with the stale backup |
| TigerBeetle replica loss | Follow the pinned vendor recovery procedure [T8]; do not format a replacement as a fresh cluster member |
| Complete accounting-cluster rebuild | Fresh cluster generation; exact replay; full verification and fenced cutover before execution resumes |
| Delayed payout report or partition | Preserve original attempt and encumbrance; reconcile under M6-C6; no timer-driven retry/release |

Corrections append authorized new journals referencing the original event.
They never edit historical events or delete TigerBeetle records. An accidental
extra accounting transfer does not authorize a compensating customer transaction;
the repair procedure must establish the canonical history and explicitly account
for the discrepancy before reopening the scope.

## 10. Conformance and release gates

Required model invariants are one authority per scope/lot; atomic admission and
nonce consumption; balanced books per currency; single event/body binding;
nonnegative capped balances; immutable terminal decision; ordered exact
projection; verified-prefix gating; no execution from a stale epoch; and no
release/retry from uncertainty. Liveness under partition is not assumed.

Extend Lean/Quint with canonical-prefix, projection-prefix and execution-gate
state, including two authorities and a distinct coordinator. Check crashes
between every durable boundary, replay/conflict, loss of projector fencing,
stale backups, partitions and succession. Generate conformance traces against
the real PostgreSQL append gate and real pinned TigerBeetle process. Existing
SQLite tests and abstract financial proofs do not establish this refinement.

| Acceptance case | Expected result |
| --- | --- |
| Replay accepted history twice | Identical journal/state and IDs; zero extra postings |
| Same ID with changed canonical bytes | Conflict retained; no second effect |
| Two concurrent admissions consume one nonce/lot | One ordered effect; other request held, rejected or exact replay |
| One compiled leg fails | Entire linked event absent; canonical reservation stays held |
| Multiple debits and credits, multiple currencies | Exact per-currency fold equals compiled counters and balances |
| Account registry or compiler version differs | Verification fails before execution |
| Amount 1 and amount 9223372036854775807 | Valid numeric boundary when all other account/capacity conditions hold |
| Amount 0, negative, 9223372036854775808, fraction or overflow | Reject as a posting amount; no financial effect |
| Projection unavailable after canonical commit | Same committed intent and reservation after restart; no execution |
| Extra transfer outside canonical history | Full inventory detects divergence and holds the affected scope |
| Old writer/projector runs after epoch change | Actual write boundary rejects it; no duplicate attempt |
| Bilateral peer is missing or has conflicting decision evidence | Retain prepared reservations and evidence; HOLD |

Unsafe mutations MUST produce their intended counterexamples: execution before
verification, unlinked transfer legs, retry with new IDs, changed-body duplicate
acceptance, timeout release, cross-authority direct writes, and accepting a stale
restore. Tool/runtime errors do not count as successful negative controls.

Implementation order: exact schemas and canonical-byte vectors; pure compiler
and differential accounting vectors; PostgreSQL atomic append/replay adapter;
TigerBeetle projector and full verifier; restart/partition/fencing harness;
formal trace refinement; paired biz-kit integration review. All D5 release
requirements remain prerequisites to actual custody or money movement.

## 11. Remaining implementation decisions

- Pin server/client versions and test their exact result, batch and recovery
  semantics. Commit the real deployment manifest rather than rely on current
  web defaults.
- Publish canonical bytes and domains for the account registry, compiler
  manifest, verification records and checkpoint exports; bind them to existing
  event schemas without changing old signatures.
- Prove PostgreSQL durable append, key handling, anti-rollback and single-writer
  fencing for the chosen deployment; specify how projector access is fenced at
  the actual TigerBeetle network boundary.
- Specify the deployed coordinator mandate and recovery policy before any
  distributed financial pilot; extend existing formal models accordingly.
- Review the biz-kit host contract in lockstep before claiming interoperability
  or changing the paired ADR texts. Hawala remains the owner of financial rules.

## 12. TigerBeetle references

Vendor behavior checked 2026-10-28. The choices in this specification are Hawala
design requirements; the references establish the underlying database behavior.

- [T1 Cluster recommendations](https://docs.tigerbeetle.com/operating/cluster/)
- [T2 Deploying](https://docs.tigerbeetle.com/operating/deploying/)
- [T3 Account reference](https://docs.tigerbeetle.com/reference/account/)
- [T4 Linked events](https://docs.tigerbeetle.com/coding/linked-events/)
- [T5 Currency exchange](https://docs.tigerbeetle.com/coding/recipes/currency-exchange/)
- [T6 Reliable transaction submission](https://docs.tigerbeetle.com/coding/reliable-transaction-submission/)
- [T7 Change data capture](https://docs.tigerbeetle.com/operating/cdc/)
- [T8 Recovering](https://docs.tigerbeetle.com/operating/recovering/)
