# Private banking sandbox V1 — Plaid + Teller

**Status:** local-only Rust parsing of synthetic Plaid/Teller response shapes and a yes/no balance-threshold preview. No live provider connection, authenticated bank provenance, genuine zero-knowledge proof, custody, settlement or payment-execution authority.

This is deliberately separate from draft PR #58's source-attestor gate. Signing a fake bank response does not authenticate it. Issue #59 tracks real source authentication, proof-system soundness, replay and consent.

## Provider interfaces

- Plaid: POST /accounts/balance/get (fresh bank balance, not cached /accounts/get); consume only exactly selected account_id, type=depository, balances.available and balances.iso_currency_code.
- Teller: GET /accounts/:account_id and GET /accounts/:account_id/balances; verify account ID, depository type, open status, USD currency and a matching available balance string. The two reads are not atomic.

Official docs checked November 2026:
https://plaid.com/docs/api/products/signal/
https://plaid.com/docs/sandbox/
https://teller.io/docs/api/accounts/
https://teller.io/docs/api/account/balances

The future private transport must enforce sandbox endpoint allowlisting, HTTPS and authentication outside this library. This library does not store or touch OAuth secrets. Private bank payloads must never reach Biz-kit or external AI agents. A local caller-granted consent flag is NOT verified financial authorization. The output always says UnverifiedSandboxPayload and must never authorize funds or a commercial discharge.

Amounts accept only exact USD minor units, with original Plaid JSON numeric lexemes preserved via serde_json arbitrary_precision. Null available balance, malformed values, subcents, negative account eligibility, credit accounts, closed Teller accounts, overflow, account mismatch and wrong currencies fail safely. USD only until a versioned provider/currency scale contract exists.

## Reproducible tests

    cargo test -p hawala-bank-sandbox --locked
    cargo test --workspace --all-targets --locked

All fixtures are invented, synthetic and contain no real credentials or customer data.

## Follow-on gates (NOT implemented)

1. Private-node-only actual Plaid and Teller sandbox transports with provider-granted consent, revocation and key custody; no secrets in GitHub or Biz-kit. Prove sandbox network access in a credential-scoped environment.
2. Transactions: Plaid /transactions/sync requires full-page atomically committed cursor and mutation-during-pagination restart; Teller requires overlap/reconciliation for pending/posted transitions and from_id pagination.
3. Independent bank-origin attestation or reviewed TLS provenance; consumer-controlled JSON is untrusted. Sandbox balances/transactions may disagree and balance does not mean funds reserved.
4. Genuine audited ZK prover/verifier with source binding, exact obligation IDs, purpose-scoped commitments, nonce, freshness, and independently verified keys.
5. Hawala remains sole financial authority; Biz-kit receives only independently verified authorized receipts and never infers paid or settled from a balance preview.
