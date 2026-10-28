# ADR 0006: Biz-kit extension-host boundary and canonical Hawala financial ownership

Date: 2026-10-28. Status: **Proposed; non-executable architecture**.

Companion: [biz-kit cross-repository extension ADR](https://github.com/larsbx/biz-kit/blob/main/docs/architecture/2026-10-28-cross-repository-extension-and-hawala-capability.md), [Eatsbot product ADR](https://github.com/larsbx/eatsbot/blob/dev/docs/modules/decisions/pages/biz-kit-interoperability-and-source-ownership.adoc), [d-patch ADR-0015](https://github.com/larsbx/d-patch/blob/main/docs/adr/0015-biz-kit-extension-and-hawala-client-boundaries.md), and [school-ops decision 0007](https://github.com/larsbx/school-ops/blob/main/docs/decisions/0007-biz-kit-content-authority-and-extension-contract.md).

## Context

ADR [0004](0004-biz-kit-shared-identity-scoped-authority.md), merged in #40, establishes D1–D6: explicit public-key identity binding, portable but nonauthoritative attestations, disjoint signing inputs, no global Hawala ledger, optional strictly scoped D5 financial authority, and Hawala-controlled inter-authority coordination.

ADR [0005](0005-biz-kit-federated-hawala-integration.md) is **proposed in open PR #42**, paired with biz-kit #173; it defines proposed N1–N7 for native Hawala capability throughout a federation of independently administered biz-kit installations. Its G1–G11 unresolved gaps include distributed coordinator/decision semantics, signed financial finality, disclosure, authority pinning, authentication, host key isolation and normative text drift. This document adds the **cross-repository ownership boundary**; it neither accepts 0005 nor overrides 0004.

Biz-kit also hosts Eatsbot/Dispatch/SchoolOps business workflows. Financial protocol details must not migrate into domain applications simply to make Hawala convenient to invoke.

## Decision

### H1. Hawala-core is normative for financial meaning and transport

Hawala-core alone defines QR-presented public-key identity protocol, bilateral terms, trust/obligation/credit semantics, exact FX/fees, custody and exposure invariants, balanced postings, admission/result states, clearing/coordination, financial receipts and the Iroh peer transport. Its Lean/Quint lifecycle definitions and generated traces are the reference for adapters.

Biz-kit owns its own **general application-extension contract**, local business grants, business event history, per-domain workflow adapters, host supervision and source-attributed Hawala projections. Those are **not** alternative Hawala financial authorities. External products such as Eatsbot and d-patch consume their product/backend contracts; school-ops remains the authoritative curriculum content source.

Maintain non-biz-kit Hawala peers as first-class interoperable participants.

### H2. A Hawala capability is not an automatic custodian

Participating biz-kit installations may offer a `HawalaCapabilityV1` port through an independently supervised Rust/Iroh service. This is a separately versioned Hawala service contract, not a generic `BizKit.ExtensionV1` app manifest and not a finance SDK implemented in Elixir.

A host may be a business node, Hawala peer, noncustodial router, operator or explicitly named scoped financial authority; the roles are disjoint and require independent verification. A public listing, member account, biz-kit `prn_` binding, host-installed plugin, grant declaration or network connectivity never grants custody/financial authority.

Financial scope `S` has exactly one named ordering authority over its specified customer claims, credit lots, collateral and dar exposure. No generalized bridge or replicated global Hawala balance database.

### H3. Authentication, consent and key custody cannot collapse into the host

Participant identity is presented **only** by QR, with proof of possession and independently scoped consent/transaction authorization. The host cannot custody participant identity, consent, or transaction-QR signing keys; node/operator keys are disjoint, and host-produced consent is refused. Biz-kit grants cover only its own platform actions.

Continue from merged [hawala-capability-port #43](https://github.com/larsbx/hawala-core/pull/43): it authenticates pinned host-signed, grant/body/version-bound calls and pinned service responses, rejects host-signed participant authorization and performs no financial execution. It does not yet implement socket transport, durable nonce storage, live service handshake, or biz-kit grant-to-`GrantPolicy` translation. Its `Status::Received` is **not** financial commit.

Paired cross-language identity fixtures in open #41/#172 must pin exact signing bytes, Ed25519 signatures, SHA-256 and disjoint input domains. The initial `0x68` byte of Hawala's raw `hawala...` prefix is ASCII `h` (and would denote a CBOR text-string head *if interpreted as an initial CBOR byte*), not a CBOR map head; do not call the raw signed prefix a CBOR-encoded string.

### H4. Business composition does not promote signed evidence into financial fact

Commerce, restaurant checkout, group-buy, Dispatch billing, remittance and ATM interactions may request Hawala operations through authenticated ports and retain signed receipts with provenance. A retained credential or receipt never proves actual delivery of cash or final settlement.

A Hawala financial outcome and a business-domain outcome remain distinct:

- Hawala signs/defines the financial meaning of `Proposed`, `Held`, `Committed(intent)`, execution observations and final settlement; G1 must settle the definitive state machine and independently verifiable signed bytes before business workflows rely on it.
- A successful `Committed(intent)` authorizes at most one properly fenced **attempt** in its defined scope; it is not `Paid`, a completed payout, a released escrow, or business fulfillment.
- Business-domain completion requires the exact verified financial condition specified by that application's policy.
- Cross-authority reservations follow M6-C6's coordination and safe terminal decision/abort fences. M6-C7 is currently a **synthetic local** reference, not evidence of distributed transaction commit; partitions and uncertain effects remain `HOLD`.

The biz-kit seller-home group-buy ledger has its own authoritative signed contract and external-evidence admission. Financial custody claims remain outside seller-home's ability to prove on its own; no double writer or global escrow assumption is introduced.

### H5. Any biz-kit scoped financial authority requires all D5 gates

No automatic trust, co-ownership, financial administrator privilege or payment execution from installing the capability. Before an instance may order scope `S`, require:

1. Exactly one serialized append transaction consuming relevant QR/event/attempt identifiers, custody holds, balanced postings, dar exposure, policy/grants/quote and expiry; failure is `HOLD`.
2. Pure exact double-entry invariants by currency, nonnegative balances and deterministic idempotent replay.
3. Canonical wire and ledger amount encoding with explicitly tested i64/u64 boundaries.
4. A governance fence preventing platform administration from modifying issued claims/collateral.
5. Lean/Quint trace refinement against **actual** Rust and host append/recovery implementations, including partitions, stale epochs and multi-authority agreement.
6. Demonstrated real custody reconciliation, licensing, AML/KYC, independent security/legal review and operator mandates.

The ability to run a financial *simulator* is not a waiver for any gate.

### H6. Cross-repository tests and change control

Pair this decision with biz-kit's platform-extension ADR and the application-specific source-of-truth decisions in Eatsbot, d-patch and school-ops. Version Hawala wire contracts independently of the general extension-manifest version. Reject a host that changes participant consent, fabricates financial finality, bypasses a scoped named authority, sends private payment details over public ActivityPub, or double-posts a financial lot.

Any change to D1–D6 must update and review the paired existing ADRs. Any proposed change to N1–N7 must be reconciled with both still-open 0005/#42 and biz-kit/#173, including G11's non-identical text. Do not silently declare the proposed pair accepted.

## Implementation order and verification

1. Review paired ownership ADRs and align exact cross-repo commit pins/compatibility requirements.
2. Verify matching golden signatures; implement mutually authenticated local transport, persistent replay and local grant-policy mapping without financial mutation.
3. Exercise two independently administered biz-kit nodes and Hawala Iroh peers, plus a noncustodial router, with signature/provenance/restart/partition rejection tests.
4. Formalize G1–G11 (especially financial terminal decisions and coordinator liveness) in Lean/Quint, then replay generated traces through Rust and biz-kit.
5. Consider a single D5 financial scope only after technical and regulated-operation barriers are independently verified.

This ADR changes neither current financial permission nor protocol implementation and does not represent deployable Hawala service, real custody, ATM payout or distributed settlement.
