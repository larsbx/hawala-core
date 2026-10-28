# ADR 0005: Hawala as a network-native capability of federated biz-kit

Status: **Proposed**, 2026-10-28. Paired with biz-kit's
[network-native Hawala profile](https://github.com/larsbx/biz-kit/blob/main/docs/architecture/2026-10-28-federated-hawala-native-capability.md).
N1–N7 are mirrored verbatim; changes require the same reviewed change in both repositories.
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

<!-- HAWALA_NORMATIVE_BEGIN -->
**N1. One Hawala capability plane per participating biz-kit instance.**
A biz-kit installation may host one locally administered Hawala-capability
plane: signed identity/reputation evidence, authenticated Iroh peer transport,
quote/terms and payment proposals, authority routing, recovery and status
projections, through versioned authenticated local ports. A supervised Rust
service/sidecar is preferred; pure Rust validators may be linked separately.
Calling this service never confers financial authority on a host, controller,
JVM/BEAM process or ActivityPub adapter. The host never holds participant
identity, consent or transaction-QR keys, and never signs for participants.
It may hold only its own node/operator keys in distinct signing roles. No
universal Hawala operator, required central server or singleton identity
registry is introduced.

**N2. Business, custody, financial authority and coordination are separate roles.**
An installation may be a business home, participant/merchant peer, routing or
observer peer, hawaladar operator, terminal operator, or one explicitly named
financial authority for a scoped portfolio. None of the business, discovery,
identity or routing roles grants custody or financial admission powers. A
separate **scoped decision coordinator** may order commit/abort only for the
manifest and authority scopes jointly accepting its mandate, key, durable
order and epoch/succession policy. It cannot spend either authority's assets,
attest delivery/finality, sign participant consent or order other scopes.
The recovery resolver, fence operator, execution observer and settlement
attestor require their own explicit mandates. Operator roles retain independent
legal eligibility, local authorization and enforced scope.

**N3. Hawala owns canonical financial semantics.**
`hawala-core` remains the sole protocol/specification owner for QR-only identity
presentation, authenticated Iroh binding, dual-signed terms and transaction QR,
obligation provenance, trust/exposure, exact accounting, custody, FX, clearing,
payout/dispute lifecycle and `admit(operation, authority_envelope)`. It also
owns the [cross-authority evidence and blocking-commit profile](https://github.com/larsbx/hawala-core/blob/main/docs/CROSS_AUTHORITY_V1.md)
and its formal artifacts. Biz-kit owns versioned host adapters, business
grants, domain workflows/UI and source-attributed business evidence. Do not
fork a Hawala financial ledger/protocol into `commerce_core`, `group_buy_core`,
`federation_core`, `coop_substrate` or generic `CapitalAccounts`. Independent
non-biz-kit Hawala peers remain possible. These normative N1–N7 decisions are
mirrored verbatim; a change requires the same reviewed change in both records.

**N4. Exactly one named financial authority per declared scope.**
Each biz-kit business ledger owns its local business facts. Only when all six
ADR 0004 D5 conditions hold may an instance's serialized financial append
boundary be the single named Hawala financial authority for explicit scope S
of obligations, credit lots, claims, collateral and dar exposure. Pin scope,
operator, custody profile, acceptance/grant policy, keys, epochs, successor and
effective fencing. Other installations retain independently verified read-only
receipts/projections, never co-write S's balances. Without D5, use an independently
authorized Hawala authority. There is no network-wide shared biz-kit balance
ledger and no general financial event-store bridge.

**N5. Private Hawala coordination; HOLD on unresolved decisions.**
Iroh carries authenticated, signed end-to-end financial messages. ActivityPub
is limited to public listings/discovery, never financial authorization, private
KYC, transaction QR, custody instructions or payment finality. Cross-authority
commit uses one jointly mandated scoped coordinator's irreversible decision
order, signed prepares and participant ACKs under the canonical profile;
neither authority infers that its peer committed. Unilateral refusal is safe
only before durable prepare. After prepare, coordinator failure, partition,
stale/mismatched mandates or unresolved financial state require `HOLD`, retaining
locks, original IDs and evidence. Recovery requires an effective predecessor
boundary fence, both authority fences, complete authenticated decision history,
separate successor-installation ACKs and the named recovery mandate. No timer
unlocks reservations. Terminal commit/abort cannot change. M6-C6/C7 remain
local synthetic references; formal models do not implement distributed settlement.

**N6. Evidence-typed composition with explicit workflow gates.**
Checkout, dispatch fees, group-buy pledges, remittance, payout and ATM UIs start
as authorized proposals, never financial postings. Bind immutable business
ID/revision, network/attempt, parties, exact terms/amounts/fees, expiry, funding,
obligations, authorities/epochs, destination and permitted disclosure.
The canonical profile specifies exact domain-separated bytes and independently
pinned signers for `Proposed`, `Committed(intent)`, `Executed(observed)` and
`Settled`. Complete committed intent evidence requires both authority ACKs;
coordinator decision, execution observation and settlement evidence are distinct.
Cancelable order reservation may wait for committed intent; goods release,
paid dispatch, group-buy release and remittance-delivered status require
independently verified settled evidence plus local business conditions.
`Held(lastProven, reason)` blocks advancement and preserves uncertainty.
Preparation abort is possible only before terminal commit; subsequent refunds,
disputes and compensation cannot rewrite that decision and require new funded,
dual-signed obligations. No cross-instance ACID or real payment finality is
claimed. The financial authority independently admits before separately
authorized execution.

**N7. Proof-driven rollout before orchestration or custody.**
Preserve QR as the sole presented participant identifier with live challenge
proof, never bearer authorization. Keep participant/device consent keys off
the host and separate node, endpoint, mandate, observation and settlement roles;
the service independently refuses host/operator assertions of participant consent.
Pin disjoint signing domains, exact monetary encoding (unsigned big-endian
minor units in 0..2^63−1), replay-resistant IDs/nonces, independent local grants
and privacy/disclosure policy. Review Lean/Quint cross-authority agreement,
abort fencing, epoch/replay rejection, partition HOLD, independent crash/restart
and mandated recovery **before synthetic cross-instance checkout orchestration**.
Generated formal traces must replay against the actual Rust and Elixir hosts;
model-to-model checks alone do not satisfy that gate. The current proof increment
assumes authenticated messages, durable non-equivocating history and effective
fencing, and supplies no production mutation endpoint. All D5 accounting,
refinement, real custody, hardware, licensing, AML/KYC and independent
security/legal/operator gates remain in force. The architecture stays Proposed
until its open gaps are accepted and closed in both repositories.
<!-- HAWALA_NORMATIVE_END -->

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
   signing domains. Publish a cross-repository test-vector matrix. Review the canonical cross-authority Lean/Quint increment and typed
   evidence profile; complete ADR 0004 D1–D3 regression work.
2. **Two-node read-only pilot:** Run two **separately administered** biz-kit
   processes and two real Iroh Hawala peers, explicit identity proof and
   source-attributed read-only evidence; restart/partition/replay tests.
   Public listing discovery through ActivityPub stays independent.
3. **Synthetic payment orchestration, only after G1–G4 close:** Compose a test commerce checkout and
   test payout with the reviewed cross-authority profile and host conformance
   suite (M6-C6/C7 alone are insufficient); keep all results non-executable and clearly test-only. Test
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
state" needed an explicit definition. A reviewable typed evidence profile is
now specified in [CROSS_AUTHORITY_V1](https://github.com/larsbx/hawala-core/blob/main/docs/CROSS_AUTHORITY_V1.md);
acceptance and actual host/consumer verification remain open.
*Closes when:* a typed state sequence is specified. A minimal version is
`Proposed → Committed(intent) → Executed(observed) → Settled`, with
`Held(lastProven, reason)` preserving proven history. Preparation `Aborted`
is reachable only before terminal commit; post-commit compensation is a new
obligation, never an abort/rewrite. For each
state, the specification names:
- the authority that signs it;
- the exact signed bytes and their domain;
- what a relying instance verifies before acting on it.

Each consuming workflow (checkout, dispatch fee, group-buy release,
remittance) declares the minimum state it waits for.

**G2. Decision coordinator and liveness (N2, N5). Blocks step 3.** M6-C6's
"common decision order" across independent authorities is an atomic-commit
problem, so some party must provide the order. N2 now proposes that scoped
role under the canonical profile; mandate/fence implementation and independent acceptance remain open.
*Closes when:* N2 and N5 name a scoped **decision coordinator** role, with
mandate, keys and epoch fencing. Its liveness rules are stated:
- A participant may abort unilaterally only before it prepares.
- After prepare, coordinator failure leaves reservations in `Held`.
- The rules name who resolves a `Held` reservation, within what bound, under
  which mandate, and with what evidence retained.

**G3. Distributed commit model before orchestration (N7). Blocks step 3.**
M6-C6 and M6-C7 alone model only one local journal. The new
`CrossAuthority.lean` / `crossAuthority.qnt` increment models independent
participants and blocking recovery. Persistent Rust/Elixir TEST adapters now
replay the selected formal corpus through separate local journals. Actual
financial-writer and custody-boundary refinement remains open. A restricted
signed TEST RPC/outbox profile now checks lost replies and query-only recovery;
its `test_reserved` state never permits payment, fulfillment or escrow release.
Actual cross-instance financial checkout orchestration remains gated by G1–G8.
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

*Status:* specified and implemented in hawala-core #52 (`hawala-remedy`,
`docs/REMEDIES.md`): remedies are new linked dual-signed obligations; consent
comes from the party whose money moves, or an award from the pinned
adjudicator, with one award per case; cumulative caps prevent double
recovery; disputes hold and move nothing; and each G1 workflow has a dispute
profile. The adjudication service, the finality-policy encoding of the
adjudicator, and remedy execution remain open.

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

*Status:* specified and implemented for scope authorities in hawala-core #47
(`hawala-authority-descriptor`, `docs/AUTHORITY_DESCRIPTOR.md`). Pinning of
execution observers, settlement attestors and coordinators, the discovery
transport, durable pin storage and the biz-kit relying-party side remain open.

**G6. Capability port authentication (N1). Blocks step 1.** N1 says
calling the host confers no authority, but leaves two questions open: how
the Rust service authenticates its Elixir caller, and how a biz-kit grant
maps to a permitted Hawala operation.
*Closes when:* the local port has mutual authentication, a version
handshake and per-call grant binding. The service re-verifies every
participant signature itself and never accepts the host's assertion of
consent.

*Status:* host-signed, pinned, versioned, grant- and body-bound admission
and service-signed responses were merged in hawala-core #43. The restricted
TEST endpoint increment adds exact mutually signed local RPC, pinned TEST grants,
durable replay storage and a query-only business outbox. It carries no participant
consent or financial evidence. Actual capability negotiation, participant-signature
verification and mapping biz-kit grants to production `GrantPolicy` remain open.

**G7. Disclosure model (N6). Blocks step 2.** N6 binds "permitted
disclosure" into the correlation record, but no model defines it.
*Closes when:* the specification states, per role (merchant, routing peer,
coordinator, authority, observer), which fields of terms, receipts and
status each one may see. Routing and observer peers see no amounts or
counterparties beyond what routing needs.

*Status:* specified and implemented in hawala-core #51 (`hawala-disclosure`,
`docs/DISCLOSURE.md`). Its digest is the manifest's
`permitted_disclosure_policy_digest`. Finding: v1 manifests force leg agents
to over-disclose, because evidence signs the whole manifest. Per-leg
commitments are proposed as the v2 fix. Encryption of routed messages and
hiding correlation identifiers remain open.

### Keys and identity

**G8. Participant keys stay on participant devices (N1, N7). Blocks step 1.**
The capability host must never hold participant identity, consent or
transaction-QR keys. Accepted terms and transaction QR are signed on the
customer's or merchant's own device, in line with biz-kit's rule that no
server signs in a person's name.
*Closes when:* N1 and N7 state this. A host-side test refuses any request
to sign on a participant's behalf. The host's own node and operator keys
stay distinct from participant keys.

*Status:* N1 and N7 now state the rule. The pure service-side refusal of
host-signed consent was merged in hawala-core #43. The biz-kit host-side
refusal and transport/durable replay integration remain open.

**G9. Signing-domain guard and fixture integrity (ADR 0004 D3). Blocks
step 1.** The interoperability vectors exist, but three follow-ups remained:
- **Runtime guard:** no biz-kit signer was prevented from signing bytes in
  Hawala's signing domain.
- **Fixture drift:** the two fixture copies could diverge silently.
- **Wording:** `0x68` was described as "not a CBOR header". Read as a CBOR
  initial byte it is a text-string head (major type 3), never a map head
  (major type 5).

*Closes when:*
- biz-kit's ledger signer refuses to sign or verify any input beginning
  with `hawala`. The check is byte-level, so it covers every key.
- A biz-kit test pins every other direct Ed25519 call site to a
  non-`hawala` prefix.
- Hawala keeps every signing domain under the `hawala` prefix.
- Both tests pin the fixture's SHA-256.
- The wording states both readings of `0x68`.

*Status:* all criteria implemented, in review in hawala-core #41 and
biz-kit #172.

**G10. Exact money encoding (D5.3, N7). Blocks step 4.** N7 requires
exact encodings. The proposed canonical profile now selects 0..2^63−1
unsigned big-endian minor units and publishes both boundaries; actual host
schema, arithmetic and accounting range refinement remain open.
*Closes when:* the specification fixes one encoding for amounts on the wire
and in the biz-kit ledger. Either amounts are capped at 2^63−1 minor units
everywhere, or they use decimal strings with a canonical grammar. The
specification also publishes vectors at both range boundaries.

### Process

**G11. Lockstep text for N1–N7.** The proposed paired increment now mirrors
N1–N7 verbatim, with Hawala owning the canonical financial profile and formal
artifacts. A biz-kit source pin/check verifies the exact reviewed Hawala
commit and text/artifact digests. Acceptance in both repositories remains
required; identical proposed wording alone does not close the architecture.
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
