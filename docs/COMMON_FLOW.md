# Universal human/machine Hawala flow binding (M6 interface)

`hawala-flow` binds a signed interaction agreement to a **specific**
bilaterally accepted Hawala obligation. It is deliberately a *pure*
structural validator: it cannot authorize cash, verify a device operator,
settle funds or release custody.

## Same contract, all participants

A person or an automated device can occupy either role in a transfer leg.
The signed interaction already records both full QR identities, optional
machine operator claims, direction, operation, terms digest and nonce.

| Operation | Left legal/economic principal | Right principal |
| --- | --- | --- |
| AcceptCash | Sender | Origin hawaladar |
| DistributeCash | Beneficiary | Disbursing hawaladar |
| TransferValue | Origin hawaladar | Disbursing hawaladar |
| ReconcileObligation | Origin hawaladar | Disbursing hawaladar |

A **human** must sign using that principal's identity key. A **machine**
signs using its device identity key, while the interaction explicitly
names the economic principal as its supposed operator. Both directions
are valid. This mapping is identical for human/human, human/machine,
machine/human and machine/machine.

**Important:** A machine's claimed operator is not authenticated by this
mapping. Separate, contemporaneous issuer/operator-delegation policy MUST
prove that it acts for the principal, with operation/amount/currency limits,
jurisdiction, and an appropriate deposit or custody authority. Likewise,
self-presented participant kind is untrusted until corroborated.

`BoundFlow` is only a witness to matching signatures/parties/terms, not
a financial-authorization capability. It may not be sent to an ATM as an
authorization token. The system must still enforce eligibility, AML/KYC
requirements, sanctions, transaction-bound authenticator authorization,
ledger/custody reservation, and atomic at-most-once execution before
permitting any cash movement.

## Cash-in and cash-out

Cash-in: sender and origin agent agree to accept cash. An ATM can be
a machine operating on behalf of the origin. Deposit contents must be
authenticated and accepted before creating a redeemable customer claim.

Cash-out: beneficiary and disbursing agent agree to deliver funds. A
machine can represent the agent or an authorized beneficiary-side
collection endpoint, but this is not automatically permission to
dispense: responsible Hawala parties and custody controls remain fixed.

For physical fulfillment, proof sources differ:

- Human/human: recipient and agent confirmation plus independent dispute
  and accountable funds/cash-handoff evidence.
- Human/machine: signed device telemetry alongside beneficiary consent,
  accountable operator, denomination inventory and external reconciliation.
- Machine/machine: signed bilateral automation/clearing instructions and
  independently trusted machine delegations; *not* proof that notes changed
  hands or that a bank settled.

The durable journal's one-time attempt must be claimed before invoking an
external actuator. Once the external outcome may be uncertain, the journal
must not automatically retry.

## Security and test obligations

- Verify the **full** QR public identities and fresh signatures. Names and
  shortened codes aid human recognition but never bind financial authority.
- `InteractionIntent` includes nonce and a <=300 second expiry window.
  The current flow crate validates the signature and expiration but does
  **not** persist or consume the nonce: callers need durable replay guards.
- No global identity revocation mechanism. Operating permissions, expired
  badges, stolen keys and key succession require separate current policies.
- Two-agent signature acceptance is distinct from beneficiary consent and
  payout/settlement finality.
- Tests exercise all four oriented pair types in cash-out, cash-in,
  transfer and reconciliation, plus inverted initiator direction and
  mismatched agents, terms, operations, and expiry.

## Next integrations

1. Dedicated current-eligibility and machine-delegation proofs checked
   against independent policy, including collateral and liquidity.
2. Durable nonce/attempt binding **atomically** with the financial journal
   (avoid a TOCTOU gap between validating and reserving).
3. Cash-in journal with pending/nonredeemable notes and inventory evidence.
4. Unified operator gateways for human handoff, ATM disbursement, and
   machine-to-machine settlement coordination.
5. End-to-end Iroh live-peer tests: human/human simulated handoff,
   human/ATM simulated withdrawal and two authorized automated peer nodes.

**Not production-ready:** no connected custodian, no real ATM interface,
no human custody evidence, no active financial-control authority.
