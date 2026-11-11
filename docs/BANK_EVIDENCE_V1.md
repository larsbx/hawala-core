# Bank predicate evidence — V1 TEST contract

**Status: source-attestor admission prototype, not zero-knowledge, not an open-banking connector, and not a financial writer.** Normative financial authority remains Hawala-core; Biz-kit may only consume source-attributed results through its existing authorization boundary. Never treat an account-balance assertion as asset custody, payment reservation, fulfillment, or legally final settlement.

## Threat model and what the prototype actually establishes

A user-controlled private connector may read bank data (with user consent), but Hawala and Biz-kit must not receive bank credentials, raw account numbers, raw transaction feeds, or unredacted statements. A purpose-scoped randomized account commitment is public to an authorized verifier. A separate **independently pinned attestor** may sign a narrow assertion; its signature proves **who asserted it**, not that a bank authenticated the underlying records. Signer trust requires bank/source verification by an external adapter, independent operational controls, revocation policy, consent, and an auditable provenance chain. None exists here.

Implemented in `crates/hawala-bank-evidence`:
- Three typed predicates: `AccountEligible`, `BalanceAtLeast` (public lower bound), `ExactPaymentObserved` (binds exact signed obligation digest and payment amount/currency). Every predicate discloses at least its existence and the chosen public fields.
- Fixed-width, domain-separated Ed25519 attestor signatures over business scope, randomized account commitment, optional obligation digest, amount, currency, nonce, source-record commitment, observation and expiry.
- Independent attestor pinning; no platform/host key inherits financial-data authority.
- Strict typed validation, exact expected-statement matching, freshness/lifetime/expiry, signature verification and caller-supplied fail-closed replay consumption.
- Only an opaque admitted source-attestation evidence ID plus predicate, **never** a financial success, reserved funds, ZK-verified claim or payment instruction.

**Not implemented:** true zero-knowledge proving/verifying, bank API or provider OAuth, durable replay store, trusted bank-origin verification, consent/revocation, independent custody/settlement attestors, provider webhooks, reconciliation, regulator reporting. The test in-memory replay store is not production-safe.

## Exact signing format

All fields are fixed-width and the signing operation is Ed25519 over the concatenation below, **not JSON/CBOR**:

```text
"hawala/bank-predicate-attestation/v1\\0"
version:U16be (=1) predicate:U8 (=1 eligible, 2 balance >=, 3 exact payment observed)
business_id:B32 account_commitment:B32 obligation_digest:B32
amount_minor:U64be currency:3-uppercase-ASCII nonce:B32
source_digest:B32 observed_at:U64be expires_at:U64be
```

Amounts are nonnegative integers up to 2^63-1 minor units; arithmetic/scale/fx are not inferred. A balance predicate requires a nonzero disclosed bound and zero obligation digest. Exact payment requires nonzero amount and nonzero obligation digest. Eligibility requires zero amount and zero obligation digest. Each purpose-scoped commitment must use a cryptographically random secret salt held only by the private connector; the signed `source_digest` is not a bank signature or proof of possession.

`evidence_id = SHA256("hawala/bank-predicate-evidence-id/v1\\0" || attestor_public_key || signing_bytes || signature)`.
Nonce consumption is scoped by `(business_id, nonce)` and must be durable and atomic in production; reject changed retransmissions. External controllers must bind statements to the authenticated customer's intended request and consent.

## Zero-knowledge development plan (unimplemented)

1. **Source authenticity**: investigate supported direct API/Open Banking and aggregator models. The provider's raw-data visibility must be disclosed. Obtain bank-issued signed records or a rigorously defined independent TLS provenance/attestation scheme, with explicit freshness, origin, anti-replay and counterparty checks. Client-created JSON is never authoritative.
2. **Private witness**: on the customer's device, validate the authenticated source, derive a fresh purpose-scoped account commitment and hold raw records, salts, OAuth secrets and ZK witness locally; do not upload them to Biz-kit, Hawala or public GitHub.
3. **Actual ZK**: define a proving circuit whose public inputs bind statement, issuer/provenance anchor, source-record commitment, proving key version, nonce, business/obligation scope, currency, observation time, expiry and statement kind. The circuit must **actually verify source authenticity or bind to a trusted source attestation**, enforce exact minor-unit arithmetic, range and no-overflow rules, and prove the advertised predicate. Prove and independently verify false/tampered witness rejection. Do not label the present Ed25519 attestation as ZK.
4. **Verifier/admission**: pin a reviewed verifying key and proof-system version, process proofs using an independent verifier, bind verified output to the exact source commitments, authorized business and agreement, and consume nonces durably. Separate proof verification from legally sufficient settlement/finality evidence.
5. **Integration**: Hawala owns financial meaning; Biz-kit uses an ExtensionV1-capable **read-only** status adapter, not an alternate ledger or financial writer. Existing source-obligation-binding limitation in Biz-kit `CommercialSetoff` must be resolved before proof evidence authorizes any commercial-setoff effect.

## Acceptance gates

- Tamper statement fields, issuer key, signature, source digest, currency, amount, nonce and scope: reject.
- Reject malicious/stale/future/revoked evidence, time-of-check/time-of-use use of an old balance, replay after process restart, duplicate claims with different source records, and failure of durable nonce storage.
- No raw account/transaction data, access tokens, secret salts, ZK witnesses or personally identifying bank information in logs, analytics, inter-node gossip, GitHub artifacts or Biz-kit event history.
- Verify source authenticity and ZK soundness with *independent* negative fixtures: an attacker must not prove a fabricated $10k balance by inventing a source record.
- Prove a payment's exact source obligation(s), parties, amount, currency, reference and settlement attempt are cryptographically bound; no netting across currencies without independently established lawful conversion.
- Verify statutory consent, bank/provider contracts and regulatory obligations with counsel/authorized institutions; financial privacy does not remove AML/KYC or record-retention duties.

## Local test

```bash
cargo test -p hawala-bank-evidence --locked
```

Passing tests demonstrate only the typed attestor-admission reference behavior; live banks, ZK soundness and production custody remain unverified.
