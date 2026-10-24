# M6-A — signed cash-out admission evidence (reference gate)

**Status: experimental verification-only implementation.**

The `hawala-admission` crate checks *signed statements* about the
preconditions of a cash-out against one exact bilateral Hawala obligation,
a mutually signed human/machine interaction, and one temporary QR
authorization. It does not create an operator, transfer money, control
cash cassettes, reserve genuine assets, or verify regulatory licensing.

## Evidence profiles

The verifier receives an **independently anchored**, local
`Authorities` configuration containing:

- A specific eligibility/policy issuer key (not the beneficiary or dar).
- A separate custody issuer key (also not the beneficiary/dar).
- A pinned policy/custody epoch and minimum account-state version.
- The exact separately agreed amount of fee in payout currency minor units.

The two issuers must be distinct. The eligibility issuer signs a scoped
eligibility statement; the custodian signs an exact-amount custody-hold
assertion. BOTH statements cover the same QR/attempt/obligation context.
Each includes an external evidence digest, an authority state version, and
a maximum 300-second signed interval. Epoch, evidence and validity are
not inferred from unauthenticated Iroh messages.

A **machine** acting for a beneficiary or disburser also requires a
separate domain-specific signed delegation issued by THAT principal.
This is required for either or both sides of machine↔machine operation.
A human directly using their persistent identity key must NOT smuggle in
a device delegation. Neither ratings nor identity-badge status grant a
financial permission.

All signed statements commit to:
- the full transfer and accepted terms digest;
- exact payout attempt and temporary-QR signature digest;
- beneficiary, disbursing dar, actual holder and cash interface keys;
- payout amount, currency, expected fee and authority epoch;
- issuer, statement kind, attestation version and evidence digest;
- issuance and expiry timestamps.

A change to any signed field invalidates its signature. Exact terms,
cash participant and orientation come from `hawala-flow` and verified
`SignedAuthorization`. The trusted authority policy is supplied locally;
the gate does not discover it by network gossip.

## Verification contract

`evaluate(...)` refuses missing machine delegation, wrong issuer or kind,
altered signatures, stale credentials, mismatched custody state version,
policy-epoch changes, wrong fee, swapped counterparties and changed QR
challenge/session. All human↔human, human↔machine, machine↔human and
machine↔machine orientations use one implementation.

Success returns an `EvaluatedEvidence` reporting verified document
identities and version numbers, **not a payment authorization**.

## Why this does not permit real cash yet

Even an honestly signed custodian statement is not proof that a deposit
actually exists, that the custodian controls unencumbered assets, or
that a payment authority accepted the liability. A dishonest or
compromised issuer can sign a false statement. Current membership,
mandates, legal authorization and eligibility are application and
jurisdiction-specific, and cannot be established just by a public key.

Additionally, the reference gate has NO single atomic ledger transaction
joining currency balances, liability lots, collateral reservations,
temporary QR nonce, epoch/fencing checks and a unique cash attempt.
The existing SQLite journal records only local intent. Therefore:

1. A locally passing gate MUST NOT directly drive an ATM.
2. A proof CANNOT replace bank/custody account reconciliation.
3. No 'settled' status is inferred from device telemetry or human receipts.
4. Identity remains append-only. Financial grants are time-bounded and
   rechecked, not a universal identity revocation mechanism.
5. A signed fee match is NOT the full executable FX quote check.

## Next safety milestone

Implement an independently reviewable reference *accounting* authority
that atomically checks the signed evidence, confirmed custody allocations,
lot provenance, bilateral secured capacity, current grants and all ledger
entries in a single durable commit. It must block during network partitions
unless its pinned fault model can prove one accepted order. **Only after
this works** should hardware adapters or real funds be considered.

Acceptance tests: all oriented peer combinations; forgeries, insufficient
custody evidence, same-issuer collusion, stale versions, wrong delegation,
non-fresh grants, replays and crash/epoch fences. Before production require
external code/security review and legal/regulatory assessment.
