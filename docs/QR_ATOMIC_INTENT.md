# Atomic local QR approval + payout intent (M6 prototype)

## Scope and exact claim

This milestone changes the *local* payout-intent journal so that a
verified temporary QR authorization nonce and the unique intended dispense
attempt are recorded **in one SQLite transaction**. This is a meaningful
replay/TOCTOU improvement over consuming an identity nonce and calling
`begin_attempt` separately, but **does not** atomically debit customer
balances, reserve collateral/custody or execute an ATM command.

**Absolutely no real-money execution is enabled by this implementation.**

## Sequence

1. The customer, agent and/or device identify each other by QR, with
   adjacent name/code and a mutually authenticated Iroh session.
2. Two responsible hawaladars have already co-signed accepted terms.
3. The two interacting peers co-sign an `AgreedInteraction` referencing
   precisely those terms; the person or delegated machine representing the
   beneficiary signs a separate temporary-QR `SignedAuthorization`.
4. `verify_signed` checks full Ed25519 signature, exact sender/beneficiary
   and cash counterpart, terms digest, amount/currency, attempt ID, nonce,
   verifier, short-lived QR, challenge and authenticated session binding.
   It returns typed `VerifiedAuthorization` evidence, **not** an authority
   token for any hardware.
5. `Journal::begin_qr_cashout_intent` validates this structural evidence,
   then in a SQLite IMMEDIATE transaction:
   - permanently inserts a unique nonce by holder+verifier, referencing
     transfer ID, attempt ID and signed evidence digest;
   - transitions exactly one registered obligation to `dispensing` bound
     to that attempt; if the cash peer is a machine, pins its full key;
   - appends one local evidence event, all in the same database commit.
6. On duplicate, changed terms, closed state or any DB failure, the whole
   transaction rolls back and NO partial new authorization/attempt is applied.
7. On process restart the existing journal transitions unacknowledged
   dispensing to `uncertain`; it will never repeat the cash attempt.
   Independently signed human or machine fulfillment evidence can be
   recorded, but neither unfreezes a payout or settles liabilities.

## Safety boundaries

- Public `begin_attempt` and `begin_atm_attempt` remain experimental
  low-level interfaces. They do NOT implement active financial permission.
- All involved signing/terminal/operator keys must be legitimately provisioned
  and authorized. Machine self-declarations do not prove operator delegation.
- The peer must independently check fees, FX quote version, current
  KYC/AML/sanctions eligibility, operating licenses, dar security deposits,
  trust capacity, liquidity, collateral custody and consumer consent.
- The temporary QR nonce must be randomly generated and presented via
  an authenticator; a QR is not a bearer claim and may be photographed.
- An operator must prove that the session digest is bound to the live Iroh
  transport. The core merely compares the supplied value.
- The single SQLite authority MUST be properly fenced and durable; this
  does not solve distributed asset double spending, multiple identity devices,
  independently stored ledgers, revocation of an identity, or bank settlement.
- The local clock is checked before opening the SQLite transaction, not
  guaranteed against clock manipulation or expiry during a long blocked DB
  operation. Production authorization needs authoritative commit-time checks.

## Verification

Tests include human/human and human/ATM cash-out, wrong verifier/QR/session
checks, duplicate nonce, changed attempt, restart to uncertain and preservation
of the pinned ATM identity for device outcome evidence. No physical cash
devices or customer balances are involved.

## Next required milestone

Bring the authorization and attempt transition into the **same legally and
economically authoritative ledger/custody commit**, with agreed fault model
and reconciliation. This must be independently reviewed before financial
operations or external dispenser control are enabled.
