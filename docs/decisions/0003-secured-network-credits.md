# ADR 0003: Secured network credits and scoped authority

Status: Proposed specification, 2026-10-24. No production authorization or
fiqh certification is implied. Extends ADRs 0001–0002. Normative protocol:
[secured-payment-protocol.md](../secured-payment-protocol.md).

## Agreed requirements

- Hawala uses Iroh P2P. The app records currency-denominated credits; it is
  not itself a legal person or an unexplained debtor called “the network.”
- Customers present identity only through temporary QR bound to an
  authenticator. Names/codes are visual companions, never alternate login IDs.
- Sender and sending dar participate together at funding. Receiver and payout
  dar participate together at cash collection. These are separate encounters.
- Receiving currency, exact exchange rate, net amount and fees are accepted
  at the exchange. A completed exchange creates credit in the receiving currency.
- Every dar provides a deposit. Accepted obligations consume secured capacity.
- Support independent escrow and shared dar custody through the same
  [custody interface](../custody-interface.md); choose the mode per account.
- Reputation derives from verifiable receipt of funds, not arbitrary stars.
- Persistent identities and signed historical evidence are not revoked.

## Evolution of the two-dar arrangement

The earlier model fixed both dars before funding. The requested payment-service
extension permits later redemption anywhere in the participating network.
For this proposed profile, the origin dar and a named backing dar commit at
issuance; their liability remains recorded. The customer authorizes selection
of a later payout agent under the accepted network agreement. A payout agent
does not silently replace either original obligor. Network-wide means agents
with an effective service agreement and confirmed local availability, not an
unconditional promise of cash at every place, time and denomination.

This is a proposed reconciliation of those requirements, not a claim that the
user has already selected the legal allocation of joint/several liability.
The liability agreement must name each duty, creditor, recourse and discharge
condition. No issuance is allowed with that agreement missing.

## Who can revoke what, and why?

| Authority | Permitted effect | Prohibited inference |
| --- | --- | --- |
| Nobody | None over existence of another persistent identity or old signatures | No global identity cancellation or erasure |
| Grantor / expressly delegated administrator | Withdraw its own future operational grants under their terms | Cannot cancel issued customer claims or another grantor's permissions |
| Relying peer | Change its own issuer trust/admission policy | Cannot bind all peers or erase adverse evidence |
| Customer | Approve exact spend terms; cancel an uncommitted proposal | Cannot reverse a completed spend unilaterally |
| Collateral custodian | Reserve/release assets under an identified mandate | Cannot invent deposits or treat every allegation as default |
| Agreed adjudicator / authorized recovery executor | Award and execute established claims within mandate | No punitive confiscation, duplicate compensation or identity cancellation |
| Recovery witnesses | Sign continuity evidence | Cannot redirect balances, appoint themselves, or inherit financial authority |

Suspending new business preserves existing debts, redemption claims and access
to dispute remedies. Any restriction on existing claims requires a distinct,
identified authority and basis; it is not a generic “revoke identity” API.

## Layering

Identity-core: public-key identity, signatures, possession evidence, continuity
claims and provenance. No money, collateral eligibility or universal reputation.

Hawala: quotes, grants, balances, double-entry journals, liabilities, secured
capacity, payout authorization, settlement and dispute decisions.

Discovery: approximate-location/radius queries, signed expiring listings,
currency/corridor availability, net rates/fees and source-backed reputation.
Listing presence neither reserves liquidity nor authorizes a transfer.

Iroh: authenticated transport and evidence exchange. It does not establish
ledger finality, a complete history, funds possession or financial permission.

## Trust and implementation limits

The custody-mode decision is settled: both are supported. Actual providers,
federation controllers/approval rules and enforcement mandates remain deployment
configuration. Neither mode can bypass reservations, historical liabilities or
default adjudication. Custody-controller succession is distinct from participant
identity succession and requires asset-level fencing.

External cash acceptance, collateral custody, witness honesty and award
enforcement remain explicit trust assumptions. Colluding parties can sign false
receipts. A compromised key continues producing valid signatures. Disconnected
peers cannot infer current balances or absence of double spending.

This PR specifies required interfaces and provides a small deterministic safety
kernel. It does not implement a production ledger, consensus, authenticator,
custodian, recovery service or physical cash delivery. PRs #3/#4 independently
cover endpoint binding and signed bilateral evidence; their receipts must not
be mistaken for the finality required here.
