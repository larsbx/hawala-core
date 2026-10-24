> Proposed secured-payment extension: see [the protocol](docs/secured-payment-protocol.md),
> [ADR 0003](docs/decisions/0003-secured-network-credits.md), and
> [security conformance](docs/security-conformance.md). Temporary transaction QR,
> receipt-based financial reputation and deposits are specified requirements;
> current main's static identity QR and generic star ratings do not implement them.
> The reference safety kernel is not a production ledger or custody service.

Deposit custody supports [independent escrow and federated dar custody through
one interface](docs/custody-interface.md). Both preserve the same collateral,
liability and default-recovery rules; live providers remain unconfigured.

# hawala-core

A protocol-first, Iroh-native peer-to-peer **hawala** network. Hawala obligations,
bilateral trust, fulfillment, clearing and settlement are the system's core.
Every QR identity presentation must include a human-readable name or identity
code. The public QR key is the cryptographic identifier; the adjacent label
helps people verify they are interacting with the intended participant.
**Human and machine participants are equal first-class Hawala peers.**
All three interaction families are supported by the shared protocol:
**human ↔ machine, machine ↔ machine, human ↔ human**. ATM and cash-recycler
integration is the first **hardware adapter priority**, not a restriction on
who can exchange Hawala obligations. Individual hawaladars and authorized
businesses remain human-operated access points; machines may execute only
actions explicitly delegated by independently verified policy.

## Current state

**Experimental protocol foundation only.** This repository implements reusable
QR identity primitives, a mutual Iroh peer-identity handshake over live QUIC
streams, and signed bilateral Hawala obligation evidence. It is NOT a
money-movement service or a deployable financial network.
Do **not** represent the current code as a deployed financial network.

The identity crate includes:
- Persistent Ed25519 public-key identities with `hawala:id:v1:<hex>` QR payloads.
- QR identity cards with an always-visible public identity code and optional
  display name; scanned apps recompute the code from the QR key.
- Challenge-response proof of possession with mandatory injected replay protection.
- Issuer-, attribute- and scope-specific signed verification evidence with bounded validity.
- Local issuer trust policy; no revocation registry or centralized identity disablement.
- Mutually signed interaction evidence and domain-specific verified ratings.
- A four-message mutual, CSPRNG-challenged, session-scoped handshake over
  authenticated Iroh 1.4.1 QUIC; includes a real local two-peer integration test.
- Optional SQLite replay guard, durable identity nonce consumption across
  restarts and bounded per-frame handshake deadlines.
  Identity proof never confers financial rights.

The new `hawala-protocol` crate models dual-signed accepted obligations,
independent payout/settlement attestations, and deterministic evidence
reconciliation across temporarily disconnected peers. This is **not** an
execution ledger: signed attestations are not evidence of physical cash
delivery or legally final settlement.

The experimental `hawala-journal` crate records dual-signed transfer
intents, one-way dispense-attempt transitions, and crash recovery to
`uncertain`. It does not execute cash transfers or reconcile real hardware.

The `hawala-interaction` crate models directional human/machine combinations
and dual-signed interaction context bound to a Hawala obligation digest.
The `hawala-flow` crate additionally verifies each side represents the
correct economic principal of the **exact** dual-signed accepted Hawala
terms, independently of whether the presenter is a human or machine.
The `hawala-human` crate supports signed beneficiary↔disbursing-hawaladar
handoff assertions bound to a single durable cash attempt.
The `hawala-qr-auth` crate adds temporary, verifier/session/attempt-bound
QR authorization evidence for every human/machine pairing.
The journal can store nonce consumption and the one permitted *intent*
in one SQLite transaction, but **no customer balance/custody reservation,
real cash command or financial authorization** is implemented.

The `hawala-authority` crate adds a pure `admit_preview` for the accepted
Hawala obligation, signed QR consent, eligibility, signed custody claims,
device delegation, FX arithmetic and balanced draft postings. It returns
no executable financial permission and changes no account or custody state.
See [M6-C1 reference admission](docs/M6_C1_PREVIEW.md).

The `hawala-ledger` crate provides a finite, exact reference double-entry
invariant checker: balanced postings PER currency, nonnegative asset and
liability accounts, deterministic idempotency and rejected tampering.
This is in-memory only, with no customer funds or custody integration.
See [reference accounting](docs/REFERENCE_LEDGER.md).

The experimental `hawala-admission` crate checks independently signed,
transfer-bound eligibility and custody-hold *claims*, and human/machine
operator-delegation signatures where needed. It does not prove that assets
are held or that an issuer is legally licensed, and it does not authorize
a real transaction. See [admission evidence gate](docs/ADMISSION_GATE.md) and
[the next single-authority financial commit contract](docs/M6_C_AUTHORITY.md). The journal
retains those reports without inferring cash delivery or releasing a retry. Its
signatures attest only to an agreed proposal, not to a regulated permission
or a successful cash transfer. See [universal peer modes](docs/PEER_MODES.md) and
[common flow binding](docs/COMMON_FLOW.md) and
[temporary QR authorization](docs/TEMPORARY_QR_AUTH.md) and
[atomic local QR intent](docs/QR_ATOMIC_INTENT.md).

The **ATM-first hardware** prototype defines signed, bounded terminal observations,
trusted terminal/attempt binding in the durable journal, and conservative
tri-state cash-out reporting. Such reports never unfreeze an uncertain payout
or authorize a duplicate attempt. No real ATM hardware or custody is connected.
See [ATM-first architecture](docs/ATM_FIRST.md) and the
[ATM provider event simulator](docs/ATM_PROVIDER_SIMULATOR.md).

Its identity/reputation logic is intentionally independent of Hawala's future
financial ledger and of biz-kit. Neither ratings nor credentials themselves
grant authority to move customer funds.

See [crash-safe payout intent journal](docs/durable-payout-journal.md),
[durable replay protection](docs/durable-replay.md),
[bilateral obligation evidence](docs/bilateral-obligations.md),
[Iroh identity binding](docs/iroh-identity-binding.md),
[on-wire handshake](docs/iroh-wire-handshake.md) and
[architecture and limitations](docs/architecture.md) and
[identity without revocation](docs/decisions/0001-no-identity-revocations.md)
and [QR companion labels](docs/decisions/0002-qr-companion-labels.md).

## Local development

```sh
cargo test --workspace --all-targets
```

## Architectural direction

```text
Iroh encrypted P2P transport / discovery
              |
QR-presented cryptographic identity  <- reusable identity-core
              |
Explicit bilateral trust + obligations
              |
Payout evidence / clearing / settlement
              |
Merchant, individual and ATM adapters
              |
Credits, gift cards, remittance UX
```

There is **no global balance database or centralized transaction server**
required by this architecture. Iroh transport does not provide financial
consensus, legal compliance, or at-most-once disbursement.
