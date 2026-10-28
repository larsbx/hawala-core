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

## Open gaps (must close before Accepted)

This section is **identical** in hawala-core ADR 0005 and in biz-kit's
network-native Hawala record. Edit it in both, in the same change. A gap
closes only when its exit criterion is merged in both repositories. The
acceptance step it blocks cannot start before then. Steps refer to the
five-step acceptance sequence above, which both records number the same way.

### Protocol semantics

**G1. Finality states (N6). Blocks step 3.** "The required financial
state" that lets a business workflow advance is undefined. Without it, a
merchant cannot tell a reserved payment intent from being paid.
*Closes when:* a typed state sequence is specified. A minimal version is
`Proposed → Committed(intent) → Executed(observed) → Settled`, with
`Held` and `Aborted` reachable from every state before `Settled`. For each
state, the specification names:
- the authority that signs it;
- the exact signed bytes and their domain;
- what a relying instance verifies before acting on it.

Each consuming workflow (checkout, dispatch fee, group-buy release,
remittance) declares the minimum state it waits for.

**G2. Decision coordinator and liveness (N2, N5). Blocks step 3.** M6-C6's
"common decision order" across independent authorities is an atomic-commit
problem, so some party must provide the order. N2 lists no role allowed to
order decisions across scopes.
*Closes when:* N2 and N5 name a scoped **decision coordinator** role, with
mandate, keys and epoch fencing. Its liveness rules are stated:
- A participant may abort unilaterally only before it prepares.
- After prepare, coordinator failure leaves reservations in `Held`.
- The rules name who resolves a `Held` reservation, within what bound, under
  which mandate, and with what evidence retained.

**G3. Distributed commit model before orchestration (N7). Blocks step 3.**
Step 3 composes a cross-authority checkout, but M6-C6 and M6-C7 model only
one local journal.
*Closes when:* a Quint/Lean model of the cross-authority commit is merged.
It covers coordinator and participant messages, crashes and restarts,
timeouts, partitions, duplicate and replayed messages, and abort fencing.
It proves agreement (no two participants decide differently) and the
`Held` behaviour of G2. Its generated traces are the step-3 conformance
suite for the Rust and Elixir hosts.

**G4. Refunds, disputes and compensation (N6). Blocks step 3.** N6 cites
reconciliation and compensation without defining them. ADR 0003 forbids
unilateral reversal of a completed spend.
*Closes when:* refund and compensation are specified as **new** dual-signed
obligations, never as rewrites of committed ones. Dispute entry, evidence
retention and the adjudicating authority are named for each workflow in G1.

### Authority, trust and privacy

**G5. Authority discovery and trust pinning (N4, N5). Blocks step 2.**
There is no global registry. The ADRs do not say how a relying instance
learns which authority orders scope S, or how it pins that authority's
keys, epochs and successor.
*Closes when:* a signed, versioned authority descriptor is specified,
covering scope, keys, epoch, custody profile, successor and fencing policy.
Relying-party pinning and rotation rules follow ADR 0001: local trust
policy, no global revocation. Discovery may advertise a descriptor, but
only the pinned policy confers trust.

**G6. Capability port authentication (N1). Blocks step 1.** N1 says
calling the host confers no authority, but leaves two questions open: how
the Rust service authenticates its Elixir caller, and how a biz-kit grant
maps to a permitted Hawala operation.
*Closes when:* the local port has mutual authentication, a version
handshake and per-call grant binding. The service re-verifies every
participant signature itself and never accepts the host's assertion of
consent.

**G7. Disclosure model (N6). Blocks step 2.** N6 binds "permitted
disclosure" into the correlation record, but no model defines it.
*Closes when:* the specification states, per role (merchant, routing peer,
coordinator, authority, observer), which fields of terms, receipts and
status each one may see. Routing and observer peers see no amounts or
counterparties beyond what routing needs.

### Keys and identity

**G8. Participant keys stay on participant devices (N1, N7). Blocks step 1.**
The capability host must never hold participant identity, consent or
transaction-QR keys. Accepted terms and transaction QR are signed on the
customer's or merchant's own device, in line with biz-kit's rule that no
server signs in a person's name.
*Closes when:* N1 and N7 state this. A host-side test refuses any request
to sign on a participant's behalf. The host's own node and operator keys
stay distinct from participant keys.

**G9. Signing-domain guard and fixture integrity (ADR 0004 D3). Blocks
step 1.** The interoperability vectors exist, but three follow-ups remain:
- **Runtime guard:** nothing yet prevents a biz-kit signer other than
  envelope signing (device statements, capability documents) from using a
  key bound to a Hawala identity.
- **Fixture drift:** the two fixture copies can diverge silently.
- **Wording:** `0x68` is described as "not a CBOR header". Read as a CBOR
  initial byte it is a text-string head (major type 3), never a map head
  (major type 5).

*Closes when:*
- Binding a key to a Hawala identity excludes it from every non-envelope
  signer.
- Both tests pin the fixture's SHA-256.
- The wording states both readings.

**G10. Exact money encoding (D5.3, N7). Blocks step 4.** N7 requires
"i64/u64-safe" encodings without choosing one.
*Closes when:* the specification fixes one encoding for amounts on the wire
and in the biz-kit ledger. Either amounts are capped at 2^63−1 minor units
everywhere, or they use decimal strings with a canonical grammar. The
specification also publishes vectors at both range boundaries.

### Process

**G11. Lockstep text for N1–N7.** The paired N1–N7 texts differ in
substance. Only hawala-core's N1 rules out a universal operator, a central
server and a singleton registry. Only hawala-core's N5 requires retaining
locks and original IDs on `HOLD`.
*Closes when:* N1–N7 are reconciled into identical normative text, or one
record becomes normative and the other points to it.

## Explicit non-goals and compatibility

This proposal does **not** create a network-wide ledger, require every
biz-kit operator to be a custodian, migrate Hawala to ActivityPub, make
gossip consensus, share user/merchant balances by default, or displace
independent non-biz-kit Hawala peers. ADR 0004 D1–D6 remain intact:
D4 forbids a **network-wide** ledger, not native Hawala functionality on
participating nodes; D5 permits **scoped** authority; D6 controls
cross-authority clearing. This ADR must be reviewed and merged together
with its paired biz-kit proposal.
