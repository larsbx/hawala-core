# ADR 0001: Identity and reputation are append-only; no revocation primitive

**Status:** Accepted for the identity-core design.

## Decision

A Hawala participant's identity is a persistent public-key identity presented
using QR. Identity-core has **no operation to revoke a participant identity,
erase signed history, broadcast revocation lists, or disable a participant
network-wide**. Signed credentials and ratings are attestations of what an
issuer or counterparty stated at a specific time. Subsequent corrections or
contradictory attestations are new signed evidence, not deletion of the past.

`verify_credential` verifies a time-bounded claim for a specific issuer,
kind, subject, and scope under a relying peer's local issuer-trust policy.
The "current verified" badge is an application interpretation of evidence,
not a permanent grant of permission.

## Separation of concerns

- **Identity:** Who can demonstrate possession of a persistent key.
- **Evidence:** What issuers and counterparties signed and when.
- **Reputation:** Domain-specific derivation over selected evidence.
- **Authorization:** What the counterparty is permitted to do *now*;
  defined by each application, trust agreement and applicable law.
- **Hawala money movement:** A regulated activity requiring independently
  current eligibility, sanctions/risk controls, liquidity and bilateral
  authorization. A long-lived credential is not sufficient to release funds.

## Consequences and open security risks

No revocation means a stolen identity signing key can continue generating
valid signatures. Key loss and compromise therefore require deliberate
succession/recovery design (preferably with short-lived operational keys,
separate authorization keys and auditable recovery attestations); this ADR
does not pretend those problems are solved. Peers must never equate old
credentials with current legal or financial authority.

A credential with an elapsed expiry remains a signed historical record; it
no longer satisfies the verifier's current-claim check. Historical validity
may be evaluated using an attested time and separate policy, not by lying
about the current clock.

Issuer-trust policies can differ across peers. No central server or single
network-wide authority decides identity, reputation or permissions.

**Out of scope:** credential revocation lists, cancellation gossip, a global
blocklist, globally authoritative identity registries or retroactive erasure.
Services may refuse transactions under their own admission policies without
invalidating a participant's persistent identity.
