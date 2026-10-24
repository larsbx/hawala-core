# Hawala and reusable identity: initial protocol contract

## Boundaries

- **hawala-core** will own bilateral exposure limits, accepted obligations,
  payouts, clearing and settlement. Payout and settlement are distinct events.
- **identity-core** owns persistent identities, QR presentation, signed
  credentials, proof of possession, ratings and evidence evaluation.
- **Iroh** is intended for transport and peer connectivity; it is not a
  canonical ledger or authorization oracle. Identity keys should not be Iroh
  endpoint keys. Neither transport nor app-agnostic identity implies consensus.
- **biz-kit, merchants, ATMs and gift-card distributors** are consumers,
  adapters, or operators; none becomes a universal authority over Hawala.

## Identity rules

1. QR presents the cryptographic identity, and every QR display must show an
   adjacent readable **name or identity code**. An identity QR contains a
   public Ed25519 verification key, not real-world PII, private keys or
   withdrawal tokens. A stable short human code is derived from the QR key;
   the scanner recomputes and displays it so the user can compare it with
   the physical label. Names may be self-declared and are NOT "verified"
   without separate trusted evidence. Neither name nor short code is an
   authenticator; financial actions need signed challenge authorization.
2. Static QR identity is not authentication. Challenge-response binds a
   claimed identity, verifier, nonce, purpose and expiry using a signature.
   Verifiers must generate unpredictably random nonces and atomically persist
   one-time consumption. The library injects `ReplayGuard` rather than
   pretending an in-memory set ensures network-wide uniqueness.
3. Signature messages use domain-separated, length-prefixed v1 encodings.
   Wire interoperability requires exact bytes, version negotiation, and
   test vectors before shipping independent implementations.
4. Identities and signed historical attestations are **not revocable**.
   Credential verification checks signatures, time bounds, intended subject,
   kind/scope and locally trusted issuer. An expired assertion is still
   historical evidence; it is not proof of a *current* status.
5. Business verification, consumer verification, money-transmitter licensing,
   Hawala-agent authorization and ATM operator approval are distinct claims.
   Hawala must separately apply up-to-date financial permissions and law.

## Ratings and reputation

Each rating must be signed by the rater and refer to an interaction receipt
signed by **both** counterparties. Evidence is scoped to an explicit domain,
and duplicate ratings are rejected. The evidence evaluator returns a count
and integer star sum, not a financial credit score or universal reputation.
Evidence holders may have different subsets; no global score or globally
convergent reputation state is assumed.

A co-signed receipt shows that the two parties attested to an event, not that
a payout really occurred. Colluding participants can fabricate receipts;
Sybil-resistant reputation requires issuer weighting, abuse controls, and
external evidence for high-value decisions. Transaction contents should
never be included in public gossip; the opaque receipt ID is not a ledger ID.

## Explicit omissions and security gates

This initial milestone does NOT yet implement:
- Iroh discovery, synchronization, or authenticated application transports.
- Persistent key custody, device recovery, rotation, credential succession,
  or cryptographic nonce generation.
- Wire serialization of signed objects and forward-compatible encodings.
- Durable replay storage or credential issuance governance.
- Privacy-preserving selective disclosure, anti-Sybil enforcement, appeals,
  moderation, or durable evidence retention/deletion policy.
- Hawala ledger, bilateral trust agreements, obligation state machines,
  netting, clearing, payment finality, AML/KYC or money-transmitter licensing.
- Formal security audit or production-ready financial authorization.

**Next engineering milestone:** specify versioned authenticated P2P message
envelopes, issuer-credential trust policy and time-bounded verification,
durable nonce consumption, and signed transfer-obligation state transitions.
Start with two-peer convergence and offline replay/adversarial tests before
exposing customer balances or cash withdrawal.
