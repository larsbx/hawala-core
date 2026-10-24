# ADR 0002: QR identity plus a human-readable name or code

**Status:** Accepted for identity presentations.

## Decision

QR is the cryptographic identity input. Every participant identity card, agent
sign, merchant placard, and ATM identification screen must display a readable
**name or identity code alongside the QR**.

The Rust identity-core presentation method **always** supplies a deterministic
short code (`HW-XXXX-XXXX-XXXX` from the first six bytes of the public key);
a display name is optional. The scanned app recomputes the code from the
QR's full public key and shows it for visual cross-check against the printed
card, on-screen card or counterparty.

## Limitations and boundaries

- The short code is a convenience identifier, **not** a secret, an
  authentication factor or a globally collision-free ID. All cryptographic
  checks use the **entire** public identity key extracted from the QR.
- A displayed name is a self-asserted label. A verified name or verified
  merchant/hawaladar badge requires specific issuer evidence and relying-party
  policy; a visible label alone is not proof.
- Human-readable names may change, and several peers can share them. Never
  route financial transfers by ambiguous name or truncated code alone.
- The QR scan does not authorize a transfer. The application must verify fresh,
  transaction-bound signatures, counterparty expectations and financial policy
  before making funds available.
- Presentations containing sensitive PII must follow privacy and applicable
  records rules; pseudonyms or short codes are supported.
- This does not reintroduce a central identity database or change the
  separate, time-sensitive checks for financial-agent authorization.
