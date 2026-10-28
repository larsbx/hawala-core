# Authority descriptors and trust pinning (v1)

Status: **reference specification and pure implementation
(`hawala-authority-descriptor`); no discovery transport, no live authority.**
Addresses ADR 0005 gap **G5**. It supplies the `authority_key`,
`scope_id`, `authority_epoch` and `custody_profile_digest` that
[cross-authority manifests](CROSS_AUTHORITY_V1.md) pin.

## Problem

There is no global registry, so each relying instance needs answers to four
questions:

1. Who orders scope S now?
2. Under which custody profile, fencing policy and mandate?
3. Which key takes over next?
4. What happens when a key is stolen or the authority equivocates?

Discovery (gossip, ActivityPub, a biz-kit log, a QR placard) can carry
descriptors, but it can never be the reason one is trusted.

## Exact bytes

The encoding conventions are those of `CROSS_AUTHORITY_V1`: byte
concatenation, `B32` exactly 32 bytes, `U64` big-endian, `H` = SHA-256, no
optional fields and no trailing bytes.

```text
"hawala-authority-descriptor/v1\0"
network_id:B32 scope_id:B32 authority_key:B32 authority_epoch:U64
predecessor_digest:B32 next_key_digest:B32
custody_profile_digest:B32 fencing_policy_digest:B32 mandate_digest:B32
```

- **Envelope:** the signing input followed by a 64-byte Ed25519 signature by
  `authority_key`, 359 bytes in total. The descriptor digest is
  `H(envelope)`.
- **Predecessor:** `predecessor_digest` is zero exactly when the epoch is 0.
  Otherwise it is the digest of the previous epoch's envelope.
- **Successor commitment:** `next_key_digest` is `H(successor public key)`.
  It must not commit to the descriptor's own key. All zeros (`RETIRED`)
  means no successor will ever be accepted.
- **Required fields:** network, scope, custody profile, fencing policy and
  mandate digests are nonzero.
- **Rejection:** a descriptor that is malformed, wrongly shaped or wrongly
  self-signed is rejected.

Golden vectors are in
`crates/hawala-authority-descriptor/test-vectors/authority_descriptor_v1.json`.

## Relying-party rules

1. **Anchor.** Trust starts only from a descriptor whose digest the local
   operator pinned out of band, at any epoch. A descriptor received through
   discovery that does not match is `AnchorMismatch`.
2. **Advance one epoch at a time.** Epoch e+1 is accepted only if it:
   - has the same network and scope;
   - links to the pinned epoch-e digest;
   - is held by the key that epoch e committed to;
   - and epoch e is not retired.

   The old key's signature is neither needed nor sufficient. An attacker
   holding only the current key cannot redirect succession. Skipping an
   epoch is a `Gap`.
3. **Known epochs are immutable.** Replaying an accepted descriptor is a
   no-op. A different valid descriptor for an accepted epoch is a
   `Conflict` that carries both digests.
4. **Admission uses the exact current epoch.** A manifest leg is
   admissible only if its network, scope, epoch, authority key and custody
   profile equal the current pinned descriptor's. Older epochs stay
   evidence for their own history and never authorize a fresh command.

Pins are immutable values. `advance` returns a new pin and never changes the
old one.

## Pinning other evidence issuers

Execution observers and settlement attestors are pinned with the same chain
and rules. Their descriptor's `scope_id` is the role-specific value
`H("hawala-role-scope/v1\0" || tag:U8 || leg_scope)`, where the tag is 1
for an execution observer and 2 for a settlement attestor. Authorities keep
the leg's own `scope_id`, so manifests and the v1 vectors are unchanged.

`admit_issuer(role, leg_scope, issuer_key, signer_epoch)` checks the issuer
fields of an evidence header against the current pinned key and epoch of
that role and leg. One role's pin can never vouch for another role's
evidence, another leg's, or a manifest leg.

Coordinators are deliberately excluded. Their succession is fixed by the
recovery mandate (`CROSS_AUTHORITY_V1` §4), not by a descriptor chain.

## What this does not solve, stated plainly

- **Equivocation:** relying parties that received rival descriptors in
  different orders hold different pins. That is inherent without global
  consensus. Any party holding a `Conflict` must hold the scope and retain
  both envelopes as evidence. Conflict evidence can be shared, but nothing
  is revoked (ADR 0001).
- **Propagation window:** a relying party that has not yet seen epoch e+1
  still admits epoch e. Before publishing e+1, the authority must fence
  epoch e at its actual signing, financial-append and custody boundaries,
  under the policy that `fencing_policy_digest` names (see
  `CROSS_AUTHORITY_V1` §4). A descriptor does not fence anything by
  itself.
- **Lost successor key:** if the committed successor key is lost, the scope
  cannot advance. Continuing requires each relying party's operator to
  re-anchor deliberately, out of band. That is a local trust decision, not a
  protocol action.
- **Stolen successor key:** whoever holds the committed successor key can
  publish the next epoch. Pre-commitment protects against theft of the
  *current* key only. Keep successor keys offline until rotation.
- **Not yet built:** the discovery transport, durable pin storage, and the
  biz-kit side of pinning.
