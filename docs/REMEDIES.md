# Disputes, awards and remedies (v1)

Status: **reference specification and pure implementation (`hawala-remedy`);
no adjudication service, no execution.** Addresses ADR 0005 gap **G4**.
Applies the rules in ADR 0003, `secured-payment-protocol` §7 and
`CROSS_AUTHORITY_V1` §1 to the cross-authority workflows.

## Principle

A committed payment is never reversed, aborted or rewritten
(`CROSS_AUTHORITY_V1`: no `Committed → Aborted` transition). Every remedy is
a **new**, funded, accepted dual-signed obligation (`SignedTerms`). It is
linked to the original by the original's terms digest, and it runs through
its own manifest and attempt to its own `Settled`. Before commit, the remedy
is the ordinary abort path, not a case.

## Remedies and their basis

| Kind | New obligation | Permitted basis | Cumulative cap |
| --- | --- | --- | --- |
| Refund | Original beneficiary → original sender, in the funding currency | Payee consent, or an award | The original funding amount |
| Replacement | Original sender → original beneficiary, in the payout currency | Sender consent, or an award | The original payout amount |
| Compensation | The award's liable party → its claimant, in the award currency | An award only | The award amount |

Rules:

- **Consent:** a consenting party signs
  `"hawala-remedy-consent/v1\0" kind:U8 original_terms_digest:B32 remedy_terms_digest:B32`.
  Only the party whose money moves may consent: the payee for a refund, the
  sender for a replacement.
- **Awards:** an award counts only when signed by the adjudicator **pinned
  locally** for the workflow, under its pinned mandate digest. It must name
  this original, this kind and this currency.
- **One award per case:** a book honours one award per case. A different
  award for an already-used case (an appeal, or equivocation) is an
  `AwardConflict` that waits for deliberate resolution.
- **No double recovery:** consented and awarded refunds share one cap, and
  every award-based remedy also counts against its award. Each remedy has its
  own transfer ID, distinct from the original's and from every other remedy's.
- **What `RemedyBook` is:** an immutable value. Replaying an admitted remedy
  is a no-op, and the original is never changed.
- **Consistent with §7:** no interest, penalty or time-based increment. An
  allegation is a case, not a confiscation.

## Disputes

A party to the original opens a case by signing
`"hawala-remedy-dispute/v1\0" case_id original_terms_digest claimant reason_digest evidence_digest opened_at:U64`.
The parties are the sender, the beneficiary, the origin dar and the
disbursing dar. Anyone else is `NotAParty`.

Opening a case moves nothing. It places a `Held(lastProven, reason)`
overlay on the business workflow (`CROSS_AUTHORITY_V1` §1) and suspends new
advancing actions. Existing evidence, including any `Settled` receipt, stays
historical evidence.

## Per-workflow profile

The adjudicator and its mandate are named in the workflow's **finality
policy** (`finality_policy_digest` in the manifest), together with the
dispute window, notice, appeal and finality rules that the deployment
agreement fixes (§7). Every row retains the full evidence permanently:

- the original manifest and every evidence envelope;
- the accepted terms and consents;
- the dispute, the award and every remedy;
- source-attributed business evidence digests.

Nothing is erased (ADR 0001).

| Workflow | Who may open a case | Typical remedies | Business effect while the case is open |
| --- | --- | --- | --- |
| Checkout (commerce, restaurant) | Payer, merchant, either dar | Refund (merchant consent or award); compensation by award | Order held: no further release or fulfillment change |
| Dispatch fee | Shipper, carrier, either dar | Refund for non-performance; compensation for damage, by award only | Service release held; delivery evidence retained |
| Group buy | The pledging member, seller, either dar, after release | Refund by seller consent or award | Further releases held. A pre-release threshold failure is the group-buy escrow's own release rule, not a remedy |
| Remittance | Sender, recipient, either dar | Replacement (sender consent or award) for failed or disputed payout; refund by award | Uncertain delivery stays locked for adjudication (§6); no automatic restore |

## Not covered

- The adjudication service, its notice and appeal workflow.
- Encoding the adjudicator into the finality-policy bytes.
- Executing a remedy, which uses the ordinary cross-authority commit.
- Partial payouts and amended terms (§6).
