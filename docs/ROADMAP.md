# Hawala Core — engineering roadmap

**Scope:** decentralized hawala obligations, trust, clearing and settlement
over Iroh. QR plus a readable name/code is the participant presentation;
identity, verified attributes and reputation are reusable but financially
non-authoritative. Human agents, merchants and ATMs are adapters. Cash,
app credits and gift cards are downstream products, not the core protocol.

## Milestones and acceptance gates

| Milestone | Status | Required evidence before advancing |
| --- | --- | --- |
| M1 QR identity, verified evidence, ratings | PRs #1–#2 merged | Rust CI, adversarial proof checks; original merges alone do not prove deployment readiness |
| M2 Authenticated Iroh endpoint ↔ QR identity association | PR #3 merged | Compile against pinned Iroh; tests for wrong endpoint/QR, expiry, signed-context mutation and nonce reuse |
| M3 Two-party obligations and distinct payout/settlement evidence | PR #4 merged | Full signature/FX tampering tests, order-independent reconciliation and equivocation detection |
| M4 Real authenticated on-wire Iroh protocol | M4-A merged (#6), local two-peer verification green | Two live peers, bounded framed messages, reciprocal authentication, protocol versioning, message size limits; no 0-RTT for financial actions |
| M5 Durable financial execution journal | M5-A experimental intent journal PR #8; awaiting CI | Crash-replay tests, authenticated atomic idempotency/reservation, definite vs indeterminate cash-dispense outcomes, external machine reconciliation |
| M6 Bilateral trust, liquidity and compliance | Not started | Explicit authorized operating jurisdictions/roles, verified current financial eligibility, limits and risk decisions, auditable denial |
| M7 Clearing and real settlement | Not started | Double-entry posting, external settlement attestations, netting only with counterparty agreement, no payout↔settlement equivalence |
| M8 Human agent, merchant, ATM and gift-card adapters | Not started | QR-only identity+name/code, independent transaction-bound signed authorization, regulated providers, dispute/chargeback controls |

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
- **M5-A** Introduce a file-backed single-authority payout intent journal
  with durable one-way attempts and crash recovery to `uncertain` (PR #8).
  No cash hardware, customer balances or actual disbursement authority.
- **M5-B** Require trusted real hardware outcome evidence, audited
  reversals/disputes and double-entry accounting before any cash operation.
- **M5-A** Prototype a crash-safe single-authority obligation journal with
  payment states (reserved, dispensing, confirmed, uncertain, reversed).
  Simulate power loss between external cash execution and local acknowledgement.
- **M5-B** Add signed peer reconciliation with intentionally conflicting
  evidence, deterministic refusal, and audit/event export.
- **M6-A** Implement independent bilateral trust/exposure policies and
  current agent/regulatory eligibility checks before ANY money movement.

**Delivery rule:** Security-sensitive code is not merge-ready just because
GitHub reports a PR as mergeable. Require completed CI and targeted review;
a running two-peer transport prototype still does not authorize custody or
movement of real customer money.
