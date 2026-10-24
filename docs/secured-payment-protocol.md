# Hawala secured payment protocol v0.1

Status: complete review draft of protocol requirements. MUST/MUST NOT are
normative. Deployment choices in section 12 are required configuration, not
implied approvals. No customer funds until those gates are satisfied.

## 1. Roles, claims and currencies

Roles: sender, receiver/current credit holder, origin dar, backing dar, payout
dar, collateral custodian, commitment authority and agreed dispute authority.
One actor can occupy multiple permitted roles, but required independent roles
MUST NOT collapse merely because keys differ. Each role binds a full identity
and an independently validated mandate.

A credit lot records its holder, currency, minor-unit amount, origin funding
reference, origin/backing obligors, liability-agreement digest and collateral
allocations. A transfer preserves that provenance even when split or merged;
merging MUST retain the constituent liability allocations. No unnamed network
debtor and no automatic legal novation.

Currency metadata is versioned: code, minor-unit exponent and supported payout
denominations. Amounts are positive unsigned integers, bounded to u64 minor
units in the initial safety kernel. Rates are positive rational numbers in
destination minor units per source minor unit. Display a separately labeled
human rate per major unit; never silently confuse these units.

## 2. Discovery and quotes

Customers independently provide approximate location and radius. Responses
MUST include signed listings with dar identity, business location, service
currencies, hours, listing expiry, fee schedule, quote availability and
reputation provenance/coverage. Exact customer locations and transaction details
MUST NOT be public gossip. Discovery MUST NOT expose a searchable public list
of customer balances or journeys. Query throttling and request expiry are required.

Indicative rates are not executable quotes. An executable quote binds all
participants and obligors, source gross/debit, conversion principal, source fee,
destination gross, destination fee, net recipient credit, rate, rounding,
currency metadata versions, funding method, redemption conditions, expiry,
liability/custody/dispute profile digests and a unique quote ID. All fees are
itemized; fees in an additional currency need separate explicit acceptance.

For the reference arithmetic profile:
source_debit = source_principal + source_fee;
destination_gross = floor(source_principal * rate_num / rate_den);
destination_credit = destination_gross - destination_fee.
The residual rounding fraction and the party benefiting from it MUST be
disclosed. Reject zero credit, overflow, unsupported precision and any mismatch.
Same-currency transfers MUST use 1:1 principal conversion; service fees remain
separate. No retroactive rate changes. Quote expiry is checked at commitment;
it never expires an already issued customer balance.

Receiver MUST accept the receiving currency, rate and net amount for that
specific exchange. No broad currency preference substitutes for transaction
acceptance in v0.1. Origin/backing dars and sender accept the same terms digest.
No executing a stale quote or changing terms without all required approvals.

## 3. Temporary QR and authenticator authorization

Persistent identity and temporary presentation are distinct. The financial QR
MUST contain/version-reference a signed authorization bound to holder identity,
verifier identity, authenticated endpoint/session, operation, exact terms digest,
unpredictable verifier challenge, issue/expiry times and unique nonce. Display
human-readable amount/currency/counterparty/fees in the authenticator before
approval. QR payloads MUST NOT contain private keys or shared TOTP seeds.

If TOTP is supported it only unlocks a local signer or contributes to local
authentication; a generic rotating code is not transaction authorization.
Verifier MUST check the expected transaction and audience, strict signature,
time interval and current financial grant, then atomically consume the nonce
with the ledger mutation. Expiry without one-time consumption is insufficient.
Live relay is mitigated by counterparty/session binding; physical proximity
and honest screen display are not proven by the signature.

## 4. Issuance and exchange

States: PROPOSED -> AUTHORIZED -> PREPARED -> COMMITTED, or ABORTED before
commitment. PREPARED has no spendable credit. Cash funding requires sender and
origin dar's signed funding receipt, tied to a unique funding ID and exact
amount/currency. Receipt is evidence of their assertion, not independent proof
of banknote delivery. Cash offered before prerequisites are complete needs a
separately recorded custody/return procedure; it cannot vanish on timeout.

Commitment MUST atomically validate all approvals, funding uniqueness/finality,
quote validity, current dar grants, both dars' secured-capacity reservations,
currency arithmetic, journal balance and the receiver's available credit.
Any failure leaves all financial state unchanged. A retry of identical committed
bytes returns the same result; reuse of any transaction/funding ID with changed
terms is rejected and retained as conflict evidence.

Exchange currency and rate are fixed at the exchange. The source is consumed
and the destination credit becomes effectively available in one financial
commit. Cross-currency journal legs use explicit FX clearing/inventory accounts;
do not numerically balance unlike currencies against each other. Every journal
balances separately in each currency. No negative customer balances or hidden
credit extension. Inventory/FX exposure of dars is separately bounded.

Instant availability means after the valid durable financial commit. Iroh
broadcast is not that commit. A quorum/authority profile MUST give one accepted
ordering per spend and collateral reservation, including cross-shard atomicity.
If it cannot establish that result, HOLD. Partition tolerance never authorizes
independent conflicting commitments. No new offline issuance, exchange,
transfer, collateral release or payout in this initial profile.

## 5. Deposits and secured capacity

Independent escrow and federated dar custody MUST both be supported through
the [custody interface](custody-interface.md). Select a configured adapter per
account; do not assume a central custodian or a fixed federation quorum. Both
modes enforce the same economic invariants. Authenticated account/version/epoch
receipts, idempotent commands, external-execution reconciliation and fenced
migration are required as specified there.

Every dar MUST post a verified deposit under a custody agreement before
accepting exposure. Deposit evidence binds asset, amount, currency, dar,
custodian, asset/lot reference, custody mandate, senior liens, withdrawal
restrictions and enforceable realization procedure. Custody attestation alone
does not prove custodian solvency or absence of external liens; reconcile assets
and encumbrances independently. Self-reported deposit balances are insufficient.

Initial arithmetic profile: same-currency eligible collateral, 100% reservation
against each explicitly covered obligation, no leverage and no collateral reuse.
This is a conservative proposed profile, not a user-selected commercial ratio.
Other currencies/assets/haircuts require a separate valuation/FX/liquidation
profile and review; v0.1 does not quietly apply a spot mark as guaranteed value.

Per dar/currency/coverage bucket:
free = eligible_deposit - sum(active_reservations).
Require free >= new_covered_exposure. Pending awards and withdrawal reservations
also encumber capacity. Physical payout liquidity and collateral are distinct;
the same asset cannot be counted simultaneously as free working cash and locked
security. Two dars' deposits MUST NOT be presented as additive recovery unless
their legal priorities and obligations permit it. No duplicate recovery.

Origin exposure persists while its issued liability/reimbursement obligation
is outstanding. Backing exposure persists while its agreed customer-redemption
or backstop duty remains outstanding. Settlement between dars does NOT by
itself extinguish the customer's credit or release all its backing. Payout does
NOT by itself settle inter-dar reimbursement. Release each allocation only on
evidence that its named obligation is discharged or validly replaced.

Custodian withdrawal is limited to free collateral and committed atomically
with reservations. Dispute/default encumbrances cannot be withdrawn. Deposit
shortfalls suspend NEW exposure and trigger remediation under the agreement;
they do not erase existing liabilities or automatically seize everything.

## 6. Transfer and redemption anywhere in the network

Same-currency onward payment consumes holder-authorized inputs and credits the
recipient exactly once. Origination liability/collateral follows the resulting
credit lots. New FX needs a new accepted exchange. No implicit sale of claims
at discount/premium: legal characterization of onward assignments is a launch
gate. This spec gives no unrestricted debt-trading permission.

Network agreement appoints eligible payout agents in advance; the holder chooses
an available agent at redemption. The payout agent confirms local amount,
currency, cash denominations, disclosed fees and reimbursement source. Original
obligors remain responsible unless an explicit legally valid reassignment with
the required parties' consent is recorded. Location alone grants no authority.

Redemption states: AVAILABLE -> RESERVED -> CASH_RELEASE_AUTHORIZED ->
PAID_ACKNOWLEDGED, or DISPUTED. Reserve removes the amount from spendable balance.
Only the selected authenticated payout operator/device receives a one-time cash
release authorization, after online durable consumption. Retries return status,
never an instruction to release cash again. A fresh attempt needs its own
authorization and proof the earlier attempt cannot still execute.

Before cash release, reservation cancellation requires proof that outstanding
release authority is fenced. After cash release is authorized, an acknowledgment
timeout MUST NOT automatically restore spendable funds. Device failure or
uncertain delivery enters DISPUTED with the amount locked for adjudication.
For manual cash the operator and recipient must inspect/count and confirm;
signatures cannot make cash delivery and acknowledgment physically atomic.

The recipient signs received amount/currency only AFTER receipt; readiness is a
different message. Partial payout consumes only the recorded paid amount and
retains the remainder, with explicit amended terms. The initial implementation
may reject partial payouts rather than invent their completion. Disburser and
recipient acknowledgments produce payout evidence. Physical delivery may remain
disputed despite an apparent ledger outcome; no cryptographic proof of banknotes
or honest witnesses is claimed.

## 7. Default, dispute and settlement

An allegation creates a case, not a confiscation. Case records bind claimant,
obligation, outstanding principal, evidence, respondents, notice, response
deadline, adjudicator mandate and appeal/finality rules. Different cases cannot
recover the same loss twice. Issuer/custodian/adjudicator collusion is an explicit
trust risk; segregation of duties and audit are required.

Final award specifies liable dar, claimant, currency, permitted recovery,
collateral allocations and execution ID. Executor verifies mandate and award,
then atomically records the actual recovery and remaining liability. Recovery
cannot exceed the established remaining claim; surplus collateral remains the
dar's property. No interest or automatic time-based debt increment. Permitted
actual enforcement costs require explicit legal/fiqh basis and separate records;
no generic “slash percentage” or late-profit penalty. Appeal/stay and insolvency
priorities must be fixed in the deployment agreement.

Settlement has its own signed evidence and journal. It discharges only the
identified inter-dar obligations. Customer payout, issuance finality and
settlement are independent dimensions, never one misleading completed flag.

## 8. Reputation

Financial reputation is derived from unique accepted obligations and receipt
evidence: funding accepted, credit actually made available, payout acknowledged,
settlement performed, unresolved cases, overdue duties and finalized defaults.
Credit availability counts only when backed by a valid commit receipt, not a
signed promise or listing. Cash payout counts only for the amount acknowledged
received, qualified by any dispute. No user star score drives this profile.

Show denominators and evidence coverage: accepted vs fulfilled obligations,
recent history, size bands, currencies, counterparty concentration, open exposure
and independently established relationships. A new key is not a new person.
Circulating funds and Sybil identities can manufacture signed success. Counts
and volume are not solvency or legal-compliance guarantees. Public displays
require permissioned aggregation/selective disclosure; do not publish customer
receipts or sensitive transfer identifiers to prove reputation.

## 9. Identity succession and recovery without revocation

ContinuityClaim binds predecessor, successor, scope, recovery-policy/evidence
context digest and unique nonce. Successor signs acceptance. Planned succession
also obtains predecessor endorsement. Lost-key recovery instead requires an
independently authenticated prior recovery arrangement or independently verified
re-enrollment evidence. A claimant's new witness list is not a trust anchor.

Witness policy MUST define exact keys, independence assumptions, scope, approval
rule and dispute procedure. Distinct keys alone are not independent witnesses.
Old-key signatures, self-declared dates or an opaque policy digest cannot prove
that a recovery policy existed before theft. With no old key or independent
evidence, continuity remains unestablished; do not invent it.

Competing claims remain visible and continuity-dependent actions HOLD. Neither
first arrival, majority of untrusted keys, longest chain nor latest timestamp
selects a rightful successor. Offline peers can retain inconsistent views;
reconnection reveals conflicts but cannot prove history completeness.

Historical key identity and original signed receipts remain intact. A successor
authenticates using a fresh temporary QR. Financial grants and balances move
only by their separately authorized recovery/assignment procedure. Pending
transfers cannot be redirected from continuity evidence alone. A stolen old key
can still sign: succession does not solve compromise or disable that key globally.

## 10. Message contract and canonical bytes

All signed messages MUST use domain-separated, unambiguous canonical bytes.
Common header: protocol version, network ID, message kind, unique message ID,
signer full key, role, agreement/profile digest and payload digest. Signed
authorizations add audience, transaction/terms digest, nonce, validity interval
and expected ledger sequence/checkpoint. Time uses Unix seconds, expiry exclusive;
issuer history timestamps are assertions unless independently attested.

| Message | Essential payload and signatures |
| --- | --- |
| Listing / Quote | Section 2 fields, dar signature; quote approvals from required parties |
| TransactionAuthorization | Section 3 fields, authenticator signer bound to authorized holder |
| FundingReceipt | Funding ID, quote digest, amount/currency, sender + origin dar |
| CollateralAttestation | Section 5 fields, custodian mandate and signature |
| ReserveCollateral | Obligation ID, dar/currency/amount, expected capacity version |
| CommitCredit | Input/funding references, outputs, journal and reservation digests, authority receipt |
| TransferCredit | Input lots, output holders/amounts, holder authorization, commit receipt |
| ReserveRedemption | Credit inputs, payout agent, terms, holder approval, commitment receipt |
| CashRelease | Reservation ID, operator/device, exact amount, one-time execution ID |
| PayoutReceipt | Release ID, actual amount/currency, recipient + payout dar |
| SettlementReceipt | Named obligations, amount/currency, evidence digest, both dars |
| Dispute / Award / Recovery | Section 7 fields and signatures under the agreed mandate |
| ContinuityClaim / Endorsement | Section 9 fields, successor and relevant endorsers |

Initial wire encoding for NEW financial messages: prefix
`hawala-payment/v1` followed by a NUL byte; fields in the table's schema order,
each prefixed by a u32 big-endian byte length. Integers are fixed-width unsigned
big-endian; currency codes are three uppercase ASCII bytes; IDs/digests are
32 bytes; signatures are Ed25519 64 bytes. Strings must be valid UTF-8 and within
schema bounds. No optional omitted fields: encode an explicit tagged variant.
Lists encode a u32 count followed by individually length-prefixed entries;
sets are sorted by full canonical bytes, reject duplicates. Sign the canonical
body without signatures; SHA-256 digests are domain-separated with message kind.

This defines encoding rules, not a complete field-by-field wire schema for all
messages. Concrete schemas/decoder conformance are a production gate. Existing
identity v1 signed bytes remain unchanged. No JSON formatting is signed.
Incoming frames MUST be bounded before allocation (proposed initial 64 KiB,
at most 64 signatures, label 128 bytes); reject unknown critical versions,
malformed keys, zero IDs/nonces, duplicate roles and trailing bytes. Sensitive
payloads travel over authenticated Iroh sessions to authorized peers only.

## 11. Ledger and recovery invariants

1. No positive spend without a uniquely committed source/funding event.
2. Each input and release authorization is consumed at most once.
3. Same committed request is idempotent; changed bytes with reused ID conflict.
4. Currency-specific journals conserve amounts; no overflow/float arithmetic.
5. Active reservations never exceed eligible deposits; no concurrent withdrawal
   and commitment both consume the same free collateral.
6. Grant withdrawal, identity succession, quote expiry, dar settlement and
   history synchronization cannot delete customer liabilities.
7. Cash uncertainty never automatically creates a second spendable claim.
8. Default recovery never exceeds the established remaining claim.
9. Durability acknowledges only after committed state and replay records persist.
   Restart restores authorizations/locks; stale snapshots cannot roll state back.
10. Evidence merge may discover conflict, but cannot choose financial truth
    by arrival order or execute an external cash action.

## 12. Required deployment decisions and launch gates

The protocol is parameterized; an unfilled profile MUST reject money movement.
Supporting both custody modes is an agreed requirement, not an unfilled choice.
Before launch an agreement must name: (a) exact origin/backing legal liabilities
and customer recourse; (b) collateral custodian, verified assets, segregation,
liens and enforcement; (c) adjudicator, notice, appeals and executor mandates;
(d) ledger authority/validator membership, ordering, partitions, disaster recovery
and authenticated bootstrap; (e) supported currencies, fees, actual redemption
service commitments and liquidity; (f) participant and operational-key enrollment,
loss/recovery policy; (g) applicable regulatory eligibility and data retention.

Hanbali contract review MUST assess the actual funding custody/loan character,
onward assignment, effective constructive possession, spot FX, inter-dar advances
and fees, security/guarantee and enforcement terms. No blanket compliance claim.
Sources and the distinction between precedent and design inference are in
[fiqh-review-basis.md](fiqh-review-basis.md).

## 13. Repository alignment

Main currently implements static identity presentation, credential checks and a
generic signed star-rating evaluator. Temporary transaction QR and receipt-based
financial reputation here are target requirements, not claims about current code.
The generic rating API may remain for other domains but MUST NOT stand in for
this financial profile. PR #3 endpoint proofs and PR #4 bilateral receipts are
useful prerequisites; neither implements custody, deposits or spend finality.

The new dependency-free safety crate checks exact quote arithmetic and a local
collateral reservation book. Its in-memory methods are single-process reference
transitions, not durable/distributed money movement. It has no custody oracle,
financial grant checks, award authority or networking. Production adapters MUST
enforce the complete protocol; passing these tests does not establish solvency.
