# Signed bilateral Hawala obligations — milestone 3

## Scope

This crate models **signed evidence** for one Hawala transfer. It is NOT a
wallet, bank, regulated money transmitter, durable financial ledger, or cash
disbursement control system. All transfer IDs and receipt IDs must be minted
uniquely with a secure random generator by a higher-level application.

## State model

A proposed remittance becomes an **accepted obligation** only after the
origin and disbursing hawaladars sign identical terms. The terms include the
sender, beneficiary, both agents, funding amount/currency, payout
amount/currency, exact acceptance time and quote expiry.

Two independent subsequent attestations:

1. **Payout attested**: the disburser AND beneficiary sign a receipt for the
   full quoted payout. Partial payouts and hardware proofs are not supported.
2. **Settlement attested**: the two hawaladars sign a separate acknowledgment
   referencing an external settlement-evidence digest.

Neither state implies the other, and the order can vary. These are
**attestations**, not proofs that physical cash changed hands or that banking
settlement is final.

`ObligationEvidence` holds one signed accepted terms record and at most
one payout and one settlement record. Replaying an identical signed receipt
has no effect. A conflicting claim is rejected rather than silently
overwriting evidence. Two offline peer snapshots can merge deterministically
when their accepted terms and evidence do not conflict; failed merges leave
the destination snapshot unchanged.

## Security and accounting limitations

- Signatures are Ed25519, over domain-separated, length-prefixed, canonical
  binary statements. A BLAKE3 hash of the accepted terms binds later receipts.
- The issuance and acceptance timestamps are asserted by signers; there is
  **no trusted clock or external timestamping**.
- A dual-signed payout receipt can be fabricated by two colluding actors.
- Applying receipts is in-memory. No crash-safe, atomic transfer-ID
  uniqueness, payout reservation, hardware proof, or dispute recovery exists.
- A conflict after disconnected peers perform disbursement does NOT prevent
  a double payout; it only detects a record conflict afterward.
- Evidence does not check liquidity, KYC/AML, sanctions, licensing, trust
  credit ceilings, jurisdiction rules, or externally verified settlement.
- Cross-currency terms are explicit but FX quotes, spreads, funding sources,
  fee disclosure, and exchange permissions are not implemented.
- For production, real settlement and payout execution must be delegated to
  a separate durable, regulated, authorized ledger and cash/ATM adapter.

## Future gates

1. Iroh-authenticated, framed, mutually verified peer messages and versioned
   transport encodings for Terms and receipts.
2. Atomic persistent obligation journal and idempotent execution authority.
3. Bilateral trust/exposure and liquidity reservation.
4. Legal and operational controls for payout, settlement, reversals and disputes.
5. Two-peer offline/online replay, equivocation, and crash recovery tests.

Never treat a `SignedPayout` alone as permission to release cash.
