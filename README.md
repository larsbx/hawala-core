> Proposed secured-payment extension: see [the protocol](docs/secured-payment-protocol.md),
> [ADR 0003](docs/decisions/0003-secured-network-credits.md), and
> [security conformance](docs/security-conformance.md). Temporary transaction QR,
> receipt-based financial reputation and deposits are specified requirements;
> current main's static identity QR and generic star ratings do not implement them.
> The reference safety kernel is not a production ledger or custody service.

Deposit custody supports [independent escrow and federated dar custody through
one interface](docs/custody-interface.md). Both preserve the same collateral,
liability and default-recovery rules; live providers remain unconfigured.

> Cross-repository alignment (proposed): [ADR 0006](docs/decisions/0006-biz-kit-extension-host-boundary.md)
> defines Hawala's canonical financial ownership and its biz-kit extension-host
> boundary. ADR 0004 D1–D6 remain in force; the N1–N7 native-integration
> proposal in PR #42 is not yet accepted.
>
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

The `hawala-controller-fence` crate provides local, crash-durable controller
anti-equivocation for escrow and federated approval simulations. Each
controller can sign one distinct custody command for one pinned
policy/account/epoch/version checkpoint; restart and concurrent signing
cannot create a second differing signature from the same fenced key.
This does NOT establish real assets or distributed consensus.
See [controller fencing](docs/M6_C5_CONTROLLER_FENCE.md).

The `hawala-custody-model` crate simulates signed independent escrow
or federated-controller request approvals, including conservative exact
quorum-intersection guards for 4- and 8-controller examples. Neither
signature threshold nor single custodian attestation proves asset control.
See [M6-C3 custody profiles](docs/M6_C3_CUSTODY_MODEL.md).

The `hawala-commit-sim` crate demonstrates one synthetic, crash-durable
SQLite transaction for moving a test customer liability into a reserved
liability while atomically tombstoning a QR nonce, transfer and attempt.
It has no real money, custody or cash execution API.
See [M6-C2 synthetic atomicity](docs/M6_C2_ATOMIC_SIM.md).

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
The TEST optical-media capture adapter accepts bounded uncompressed Gray8, RGB888 and YUYV422 camera-like buffers with explicit row strides. This is not a camera driver or cash authority. See [capture validation](docs/ATM_CAMERA_CAPTURE_TEST.md) and the
[bounded camera-acquisition packet](docs/ATM_CAMERA_IPC_TEST.md).

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
and [QR companion labels](docs/decisions/0002-qr-companion-labels.md)
and [biz-kit shared identity and scoped authority](docs/decisions/0004-biz-kit-shared-identity-scoped-authority.md)
and [network-native Hawala across federated biz-kit instances](docs/decisions/0005-biz-kit-federated-hawala-integration.md)
and the proposed [cross-authority evidence/commit proof profile](docs/CROSS_AUTHORITY_V1.md)
and [persistent TEST-host conformance](docs/CROSS_AUTHORITY_HOST_CONFORMANCE.md).

The `hawala-capability-port` crate is the reference admission logic for the
`HawalaCapabilityV1` local port between a host platform and its Hawala service.
Calls are host-signed, pinned, versioned, grant-bound and replay-fenced, and
responses are service-signed. Participant consent is re-verified by the service
and refused when any host key signed it. It grants no financial authority. See
[the capability port](docs/CAPABILITY_PORT.md).

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

The test-only integrated `Simulator::admit` revalidates signed evidence within
the reservation transaction and fences provenance/custody checkpoints.
See [durable integrated admission](docs/M6_C_DURABLE_ADMIT.md) for its
non-executable outcomes, adversarial tests, and remaining production gates.

[M6-C4 signed peer recovery](docs/M6_C4_SIGNED_RECOVERY.md) adds authenticated
Iroh observation exercises for every human/machine pairing. Signed reports
remain held evidence through restart; they never prove cash delivery or
permit retry, release or settlement.

The [test-only durable outbox](docs/M6_C4_TEST_OUTBOX.md) queues one synthetic
dispatch with admission and marks it uncertain before claim returns. Claimed
items cannot be retried after timeout or crash; no real executor is included.

The [Lean/Quint verification layer](formal/README.md) proves abstract lifecycle
safety and exhaustively checks a finite instance with a duplicate-claim negative
control. Its CI gates precede further financial state-machine changes; the
model-to-Rust correspondence and remaining refinement obligations are explicit.

The proposed [node-owned books and distributed TigerBeetle profile](docs/NODE_BOOKS_TIGERBEETLE_SPEC.md)
specifies a canonical signed history and PostgreSQL financial commit per
installation, with a deterministically compiled and verified TigerBeetle
accounting copy. Replication stays within each authority domain; independent
authorities coordinate through Hawala. This is an implementation specification,
not an implemented adapter or a change to the existing single-commit gates.

The [bilateral coordination and resolution contract](docs/M6_C6_COORDINATION.md)
adds an isolated durable test journal for two synthetic reservations and one
permanent commit-or-abort decision. Lean/Quint proofs and generated Rust traces
check its abort fence. It is not integrated custody/financial atomicity and
cannot release collateral or resolve an actual payout.

The [coordinated admission test profile](docs/M6_C7_COORDINATED_ADMISSION.md)
binds both synthetic custody claims and the existing admission commit in one
SQLite transaction. Final abort fences delayed admission, and coordinated mode
blocks the older journal/admission APIs from bypassing that decision. This local
test integration still provides no real custody, payout or deployment authority.

The [synthetic terminal execution authority](docs/M6_C8_TEST_TERMINAL_AUTHORITY.md)
provides one durable mock effect or a permanent no-effect fence for a scoped
committed attempt, with independently verifiable test signatures. Terminal
evidence remains held, including conflicts; it cannot release balances or
collateral or authorize a real payout.

## Air-gapped optical TEST boundary

The [air-gapped optical evidence lab](docs/ATM_OPTICAL_TEST_PROTOCOL.md)
uses distinct signed TEST envelopes and bounded multiframe QR text, with security
cameras treated as untrusted receivers. Existing `hawala:auth:v1:` references
still require a live authenticated Iroh session. The lab requires prior synthetic
M6-C7 commitment for export, permanently consumes holder responses as held
evidence, and cannot authorize hardware, cash delivery, retry or financial release.

