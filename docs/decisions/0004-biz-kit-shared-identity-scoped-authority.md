# ADR 0004: Shared identity with biz-kit; biz-kit only as a scoped authority

Status: **Proposed**, 2026-10-28. Paired with biz-kit's
[Hawala: shared identity and scoped authority](https://github.com/larsbx/biz-kit/blob/main/docs/architecture/2026-10-28-hawala-shared-identity-and-scoped-authority.md).
Both records state the same decisions D1–D6; a change to one requires the same
change to the other. No production financial authority is implied. Extends
ADRs 0001 and 0003 and [M6-C](../M6_C_AUTHORITY.md).

## Context

biz-kit consolidates its business state into one signed CoopSubstrate ledger
(Ed25519 over `CoopEventCanonicalV1`, a serialized append gate, anchored
checkpoints). Hawala has trust, obligation, clearing and settlement concerns
and requires every change to customer claims or dar exposure to be ordered by
**exactly one named financial authority** per credit lot and collateral
allocation ([M6-C](../M6_C_AUTHORITY.md)). [Architecture](../architecture.md)
already says biz-kit is a consumer, adapter or operator, never a universal
authority over Hawala.

The question is how far the two may integrate: shared identity, shared
evidence, and whether biz-kit's ledger may order Hawala settlement.

## Decisions

**D1. Identity is shared by key, not by registry.** A Hawala identity is its
Ed25519 public key (ADR 0001). biz-kit may bind that key to a platform
principal through an explicit binding, exactly as its resolver binds
`{issuer, subject}` today. The binding is biz-kit's local fact. It confers no
Hawala authority, and Hawala never resolves a biz-kit principal ID. Revoking a
biz-kit grant, session, device or binding does not revoke the Hawala identity
or invalidate its historical signatures. This is consistent with ADR 0001 and
with ADR 0003's rule that a grantor withdraws only its own future grants.

**D2. Attestations are portable evidence.** Credentials, co-signed interaction
receipts, dual-signed obligations, and payout and settlement acknowledgments
may be stored verbatim as byte strings inside biz-kit events. Inclusion in a
biz-kit log and checkpoint proves retention and position in that log only. It
does not prove validity (re-verify the Hawala signature), cash movement,
finality or authority.

**D3. One key may sign in both systems only under disjoint signing inputs.**
Every identity-core signing input begins with `hawala-identity-core/v1\0`; its
first byte `0x68` is a CBOR text-string head. biz-kit envelopes sign a
canonical CBOR **map**, whose first byte lies in `0xA0..=0xBB`. The two input
sets are therefore disjoint. That holds only while a shared key signs, in
biz-kit, nothing except envelope cores (or other inputs proven disjoint), and
both repos must pin it with a test. Identity keys remain distinct from Iroh
endpoint keys.

**D4. biz-kit is never the network-wide Hawala ledger.** No global balance
database or central transaction server is introduced. Gossip, federation,
marketplace and group-buy surfaces in biz-kit hold no Hawala balances.

**D5. A biz-kit instance may be one named authority for an explicit scope.**
Under M6-C's single-operator custody profile, one biz-kit instance *A* may be
the named authority for a declared scope *S*: the credit lots, collateral
allocations and dar-exposure accounts of named operators. Hawala defines the
contract; *A* implements `admit(operation, authority_envelope)`. Hawala never
calls biz-kit domain APIs. *A* may be enabled for *S* only after every
condition below holds and is tested:

1. **Single commit.** One serialized append batch consumes the QR nonce and
   event ID, reserves custody, applies balanced postings, checks dar exposure
   and rechecks policy epoch, grants, quote and expiry at commit time.
   Otherwise the result is `HOLD`.
2. **Accounting.** The `hawala-ledger` invariants (balanced per currency,
   nonnegative natural balances, idempotency by event ID and posting hash)
   hold as a pure fold over *S*'s events.
3. **Integer range.** Amounts stay within `CoopEventCanonicalV1`'s i64 range,
   or are encoded as decimal strings at the schema layer. Silent truncation
   is forbidden.
4. **Governance fence.** No biz-kit governance or administrator event can
   change issued customer claims or collateral in *S*. ADR 0003's revocation
   table binds *A*.
5. **Refinement.** The Lean/Quint lifecycle properties and the generated
   traces in `formal/traces/` replay against *A*'s fold and append gate as
   conformance tests.
6. **Custody, law and review.** Real asset reconciliation, licensing,
   AML/KYC and an independent security/legal review are in place. Neither
   codebase provides them.

**D6. Clearing between authorities stays a Hawala protocol.** Obligations
between dars ordered by different authorities (two biz-kit instances, or
biz-kit and any other custodian) settle through Hawala's bilateral
coordination contract ([M6-C6](../M6_C6_COORDINATION.md)), not through a shared
or bridged ledger.

## Consequences

- Accounts in *S* lose offline commit: a dar partitioned from *A* gets
  `HOLD`. Peers outside *S* are unaffected.
- *A*'s operator becomes the regulated custodian or operator for *S*.
- Formal guarantees proved for the Rust journal do not carry over to *A*
  until D5.5 holds.

## Next steps

1. Publish identity-core test vectors that biz-kit verifies byte-for-byte (D1–D3).
2. Add the D3 disjointness test on both sides.
3. Only then consider a single-operator adapter behind M6-C's `admit` (D5).
