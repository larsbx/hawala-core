# Hawala Core — engineering roadmap

**Scope:** decentralized hawala obligations, trust, clearing and settlement
over Iroh. QR plus a readable name/code is the participant presentation;
identity, verified attributes and reputation are reusable but financially
non-authoritative. Human agents, merchants and ATMs are adapters. Cash,
app credits and gift cards are downstream products, not the core protocol.

## Delivery strategy — ATM-first

**Automated tellers are the first operational cash-in/cash-out interface.**
Human hawaladars remain financially responsible counterparties when used in
a transfer, but initial physical access prioritizes unattended cash terminals
and cash-recycling devices. Merchants and digital gift cards follow once
ATM payments and reconciliation have proved safe. ATM operators are parties
with separate custody, hardware keys, liquidity and eligibility conditions.

| Priority | Milestone | Current state | Exit criteria |
| --- | --- | --- | --- |
| P0 | QR + name/code, identity and Iroh | PRs #1–#3, #6 merged | Live mutual peer authentication; no sensitive data on public gossip |
| P0 | Accepted bilateral Hawala obligations | PR #4 merged | Signed exact terms, FX acceptance, payout != settlement |
| P0 | Secured payment and deposits specification | PR #5 merged | External custody, liability policy and legal controls before real funds |
| P0 | Durable replay and payout intent | PRs #7–#8 merged | No repeated attempts across crash/reconnect |
| **P0** | **ATM identity, capability and outcome reporting** | **PR #9 in progress** | Signed terminal evidence, trusted terminal/attempt binding; ambiguous or partial payouts frozen |
| P0 | ATM normalized event simulator and provider contract | PR #10 in progress | Full/partial/retract/jam/no-sensor scenarios; immutable signed report |
| P0 | ATM operator integration (XFS4IoT or contracted network API) | Not started | Authorized device and operator, cash-out mock and vendor sandbox proof |
| P0 | ATM terminal deposit/recycling | Not started | Deposit escrow pending authenticated note acceptance and settlement, recycler inventory reconciliation |
| P0 | Durable transaction-specific QR authorization | Not started | Authenticator-bound QR, funds held, safe payout gate, full audit |
| P1 | Real bilateral trust, liquidity and agent eligibility | Not started | Limits, legal authority, regulated custody and hard-stop policy |
| P1 | Clearing, double-entry and custody settlement | Not started | Journal balances and settlement evidence; no fake finality |
| P2 | Merchant/human teller UI and digital gift cards | Not started | Reuses verified ATM cash protocol and independent compliance gates |

No automated teller may accept or dispense real customer funds on the basis
of current prototype code. Priority does not relax financial compliance,
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
