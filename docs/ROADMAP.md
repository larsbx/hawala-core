# Hawala Core — engineering roadmap

**Scope:** decentralized hawala obligations, trust, clearing and settlement
over Iroh. QR plus a readable name/code is the participant presentation;
identity, verified attributes and reputation are reusable but financially
non-authoritative. Human agents, merchants and ATMs are adapters. Cash,
app credits and gift cards are downstream products, not the core protocol.

## Delivery strategy — unified human/machine peers; ATM-first hardware

**All peer pairings are in scope from the beginning.** A Hawala peer can be
a human or a machine; each interaction is handled by the same identity,
cryptographic trust and obligation layers:

- **Human ↔ human:** individual ↔ hawaladar, hawaladar ↔ hawaladar,
  person ↔ authorized merchant representative.
- **Human ↔ machine:** user ↔ ATM or cash recycler, operator ↔ device;
  machine-initiated interactions are supported as distinct directions.
- **Machine ↔ machine:** ATM ↔ authorized gateway/other ATM, automated
  settlement node ↔ node. No unsupervised financial authority follows
  from machine status alone.

QR plus name/code applies to *both* human and machine identities.
Machine-to-machine identities are established through QR-scanned or
QR-backed authorized provisioning, then cryptographically proved over Iroh
during automated interactions; an ordinary gossip record is not proof
of QR enrollment or delegation.

**ATMs remain the first hardware integration priority**, not the exclusive
participation mode or a prerequisite for human-to-human Hawala. Human
cash-handling agents should reuse the same protocol and reconciliation
invariants, with human receipts replacing device sensor attestations.
Business agents and machines still require separately verified legal
status, eligibility, bilateral limits, liquidity and funding permissions.

| Priority | Milestone | Current state | Exit criteria |
| --- | --- | --- | --- |
| **P0** | **Human/human, human/machine, machine/machine symmetric interactions** | PR #11 merged; signed exact-context agreements | Signed contexts, no unverified machine authority |
| P0 | QR + name/code, identity and Iroh | PRs #1–#3, #6 merged | Live mutual peer authentication; no sensitive data on public gossip |
| P0 | Accepted bilateral Hawala obligations | PR #4 merged | Signed exact terms, FX acceptance, payout != settlement |
| P0 | Secured payment and deposits specification | PR #5 merged | External custody, liability policy and legal controls before real funds |
| **P0** | **Unified flow binding across people and machines** | PR #12 merged | Verify each signer represents the exact economic principal from dual-agent accepted terms, any orientation; no financial authority |
| P0 | Durable replay and payout intent | PRs #7–#8 merged | No repeated attempts across crash/reconnect |
| **P0** | **ATM identity, capability and outcome reporting** | **PR #9 merged** | Signed terminal evidence, trusted terminal/attempt binding; ambiguous or partial payouts frozen |
| P0 | ATM normalized event simulator and provider contract | PR #10 merged | Full/partial/retract/jam/no-sensor scenarios; immutable signed report |
| P0 | ATM operator integration (XFS4IoT or contracted network API) | Not started | Authorized device and operator, cash-out mock and vendor sandbox proof |
| P0 | ATM terminal deposit/recycling | Not started | Deposit escrow pending authenticated note acceptance and settlement, recycler inventory reconciliation |
| P0 | Durable transaction-specific QR authorization | Not started | Authenticator-bound QR, funds held, safe payout gate, full audit |
| P1 | Real bilateral trust, liquidity and agent eligibility | Not started | Limits, legal authority, regulated custody and hard-stop policy |
| P1 | Clearing, double-entry and custody settlement | Not started | Journal balances and settlement evidence; no fake finality |
| P0 | Human-agent cash handoff and signed receipt evidence | PR #13 open | Same agreed obligation and replay safety as ATM; recipient and agent agree and sign, dual custody verification |
| P2 | Merchant POS and digital gift cards | Not started | Reuses common peer protocol, financial eligibility and independent compliance gates |

No human or automated teller may accept/disburse real customer funds on the
basis of current prototype code. Priority does not relax financial compliance,
provider agreements, cash-in authentication, or independent security review.

## Security invariants

1. The public QR key is authoritative. The visible name/code only helps human
   confirmation and is never a transaction credential or unique routing key.
2. Verified-peer sessions derive from Iroh authenticated `Connection::remote_id`
   and short-lived domain-separated signed identity proof. Require **mutual**
   proof before confidential exchanges; do not trust client-supplied endpoint IDs.
3. Identity-core contains no global revocation or unilateral account-disabling
   facility. Historical signed evidence persists; expired claims cannot
   establish current status. Key compromise and succession need an explicit
   design review before real funds.
4. Signed credentials and reputation do not confer authority to handle money.
   Current permissions, KYC/AML, sanctions, agent licensing, bank/ATM rules,
   jurisdiction, bilateral credit limits and liquidity live in Hawala policy.
5. An accepted obligation is signed by both hawaladars. Fulfillment and
   settlement have **separate** authenticated evidence and accounting.
6. Idempotent *evidence merging* is NOT at-most-once cash execution. No cash
   payout without a durable execution authority and reconciliation of
   uncertain external outcomes after device/network failure.
7. No public gossip or global replication of identifiable customer financial
   activity. Gossip/public receipts must use minimal privacy-safe data; full
   details flow only between authorized parties and legally required auditors.
8. No nominal `verified` badge from a self-asserted displayed name; the
   badge needs trusted, scope-specific, bounded evidence.

## Next concrete implementation tasks

- **M4-A** Completed in merged PR #6: bounded, versioned mutual Iroh handshake
  comparing independently scanned full QR identities and authenticated endpoints.
- **M4-B** Expand binary wire interoperability vectors and live adversarial
  peer tests (malformed/reordered/replayed frames, intentionally mismatched
  QR expectations, relay/network partition and reconnect). No critical
  extension fields are supported in the current exact-length v1 frames.
- **M4-C** Local crash-durable SQLite identity nonce guard and 20-second
  per-frame deadlines implemented in PR #7. Distributed replay authority
  and clock-skew policy remain open.
- **PEER-A** Implement signed human/machine interaction intents with a full
  QR-identity pair, directional participant kinds, role/operation, nonce,
  accepted Hawala obligation digest and expiry. Require both signatures;
  no financial authority follows from participation.
- **PEER-A2** Bind all human/device interaction intents to the exact bilateral
  Hawala obligation and correct principals; use one common validation path
  for cash-in, cash-out, transfer, reconciliation (PR #12).
- **PEER-B** Integrate verified Iroh sessions, durable replay consumption,
  trustworthy person/terminal roles, and human receipts or device telemetry
  into one transaction-flow adapter (not yet implemented).
- **ATM-A** Sign and verify one immutable outcome observation per authorized
  ATM attempt. Bind hardware key, full transaction terms, amount/currency,
  attempt ID and private device-log hash; classify events conservatively.
- **ATM-B** Map XFS4IoT ItemsPresented, ItemsTaken, retract/jam and unknown
  statuses through a provider-specific gateway. Signed gateway reports are
  claims, not independently proven customer delivery.
- **ATM-C** Build a simulated cash dispenser/recycler with deterministic
  interruption scenarios and callback reconciliation. Include deposits as
  non-spendable pending claims until cash acceptance is confirmed.
- **ATM-D** Integrate temporary, authenticator-bound QR authorization with
  Hawala's credit/custody reservation before an ATM cash command.
- **M6-A** Bilateral exposure/eligibility hard stop, custody integration,
  regulatory controls and financial double-entry remain prerequisites.

**Delivery rule:** Security-sensitive code is not merge-ready just because
GitHub reports a PR as mergeable. Require completed CI and targeted review;
a running two-peer transport prototype still does not authorize custody or
movement of real customer money.
