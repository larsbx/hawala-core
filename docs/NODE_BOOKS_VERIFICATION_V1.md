# Node books verification checkpoint bytes version 1

Status: **Proposed specification and read-only conformance codec**, 2026-10-28.
This defines the signed checkpoint assertion in
[the node-owned books profile](NODE_BOOKS_TIGERBEETLE_SPEC.md), section 6.
It does not implement book replay, a PostgreSQL/TigerBeetle adapter, authority
discovery, custody or an execution permit. Existing financial state transitions
and paired ADRs are unchanged.

## 1. What a checkpoint asserts

A named verifier asserts that it compared the canonical signed history, reference
journal, PostgreSQL financial projection and TigerBeetle accounting copy through
one exact source prefix, under pinned registry/compiler rules and projector
fencing. Its signature attributes that assertion. Signature validity alone does
not establish that the comparison occurred or that the booked assets exist.

The verifier is a separately pinned role. A receipt's embedded key, mandate
digest or authority ID cannot confer trust. Relying software must obtain the
expected context from its accepted authority descriptor, policy, source history,
accounting replay and prior checkpoint. Discovery and Iroh transport cannot
supply those trust decisions implicitly.

This receipt has exactly one meaning: an attributed accounting-verification
assertion for the specified prefix. It has no `Paid`, `Settled`, `Release`,
`Retry` or executable status. `Held` diagnostics use another protocol; they
cannot be encoded by changing a checkpoint flag because this format has none.

## 2. Exact binary signing input

All concatenations are bytes. `B32` is exactly 32 bytes, `U64` is an unsigned
eight-byte big-endian integer, and `U128` is an unsigned sixteen-byte big-endian
integer. No lengths, optional fields, padding, JSON, CBOR, textual hex, timestamp,
Unicode normalization or host-native integer representation enters the input.
The domain is the 33 ASCII bytes `hawala-node-books-verification/v1` followed by
one zero byte; the complete domain including that zero is **34 bytes**.

Fields follow the domain in this exact order:

| Field | Type | Constraint or binding |
| --- | --- | --- |
| `network_id` | B32 | Pinned network |
| `installation_id` | B32 | Independently administered source installation |
| `authority_id` | B32 | Named financial authority, not inferred from installation |
| `scope_id` | B32 | Explicit portfolio scope |
| `legal_book_id` | B32 | One legal entity's book |
| `authority_epoch` | U64 | Nonzero accepted authority epoch |
| `source_sequence` | U64 | Exact canonical source-prefix position; zero permitted |
| `source_hash` | B32 | Hash at that exact prefix, including a nonzero genesis commitment |
| `registry_digest` | B32 | Exact accepted account/currency/ID mapping |
| `compiler_digest` | B32 | Exact accounting compiler and digest rules |
| `projection_digest` | B32 | Accepted compiled projection through this prefix |
| `journal_digest` | B32 | Reference financial journal through the same prefix |
| `accounting_state_digest` | B32 | Normalized accounting state compared across copies |
| `cluster_id` | U128 | Pinned TigerBeetle cluster; this profile excludes zero and 2^128−1 |
| `cluster_generation` | U64 | Nonzero projection deployment/rebuild generation |
| `projector_fence_digest` | B32 | Exact independently established exclusive-write fence |
| `verifier_key` | B32 | Raw Ed25519 public key of the pinned verifier |
| `verifier_mandate_digest` | B32 | Exact independently accepted verification mandate |
| `verification_sequence` | U64 | Nonzero receipt-chain position, beginning at one |
| `previous_receipt_digest` | B32 | Zero exactly at receipt one; otherwise nonzero preceding receipt ID |

All B32 fields except the genesis `previous_receipt_digest` must be nonzero.
The fields total 528 bytes. The signing input is **562 bytes**. Sign it directly
using ordinary Ed25519, with no prehash mode or Ed25519 context parameter.
Append the raw 64-byte signature to obtain the **626-byte envelope**. Reject
unknown domains, truncation, trailing bytes, out-of-range integers and forbidden
zero fields. The receipt ID is SHA-256 of the entire 626-byte envelope.

The body identifies one profile version through its domain. Future fields,
algorithms or semantics require another domain/version; they cannot be appended
to v1, interpreted from ignored fields or negotiated by untrusted JSON metadata.
These signing inputs begin with `hawala`, preserving the signing-domain boundary
in the shared-identity ADR. Participant identity presentation remains QR-only;
these binary scope and book IDs are protocol references.

## 3. Digest and state boundaries

The digest values remain B32 commitments in this receipt codec. The proposed
[v1 accounting profile](NODE_BOOKS_ACCOUNTING_V1.md) now fixes registry/compiler
encodings and journal/projection history commitments. Normalized-state encoding
and actual replay/comparison remain open. The existing `hawala-ledger` BLAKE3
entry digest is not reinterpreted as SHA-256. Receipt verification must use the
exact accounting profile independently pinned by its consumer.

The verifier must independently compute the expected digests after the full
comparison required by the parent profile. A caller must not fill the expected
record by copying fields from the untrusted receipt and then call that result
verification. If the relevant history or pinned digest rules are unavailable,
the operation remains `HOLD`. A hash alone is not a proof of concealed postings.

There is no time-based expiry in this historical assertion. Freshness comes
from the exact source-prefix and authority/deployment pins required by the
consumer. An old valid signature is insufficient for a newer prefix, new epoch,
changed registry/compiler, disputed state or a different cluster generation.

## 4. Continuity and recovery

Within one receipt chain, network, installation, authority, scope, legal book,
authority epoch, cluster ID/generation, verifier key and verifier mandate stay
fixed. The next receipt sequence is exactly the previous sequence plus one,
without wraparound, and its predecessor digest equals the previous full receipt
ID. Source sequence cannot decrease.

Reverification of the same source sequence is permitted, for example after a
new projector fence. Source hash and registry, compiler, projection, journal
and normalized-state digests must then be identical. A second signed assertion
with a different value is retained as conflict evidence; it cannot overwrite
the prior receipt. A larger source sequence does not itself prove that the
source history extends the old one: the canonical ledger's authenticated
continuity must be independently verified.

Key/mandate rotation, authority succession and cluster rebuild require an
explicit pinned transition and independently established fencing. They start
a newly pinned receipt chain. A self-declared receipt-one marker is not evidence
that a prior chain or outstanding attempt disappeared. Bootstrap may be at any
fully verified source prefix; source sequence zero is not required.

Identical receipt replay is historical evidence with the same ID. It does not
create another financial event or execution opportunity. `verifyReceiptBinding`
checks exact bytes against complete external pins and verifies the signature.
`verifySuccessorBinding` additionally checks the local two-receipt chain rules.
Neither helper verifies the authority's canonical history, a mandate's legal
effect, accounting correctness, or the truth of a physical observation.

## 5. JSON interchange view

JSON is a tooling view, never a signing input. Every field is present exactly
once. B32 values are exactly 64 lowercase hexadecimal characters; numeric
fields use decimal strings matching `0|[1-9][0-9]*`, checked against their binary
width before encoding. No numeric JSON values, plus signs, leading zeros,
whitespace, fractional/exponent forms, booleans, nulls or unknown fields are
accepted. Raw-JSON parsers must reject duplicate keys before object conversion;
the reference `fromJson` operates on an already parsed object and cannot recover
discarded duplicate-key evidence.

Public deterministic fixtures use the Ed25519 seed consisting of byte `0x51`
repeated 32 times. These keys have no operational mandate. The three fixtures
were encoded and signed independently in Python with PyCA, then checked by the
Node codec: a receipt-one checkpoint at source zero, a successor at the U64
source boundary and U128 cluster boundary permitted here, and same-prefix
reverification with a changed fence. Opaque fixture digests are synthetic labels,
not claimed outputs of actual accounting replay.

## 6. Conformance and remaining work

Run from repository root with Node 24 and Python 3:

```sh
node tools/check_node_books_verification.mjs
python3 tools/check_node_books_verification_mutations.py
```

The checker pins expected signing bytes, envelope bytes, receipt IDs and
signatures from the committed fixtures. It covers every-byte mutation and every
truncation, all expected-context fields, malformed JSON values, integer bounds,
zero/genesis rules, predecessor and sequence checks, source rollback, same-prefix
conflicts and cross-context transplants. Seven unsafe mutations must trigger
their named assertion counterexamples; syntax, runtime and subprocess failures
cannot satisfy those negative controls.

The module is read-only specification tooling. It has no signer, database,
network, financial mutation or execution API. Test signing exists only in the
checker with the public test seed. Formal financial transitions are unchanged;
this increment claims byte/signature conformance, not Lean/Quint refinement.

Remaining dependencies are normalized-state encoding and actual history replay,
durable PostgreSQL/TigerBeetle comparison, effective projector fencing,
authority/verifier discovery and succession, Rust/Elixir codec conformance and
the execution gate's formal refinement. None of the paired ADR gaps is declared
closed by these receipt vectors alone.
