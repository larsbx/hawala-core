# Node books registry and posting compiler version 1

Status: **Proposed pure specification tooling**, 2026-10-28. This refines
[the node-owned books profile](NODE_BOOKS_TIGERBEETLE_SPEC.md) and supplies
registry, compiler, journal-chain and projection-chain commitments for its
[verification receipts](NODE_BOOKS_VERIFICATION_V1.md). It does not implement
admission, persistence, database access, a TigerBeetle client or money movement.

## 1. Authority and encoding boundary

Every object belongs to exactly one network, installation, financial authority,
scope, legal book and authority epoch. Encodings start with distinct ASCII
`hawala-node-books-<object>/v1` domains followed by one zero byte. No JSON,
native SDK memory layout or reserialization of existing signed Hawala objects
enters these byte formats. Original signatures remain verbatim evidence.

`B32` is exactly 32 bytes. `U8`, `U16`, `U32`, `U64` and `U128` are unsigned
big-endian integers of one, two, four, eight and sixteen bytes. `H` is SHA-256.
All B32 fields are nonzero except an explicitly permitted genesis predecessor.
All concatenations are bytes. Unknown fields, wrong widths, truncation and
trailing bytes are rejected. There is no optional-field or alternate-encoding
negotiation in v1.

The 168-byte context is always:

```text
network_id:B32 installation_id:B32 authority_id:B32 scope_id:B32
legal_book_id:B32 authority_epoch:U64
```

Epoch is nonzero. It is a pinned authority fact, not a permission conferred by
these codecs. Registry and source commitments must come from independently
accepted canonical history. Hashing an arbitrary supplied object does not make
it an admitted financial event.

## 2. Canonical account registry

Registry bytes are:

```text
"hawala-node-books-registry/v1\0"
context:168 bytes revision:U64 previous_registry_digest:B32
ledger_count:U16
[ ledger_id:U32 currency:3 ASCII scale:U8 ] × ledger_count
account_count:U16
[ canonical_account_id:B32 tigerbeetle_account_id:U128
  ledger_id:U32 normal_side:U8 ] × account_count
```

Revision starts at one, with a zero predecessor exactly at revision one.
Ledger count is 1..65535; account count is 2..65535. Ledger IDs are nonzero
and strictly increasing. Currency is exactly three uppercase ASCII bytes;
scale is 0..18. Duplicate currency/scale identities in one registry are rejected.
Different scales are distinct assets and must never balance against one another.

Account rows are strictly sorted by full canonical ID in unsigned byte order.
Canonical IDs and mapped TigerBeetle IDs are injective within the registry.
Mapped IDs exclude zero and 2^128−1. Every account references a registered ledger.
Normal-side tag 1 means debit-normal; tag 2 means credit-normal. These tags are
our wire tags, not native TigerBeetle flag values. The adapter must separately
set the corresponding nonnegative-natural-balance guard from the parent profile.

The registry digest is `H(registry_bytes)`. `verifyRegistryBinding` checks the
decoded context and entire digest against externally supplied pins. It cannot
choose its own trust anchor from the incoming registry.

A successor registry has identical context, revision exactly one higher, and
predecessor equal to the previous registry digest. It retains every old ledger
definition and every old account ID, mapped ID, ledger and normal side unchanged.
New rows may be inserted at their canonical sorted positions. Removing or
remapping old accounts is forbidden. Epoch changes require a separately accepted
fenced migration; they are not registry succession in this profile.

The canonical authority transaction still owns deterministic ID allocation.
This codec validates mappings; it does not allocate or reserve IDs. A deployment
sharing a cluster across scopes must enforce cluster-wide uniqueness of mapped
account and transfer IDs, and distinct ledger IDs for authority/scope/legal-book/
currency/scale tuples. Object-local uniqueness alone is insufficient. Each scope
retains its own registry and accepted history.

The bounded decoder checks counts against remaining bytes before allocating
row arrays, and rejects registry inputs larger than 4,000,000 bytes. Registry
length is `domain_length + 212 + 8 × ledger_count + 53 × account_count`.

## 3. Canonical journal wrapper

Journal bytes are:

```text
"hawala-node-books-journal/v1\0"
context:168 bytes registry_digest:B32 source_sequence:U64
source_hash:B32 event_id:B32 posting_count:U16
[ canonical_account_id:B32 side:U8 amount:U64 ] × posting_count
```

The source sequence is positive and identifies an accepted source event, not
network arrival order. Source hash commits to that canonical prefix. Event ID
retains its full 32 bytes. Context and registry digest must match the pinned
registry exactly. The codec cannot verify the source event's signatures or
acceptance; the caller must establish those before compilation.

There are 2..256 distinct-account postings. Side tags are 1 for debit and 2 for
credit. Amounts are strictly positive and at most 9223372036854775807 minor units.
The encoder sorts postings by full account ID, so input-array permutation does
not affect canonical bytes. The decoder requires that canonical row order.
Duplicate accounts, unknown accounts, extra fields and ambiguous posting sides
are rejected.

Debit totals equal credit totals **for each registry ledger independently**.
Matching USD debit against EUR credit, or quantities at different scales, cannot
balance a journal. With at most 256 capped postings, totals fit unsigned 128-bit
intermediates; the specification codec uses exact BigInt arithmetic.

This is a new wrapper commitment, `H(journal_bytes)`, not a replacement for the
existing `hawala-ledger` BLAKE3 entry digest. Reference-entry compatibility and
source attachment remain explicit adapter obligations. The decoder bounds a
journal to 11,000 bytes; its exact length is
`domain_length + 274 + 41 × posting_count`.

## 4. Deterministic transfer plan

The pure compiler receives the pinned registry, validated journal, first
transfer ID and event reference. Both numeric IDs are U128 values excluding
zero and 2^128−1, already allocated by the canonical authority transaction.
IDs remain unchanged on retry. The compiler has no database or allocation API.

For each used numeric ledger in increasing order, take debit and credit lists
in full canonical account-ID order. Emit the minimum remaining amount between
the current debit and credit. Advance each exhausted side and repeat. Transfer
IDs are `first_transfer_id + emitted_index`; overflow into a reserved value
rejects the complete plan. Numeric account-ID order never replaces canonical
account-ID order.

Each output transfer has its mapped debit/credit account IDs, equal amount,
ledger, `code = 1`, and `user_data_128 = event_reference`. All transfers except
the final transfer of the **whole journal** are linked, including across
currency boundaries. The last transfer is unlinked. The plan contains 1..255
transfers and must fit the deployment's separately pinned request limits.

The adapter must set native pending IDs, timeouts and unused user-data fields
to zero, use no pending/balancing/imported flags, and allow TigerBeetle to assign
its timestamp. The output is a logical plan, not native SDK request bytes. SDK
serialization and lookup/result handling remain unimplemented.

This compiler checks shape, mapping and conservation. It has no opening book,
customer consent, custody proof or policy grant. The authoritative reference
fold must first establish that the entire transition is authorized and leaves
all natural balances within range. A balanced compiled plan alone cannot
establish available funds or authorize admission.

Each account appears on only one posting side. Starting from a reference-valid
book, its natural balance moves monotonically toward the validated final value
as compiled legs apply. This avoids a transient underflow introduced solely by
pairing. Generated conformance cases check this property; no universal proof or
actual TigerBeetle execution is claimed here.

The compiler digest is H of these exact ASCII bytes, including both LF bytes:

```text
hawala-node-books-compiler/v1
sort-ledger-u32;sort-account-b32;pair-min;positive-i64;max-postings-256;ids-first-plus-index;code-1;event-reference-u128;linked-all-except-last;no-pending
```

This digest identifies the normative compiler contract, not an executable
binary. A deployment must separately pin and review its actual adapter artifact
in the accepted deployment manifest and demonstrate conformance to these rules.

## 5. Projection manifest and history commitments

The projection manifest is:

```text
"hawala-node-books-projection/v1\0"
journal_wrapper_digest:B32 compiler_digest:B32 event_reference:U128
transfer_count:U16
[ transfer_id:U128 ledger_id:U32 debit_tigerbeetle_id:U128
  credit_tigerbeetle_id:U128 amount:U64 code:U16 linked:U8 ] × transfer_count
```

Each row is 63 bytes. Linked is exactly 1 or 0. Event reference is shared by
all rows. `verifyProjection` recomputes the complete canonical plan from pinned
inputs and compares every supplied byte; it rejects extra, omitted, modified or
reordered legs. There is no standalone permissive projection decoder. Manifest
comparison says nothing about whether TigerBeetle actually committed that plan.

To commit to a financial history, use these separate chains for kind `journal`
or `projection`:

```text
genesis = H("hawala-node-books-<kind>-chain-genesis/v1\0" || context)
next    = H("hawala-node-books-<kind>-chain-step/v1\0" || previous:B32
            || H(canonical_object_bytes))
```

`advanceJournalDigest` and `advanceProjectionDigest` first canonically encode
their respective validated objects. Callers fold accepted financial events in
strict source order and retain event-ID/body tombstones; identical replays do
not advance either chain. Registry revisions are selected by each event's
recorded digest. Non-posting source events do not append a financial-chain step.
The pure hash helpers do not persist sequence or detect replay on their own.

For this accounting profile, a verification receipt's `registry_digest` binds
the registry at its source prefix, `compiler_digest` binds the rule text above,
and `journal_digest`/`projection_digest` bind the corresponding folded histories.
The receipt also binds source sequence/hash. Normalized accounting-state bytes
and actual PostgreSQL/TigerBeetle inventory comparison remain the next layer.
The earlier receipt fixtures retain their explicitly synthetic label digests.

## 6. Conformance and implementation boundary

Run from repository root with Node 24 and Python 3:

```sh
node tools/check_node_books_accounting.mjs
python3 tools/check_node_books_accounting_mutations.py
```

Committed fixtures were independently encoded with Python integer/byte tools.
Their transfer pairings were specified independently by hand, including two
currencies and the maximum monetary/ID boundary. The checker pins their file
digest, exact registry/journal/projection bytes, compiler digest, transfer plans
and successive history commitments. There is no regeneration mode in the checks.

Further checks cover byte tampering, noncanonical row order, truncation, registry
immutability, duplicate mappings, scope mismatch, per-ledger imbalance, numeric
limits, 200 synthetic differential journals and the maximum 255-leg chain.
An independent account-delta oracle checks compiled effects and intermediate
natural balances from synthetic starting books. Six unsafe mutations must
trigger named assertion counterexamples; runtime failures cannot pass them.

These checks establish sampled pure-code conformance. They do not prove
PostgreSQL atomicity, global ID allocation, storage honesty, external custody,
actual TigerBeetle behavior, full financial replay or Lean/Quint refinement.
Rust/Elixir conformance, normalized snapshots, durable projector fencing and
database-backed reconciliation remain implementation gates under the parent
profile. Existing financial permission and paired ADR requirements stay intact.
