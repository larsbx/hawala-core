# M6-C — one authoritative financial commit boundary (implementation contract)

Status: **normative implementation plan; no production financial authority exists.**

## Purpose and constitutional boundary

Hawala remains a decentralized Iroh P2P network. The origin dar and
disbursing dar remain the named obligors; no globally trusted wallet
server and no automatic legal novation are introduced.

**Every change to spendable customer claims or dar exposure must be ordered
by exactly one named financial authority under an explicit custody/consensus
profile.** This may be a single legally authorized asset custodian, an
escrow-backed operator, or a properly defined independent controller
federation. All paths must establish one durable ordering for each credit
lot and collateral allocation across failures and partitions. A public
Iroh gossip message is never that ordering.

Use a single canonical `admit(operation, authority_envelope)` for all
human↔human, human↔machine, machine↔machine flows. Compile permitted role,
currency, limit and jurisdiction policies into a locally verifiable
versioned policy profile. Routine transactions should take a direct
single-gate path; disputes/uncertainties are separately handled.

## Inputs (all required, no inferred permissions)

The envelope for payout / redeem includes:

1. Dual-hawaladar accepted terms and executable FX quote, with recipient
   accepting payout currency, net amount, exact fee, rounding and rate.
2. Full QR-presented identity keys and readable name/code; fresh Iroh
   mutually authenticated session/channel binding, with an explicitly
   delegated operator/controller for each machine.
3. Short-lived signed *transaction* QR authorization: amount, currency,
   payer, recipient, authenticated counterparty, attempt ID, nonce and
   expiry. Static identity QR is never cash authority.
4. Named credit lot(s) and financial issuer/obligor provenance. Each
   lot's liability agreement, backing dar and funding-finality proof must
   be retrievable and consistent with accepted terms.
5. Authority-policy digest/epoch, expected account versions, current
   operating grants, bilateral credit limits, AML/KYC/sanctions checks and
   jurisdiction. Identity/reputation evidence is not a license.
6. Confirmed custody/deposit reservation(s) under the **selected**
   per-account custody adapter, with current signed receipt, version,
   epoch, mandate, collateral ID and actual asset-control reconciliation.
   Both independent escrow and federated dar custody remain supported.
   A claim signed by a custodian alone is not real asset existence.
7. Unique idempotency/event ID and bounded authorized attempt ID. The
   caller must retain all conflicting evidence rather than overwriting.

## Required single commit effect

All steps below form one durable, serializable authority transition:

- Recheck the current grants, policy, quote, FX denominations and expiry
  **at commit time**, never relying solely on earlier local verification.
- Verify reserved collateral, available customer credit and dar exposure
  with one authoritative ordering. Reject missing, pending, unknown or
  version-mismatched custody evidence; DO NOT count unsigned balances.
- Atomically consume the QR nonce and unique event ID. Replayed identical
  events return the original durable result; different bytes under one ID
  are conflict evidence, never a new attempt.
- Reduce/fence available claims or reserve liability **once**, preserving
  underlying lot provenance and collateral allocations.
- Write a balanced accounting journal **per currency** with exact minor
  units. Cross-currency legs use distinct FX inventory/clearing accounts.
- Commit a single attempt record bound to accepted terms, beneficiary,
  dar, terminal/human identity and authority epoch.
- Persist an authenticated receipt/checkpoint and monotonically advanced
  account/authority versions BEFORE any external cash command.

This is **commit-before-execute**, not "sign-and-disburse".
A committed payout *intent* reserves claims but does not yet extinguish
liabilities or prove a physical payout. The authorized human/ATM adapter
receives an execution capability only after the financial commit, scoped
to the same attempt and guarded by the actual operator mandate.

## External execution and recovery

- A hardware/client timeout after submitting the command becomes
  `UNCERTAIN`. It must NEVER trigger a fresh cash command or release
  collateral automatically. Query actual hardware/custody outcomes under
  the same permanent attempt ID.
- Device sensor reports and bilateral human handoff signatures are
  evidence, not finality. A policy-approved dispute/settlement authority
  adjudicates irreconcilable outcomes; customer credit and collateral
  states are reconciled exactly once.
- If the authority, quorum, custodian, device or network partition cannot
  establish a safe committed order, result is `HOLD`, not a speculative
  credit or withdrawal.
- For federated custody, explicitly prove quorum intersection, controller
  independence and fencing across epochs. Never assume "some signatures"
  imply consensus or physical control of money.
- For independent escrow, establish independently the exact mandate,
  financial institution/custodian role, asset reconciliations and its
  authority to honor device/withdrawal requests.

## Narrow API and output contract

Input: `AuthorityEnvelope`, immutable proposed state/version and
authenticated caller.

Output:
- `Rejected(reason)`: provably no financial effect; invalid signature,
  incompatible FX/fees, expired approval or denied permission.
- `Held(reference, reason)`: uncertain custody, unresolved external
  command, inability to agree on ordering or compromised availability;
  credit remains fenced; **not** a permit to retry.
- `Committed(intent_receipt)`: one durable, final **intent** commit and
  explicit authorization to attempt the one external payout. It is not
  proof that cash was delivered or external settlement completed.

The interface must not expose two separate local "success" results for a
nonce and a balance mutation. No public method may bypass `admit` to
release actual customer funds, even if low-level prototype methods remain
available for testing.

## Adversarial acceptance criteria

- Concurrent identical and conflicting redemption attempts: one accepted
  ordering, all other attempts idempotent or rejected with retained proof.
- A crash after nonce consumption but before ledger mutation is impossible
  as a visible partial commit.
- A crash after financial commit but before physical action recovers the
  *same* attempt, without issuing a new one.
- Lost or delayed operator/custody acknowledgments: HOLD rather than
  spending unconfirmed assets or unlocking protected credits.
- Multi-currency exchange independently balances per currency and
  enforces a correctly accepted rational quote with all fees/rounding.
- Cross-device replay, stale authority epoch, untrusted delegate, forged
  escrow receipt, duplicate funding IDs, split/merge credit lots, offline
  partition and compromised terminal signatures all fail closed.
- End-to-end two-peer Iroh harness: human/human simulated handoff,
  human/ATM simulated terminal, two authorized machines; per-mode
  differences reside only in operator/physical evidence adapters.
- No real money or physical hardware enabled by acceptance tests alone:
  external legal, operational and independent security reviews are
  non-negotiable release gates.

## Implementation order

M6-C1: Typed `AuthorityEnvelope` and immutable `admit` decision surface
with policy-check adapters (pure, adversarial tests).

M6-C2: Durable signed custody receipt/version/epoch adapter and a
single-authority journal prototype that can atomically apply reference
ledger postings + QR nonce + cash attempt. No independent second DB commit
may be required for any of these fields.

M6-C3: Independent escrow and federated controller simulator, accounting
for partitions and epoch fencing.

M6-C4: Signed Iroh live peer exercises plus mock human and ATM actors;
unknown external outcomes remain held through crash/reconnect.

M6-C5: Replace mocks only through approved custody, verified device
operators and independent security/compliance gates.

This contract supersedes suggestions to equate the existing
`hawala-admission` or `hawala-ledger` outputs with real financial
authorization. They are verifier/model primitives only.
