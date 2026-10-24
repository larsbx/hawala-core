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
ATMs, merchants, gift cards and app credits are downstream adapters or
applications.

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

Its identity/reputation logic is intentionally independent of Hawala's future
financial ledger and of biz-kit. Neither ratings nor credentials themselves
grant authority to move customer funds.

See [durable replay protection](docs/durable-replay.md),
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
