# ADR 0005: Hawala as a network-native capability of federated biz-kit

Status: **Proposed**, 2026-10-28. Paired with biz-kit's
[network-native Hawala profile](https://github.com/larsbx/biz-kit/blob/main/docs/architecture/2026-10-28-federated-hawala-native-capability.md).
Changes to N1–N7 require corresponding review in both repositories.
This extends [ADR 0004](0004-biz-kit-shared-identity-scoped-authority.md),
[M6-C](../M6_C_AUTHORITY.md), [M6-C6](../M6_C6_COORDINATION.md), and
[M6-C7](../M6_C7_COORDINATED_ADMISSION.md). It **does not** turn any test-only
finance primitives into a deployable financial network.

## Intent

Hawala should be a **first-class network capability across independently
administered biz-kit installations**, not merely an external identity provider,
evidence importer or optional payment URL. Business products can compose Hawala
remittance, accepted obligations, escrow-backed credits, merchant payments,
clearing, payouts and ATM interactions through one native capability contract.
Hawala remains an Iroh-native P2P protocol, including peers not running biz-kit.

Deep integration is **composition of services and user flows, not universal
authority, shared custody, shared balances, or common database ownership**.

## Proposed decisions

**N1. One Hawala capability plane per participating biz-kit instance.**
A biz-kit installation may host one locally administered Hawala-capability
plane: signed identity and reputation evidence, authenticated Iroh peer
transport, quote/terms and payment-intent APIs, financial-authority routing,
operation recovery and user-facing status projections. A deployment may run a
supervised Rust service/sidecar with versioned authenticated local ports; pure
Rust functions may be linked separately. A JVM/BEAM process, web controller,
ActivityPub adapter or transport connection never receives financial authority
merely by calling this service. No universal Hawala operator, required central
server or singleton identity registry is introduced.

**N2. Node roles are separate from custody/authority roles.**
Each instance may independently be a marketplace home, customer/merchant peer,
routing/observer peer, hawaladar operator, payout terminal operator, or
**explicitly named financial authority for a scoped portfolio**. The first
four roles confer no custody or financial admission powers. Operator or
authority roles require independent legal mandate, local authorization,
jurisdiction-specific eligibility and enforced scope. Declaring a role or
advertising a route never creates financial authority.

**N3. Hawala keeps the canonical protocol and financial rules.**
`hawala-core` remains the authoritative implementation/specification owner for
QR-only identity presentation, authenticated Iroh session binding,
dual-signed terms, transaction QR, obligation provenance, trust/exposure,
double-entry postings, custody receipts, FX, clearing, payout lifecycle,
dispute resolution and `admit(operation, authority_envelope)` semantics.
biz-kit provides versioned host adapters, policy/grant translation, workflow
presentation, custody/authority deployment and signed business evidence.
Do not reimplement a competing Hawala ledger or protocol in
`commerce_core`, `federation_core`, `group_buy_core` or
`coop_substrate`; keep independently runnable Hawala peers possible.

**N4. Scoping is financial, not network-wide.**
A participating node's biz-kit business ledger owns its local business facts.
Only if ADR 0004 D5's **all six** conditions hold may that same instance's
serialized financial append boundary be the **single named Hawala financial
authority** for an explicit scope S of obligations, credit lots and collateral.
The scope, custody adapter, epochs, acceptance policy, successor/fencing policy
and precise accountable operator must be documented and verified. Other
instances may hold read-only, independently verified receipts/projections for S
but have no write authority. If D5 is not met, use an independently authorized
Hawala financial authority and treat biz-kit local records as nonauthoritative
evidence. There is no distributed shared biz-kit ledger for all Hawala balances.

**N5. Inter-instance settlement uses Hawala's dedicated private protocol.**
Iroh carries signed end-to-end financial peer messages; ActivityPub can carry
public listings/discovery signals but not signed financial authorization,
private KYC, transaction QR, custody instructions or payment finality.
Across distinct authorities use M6-C6's common final-decision/abort-fence
contract and its accountable common order, not two independent successful
commits, optimistic cross-ledger writes, gossip quorum or an ActivityPub
`Accept`. M6-C7 is currently only a **single-local-transaction synthetic
reference**, not an implemented distributed coordination protocol. On
unavailable/stale/mismatched authority, grant, custody or decision state,
return `HOLD`; retain locks and original IDs until independently authorized
reconciliation.

**N6. Native composition with commerce, fulfillment and group buying.**
A marketplace purchase, restaurant checkout, dispatch fee, group-buy pledge
or remittance starts as an **authorized business/payment proposal**, not a
Hawala posting. Bind business ID/revision, parties, exact amount/currency,
accepted FX/fees, expiry, obligation digest, funding lot, authority IDs,
attempt ID and permitted disclosure into an immutable correlation record.
The financial authority independently performs `admit`, with
commit-before-execute. A durable, idempotent signed receipt/status projection
may advance a business workflow only when it proves the **required financial
state** under pinned trust/policy; `Committed(intent)` is not payout delivery or
final settlement. Reconciliation handles partial success, timeout, replay,
dispute and compensation. Do not declare cross-instance ACID atomicity.

**N7. Security and formal conformance before custody.**
Preserve QR as the **sole presented participant identifier**, with live
challenge-based proof, never a bearer authorization. Separate identity,
endpoint, device and financial mandate keys or prove their signed-byte domains
disjoint. Enforce independent local grants, replay-resistant attempt/nonce
consumption, domain-separated signatures, consent/privacy constraints and
strict exact monetary encoding. Start implementation from Lean/Quint
specifications of per-scope authority, two-peer commit/abort, idempotency,
partition HOLD and crash recovery; compile formal traces into Rust and
biz-kit conformance tests. All real custody, KYC/AML, licensing, independent
security/legal review and hardware authorization gates of D5 remain in force.

## Required reference deployment (not yet implemented)

```text
Biz-kit instance A                           Biz-kit instance B
Marketplace/Commerce                        Marketplace/Commerce
       |                                             |
Hawala capability host                       Hawala capability host
       |                                             |
Hawala Iroh peer  <--- signed Hawala wire ---> Hawala Iroh peer
       |                                             |
Authority A for scope S_A?                   Authority B for scope S_B?
       |                                             |
Named custody A, if authorized               Named custody B, if authorized
       +------ M6-C6 coordination ------------+
                  common terminal decision
```

A node may have **no** named financial authority and simply route to an
independently authorized Hawala provider. Neither node can unilaterally spend
the other's credit lots or collateral. Different scopes remain fenced.

## Acceptance roadmap

1. **Interface/spec (no money movement):** Freeze `HawalaCapabilityV1` ports,
   command/result envelopes, discovery, status, source authority and exact
   signing domains. Publish a cross-repository test-vector matrix. Complete
   ADR 0004 D1–D3 regression work.
2. **Two-node read-only pilot:** Run two **separately administered** biz-kit
   processes and two real Iroh Hawala peers, explicit identity proof and
   source-attributed read-only evidence; restart/partition/replay tests.
   Public listing discovery through ActivityPub stays independent.
3. **Synthetic payment orchestration:** Compose a test commerce checkout and
   test payout with Hawala's M6-C7-admission simulator and M6-C6 coordination
   model; keep all results non-executable and clearly test-only. Test
   transaction QR, no cross-node shortcut, expiry, duplicate event, delayed
   receipt, unilateral abort, partition and recovery. Add a third peer with
   no authority to demonstrate routing separation.
4. **Scoped authority refinement:** Prove accounting, integer range,
   governance fence, one-append atomicity, custody/epoch fencing and
   per-authority succession against both Rust and biz-kit hosts. Do not
   reuse `CapitalAccounts` as double-entry accounting.
5. **Deployment release gate:** Independent asset-control reconciliation,
   verified real-custody interfaces, licenses, AML/KYC, privacy, independent
   security/legal review and operator obligations. No production payment,
   credit or ATM operations before these gates.

## Explicit non-goals and compatibility

This proposal does **not** create a network-wide ledger, require every
biz-kit operator to be a custodian, migrate Hawala to ActivityPub, make
gossip consensus, share user/merchant balances by default, or displace
independent non-biz-kit Hawala peers. ADR 0004 D1–D6 remain intact:
D4 forbids a **network-wide** ledger, not native Hawala functionality on
participating nodes; D5 permits **scoped** authority; D6 controls
cross-authority clearing. This ADR must be reviewed and merged together
with its paired biz-kit proposal.
