# hawala-core

A protocol-first, Iroh-native peer-to-peer **hawala** network. Hawala obligations,
bilateral trust, fulfillment, clearing and settlement are the system's core.
QR is the only participant identity-presentation interface; ATMs, merchants,
gift cards and app credits are downstream adapters or applications.

## Current state

**Foundation only.** This repository currently implements a first reusable
Rust `hawala-identity-core` crate, not Iroh networking or money movement.
Do **not** represent the current code as a deployed financial network.

The identity crate includes:
- Persistent Ed25519 public-key identities with `hawala:id:v1:<hex>` QR payloads.
- Challenge-response proof of possession with mandatory injected replay protection.
- Issuer-, attribute- and scope-specific signed verification credentials.
- Explicit revocation freshness checks and local issuer policy.
- Mutually signed interaction evidence and domain-specific verified ratings.

Its identity/reputation logic is intentionally independent of Hawala's future
financial ledger and of biz-kit. Neither ratings nor credentials themselves
grant authority to move customer funds.

See [architecture and limitations](docs/architecture.md).

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
