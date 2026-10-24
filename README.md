> Proposed secured-payment extension: see [the protocol](docs/secured-payment-protocol.md),
> [ADR 0003](docs/decisions/0003-secured-network-credits.md), and
> [security conformance](docs/security-conformance.md). Temporary transaction QR,
> receipt-based financial reputation and deposits are specified requirements;
> current main's static identity QR and generic star ratings do not implement them.
> The reference safety kernel is not a production ledger or custody service.

# hawala-core

A protocol-first, Iroh-native peer-to-peer **hawala** network. Hawala obligations,
bilateral trust, fulfillment, clearing and settlement are the system's core.
Every QR identity presentation must include a human-readable name or identity
code. The public QR key is the cryptographic identifier; the adjacent label
helps people verify they are interacting with the intended participant.
ATMs, merchants, gift cards and app credits are downstream adapters or
applications.

## Current state

**Foundation only.** This repository currently implements a first reusable
Rust `hawala-identity-core` crate, not Iroh networking or money movement.
Do **not** represent the current code as a deployed financial network.

The identity crate includes:
- Persistent Ed25519 public-key identities with `hawala:id:v1:<hex>` QR payloads.
- QR identity cards with an always-visible public identity code and optional
  display name; scanned apps recompute the code from the QR key.
- Challenge-response proof of possession with mandatory injected replay protection.
- Issuer-, attribute- and scope-specific signed verification evidence with bounded validity.
- Local issuer trust policy; no revocation registry or centralized identity disablement.
- Mutually signed interaction evidence and domain-specific verified ratings.

Its identity/reputation logic is intentionally independent of Hawala's future
financial ledger and of biz-kit. Neither ratings nor credentials themselves
grant authority to move customer funds.

See [architecture and limitations](docs/architecture.md) and
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

