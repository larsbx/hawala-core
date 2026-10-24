# Peer interaction matrix — humans and machines

Status: protocol **interaction evidence** implemented in
`hawala-interaction`; no real financial transaction runtime exists.

## Foundational principle

**Hawala is P2P between responsible participants, independent of whether
a participant is a person or an automated device.**

Human and machine are kinds of participants, not separate financial
networks. The common protocol carries fully identified cryptographic
participants, operation, reference to the already agreed Hawala obligation,
nonce and a narrow signed validity interval.

Every human-facing identity presentation includes the full public identity
QR and adjacent human-readable name or code. Machines are likewise enrolled
with an authorized QR identity/name/code. For unattended **machine ↔ machine**
operation, controllers first exchange those QR-backed full public identities
under an authenticated operator provisioning agreement; after that the
machines must present fresh possession/session proofs over Iroh. A public
nickname, discovery record, short code, or copied QR cannot authorize money
movement.

## Equally supported interaction modes

| Mode | Example | Execution-specific evidence |
| --- | --- | --- |
| Human ↔ human | Customer → hawaladar; hawaladar ↔ hawaladar | Identity proofs, agreed terms, signed receipt and physical handoff evidence |
| Human → machine | Customer → ATM deposit or cash withdrawal | Customer authorization, approved ATM operator and signed device result |
| Machine → human | ATM/gateway-initiated request for operator or recipient confirmation | Signed machine request and fresh human consent |
| Machine ↔ machine | ATM ↔ ATM/gateway; cash recycler ↔ operator system | Independent device keys, verified operator delegation, signed instructions and custody/settlement evidence |

Orientation matters: a machine requesting human consent is not equivalent
to a human issuing a command to a machine. Both still use the same
`InteractionIntent` and dual-signed `AgreedInteraction`.

## Identity and capabilities

- Every participant uses `IdentityId`, resolved from the **full QR**, not
  a short human code or an account number.
- Human peers sign using their possession-verified identity key; they may
  separately represent a licensed business/organization.
- A machine has its own **device identity** and a separate accountable
  **operator identity**. The operator must have authorized the device and
  its current capabilities independently. Device attestation is not a
  license, and machine ownership is not inferred from the machine's
  self-asserted `operator` field in a proposal.
- Peer kinds, operator IDs, and proposed actions are signed to prevent
  alteration, but the verifier MUST check the truth of these assertions
  against trusted, current policy. A signature is not a verified badge.

## Cash handling and custody

The same Hawala obligation can be fulfilled through a human handoff or
an ATM; the obligation ledger does not need a new transfer schema for each.

- **Human dispensing:** separate signed payer/recipient completion evidence
  with dispute handling, physical funds accountability and a durable
  one-attempt handoff journal.
- **Machine dispensing:** signed terminal observation, authenticated gateway,
  device sensor/cash inventory reconciliation and the same no-duplicate
  execution rule.
- **Machine/machine:** ordinarily obligations, liquidity, clearing or
  authenticated infrastructure operations. Two ATM endpoints talking over
  Iroh does NOT physically deliver cash or settle liabilities without
  appropriate hardware, custody and externally verified settlement.
- **Cash-in:** humans and machines record *proposed/pending* acceptance before
  any redeemable credit. Counterfeit screening, denomination reconciliation,
  funding settlement and regulatory authorization are mandatory.

## Signed interaction evidence

`hawala-interaction` provides `InteractionIntent` and `AgreedInteraction`
for all four orientations. The exact action, both identities and kinds,
machine operator identities, terms digest, nonce and expiration are signed
by both party keys. It rejects swapped participants and modified context.

**This alone does not authorize a transaction.** A caller must also prove:
the accepted Hawala obligation, local current permissions and financial
eligibility, agreement to transaction-specific amount/currency/fees,
funds/custody reservation, Iroh session freshness and durable idempotency.
For unattended devices, a signed, scope-limited delegation and its
remaining limit are additionally required.

## Rollout sequence

1. Common pairing/intents and adverse-signature tests across ALL modes.
2. Same signed Hawala obligation implemented in an **offline two-human**
   and a **simulated human-ATM** flow.
3. Simulated **machine-machine** authorized operator/device messaging and
   deterministic reconciliation without public financial gossip.
4. Human and ATM payout adapters share the durable journal and explicit
   uncertainty recovery, without falsely equating a human receipt and an
   ATM sensor event.
5. Run provider sandbox tests and compliance/custody reviews before real
   cash; production rollout may prioritize ATMs for operational scale
   without disabling or delaying human↔human participation.
