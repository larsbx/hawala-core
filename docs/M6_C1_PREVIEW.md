# M6-C1 — pure authority preflight: not a financial commit

This milestone implements the immutable `AuthorityEnvelope` and
`admit_preview` function. Its return types are `Rejected`, `Held`, and
`Prepared(PreviewPlan)`. **Prepared is not Committed.** There is no
wallet, no custody adapter, no actuator permit, no durable state change.

The functional checker composes the existing cryptographically signed
bilateral Hawala obligation, directional human/machine interaction, temporary
QR approval, independently pinned eligibility/custody issuer signatures, and
operator delegation. It requires matching FX quote arithmetic and preview
double-entry postings per currency, but NEVER persists these.

## Explicit constraints

- The `ExternalReviews` fields `ReportedClear`, `Unknown`, `Denied`
  are **untrusted inputs** from external authenticated controllers, not
  proven facts. Any caller can forge a `ReportedClear` in this crate.
  An independent authoritative commit MUST validate original evidence,
  grant status, account state, licensed operator rights and custody control.
- Quote is a rate/fee arithmetic profile with a nonzero digest; the digest
  is not verified as a signed executable quote. Recipient acceptance is
  currently an external report, not a validated authorizing signature.
- `Versions` refers to reports about the expected epoch/custody version;
  without the actual custodian's checkpoint and a serialized ledger they
  cannot prevent stale reservations or conflicting spending.
- `Book::post` is called on a CLONE. It proves only an in-memory reference
  model balances exact integer postings, not actual assets.
- The `PreviewPlan` digest binds exact terms, QR approval, attempt, unique
  ledger event ID, account sides/currencies/postings, quote digest and
  reported state versions. It is a reference plan, not an executable token.

## Exit conditions for M6-C2

The next phase must implement one durable authority committing customer claim
reservation, actual custodian-capacity evidence, ledger postings, QR nonce
consumption, unique attempt fencing and epoch/version sequencing in ONE
serializable transaction. For escrow and federated custody, the legal
mandates, signed receipts, quorum/fault assumptions and actual asset
reconciliation must be supplied independently. Crash, equivocation,
partition, false issuer and stale checkpoint tests are mandatory.

No human or ATM may receive cash execution permission from this crate;
all real funds are disabled.
