# Air-gapped ATM optical evidence lab — version 1

Status: **TEST ONLY**. No cash equipment, bank/custody provider, production keys,
spendable value, hardware authorization or deployment is enabled. This lab moves
signed evidence across a genuine network break using display QR → camera QR
text. Its only acceptance result is held evidence. It does not claim offline
financial finality, physical proximity, real asset control or honest sensors.

## Compatibility and authority

`hawala:id:v1:` remains the sole presented participant identifier: independently
scan the full public key and display its name/code on the trusted authenticator.
No username, telephone number, card or manually entered account ID is introduced.
The long-lived Hawala identity is distinct from an Iroh endpoint key. Optical
proofs use the former; they never fabricate an Iroh `VerifiedPeer`, QUIC channel
binding, endpoint authentication or an authenticated online session.

`hawala:auth:v1:` **retains its live mutually authenticated Iroh resolution
requirement**. It cannot be resolved at an air-gapped terminal. Optical TEST
messages use a disjoint prefix, magic and signature domain; copying a live
reference into this protocol fails. Online preparation and later reconciliation
remain peer-to-peer over Iroh. No central server is introduced. The precommitment
binds the bilateral terms and both responsible hawaladars; reusable identity and
reputation remain separate from financial grants.

Optical envelopes are plaintext: observers can see the public identity keys and
transaction scope. No confidentiality is claimed; no private keys travel in QR.

A security camera is an **untrusted optical receiver**: its video decoder returns
QR text only. It owns no signing key, policy, trusted clock, identity decisions,
financial journal or hardware command credential. The terminal's trusted local
controller validates decoded bytes. The display, signer and clock must be
independent of the camera. A future genuinely air-gapped deployment would need
local isolated camera acquisition, no IP/cloud camera control or network bridge,
and independently reviewed physical provisioning. No camera/video driver or QR
image renderer is implemented here. `transmit` and `CameraReceiver` are deterministic
QR-text adapters suitable for recorded/reordered/fragmented simulated frames.

## Prior online commitment and offline restrictions

Before disconnecting, the responsible peers would need independently verified
funding finality, collateral/asset control, both bilateral reservations, current
eligibility/delegation and policy, an exact accepted FX/fee quote, unique transfer
and attempt fencing, terminal-specific allocation, and an authoritative common
commit order. M6-C7 implements only the **synthetic local** version of that order;
it is not proof of actual distributed custody commitment.

`Simulator::export_test_optical` checks the complete immutable M6-C7 binding and
committed decision, then uses the existing `claim_test_dispatch` transaction.
This permanently marks the existing admission/outbox uncertain before returning
one signed TEST precommitment. It does not add a second dispatch rail. Preparation,
confirmation flags, final abort, legacy admissions, changed quote/terms/custody
requests, stale epochs and previously claimed outboxes cannot export it. The
scope is derived from the admitted terms and QR evidence, not camera input. The
export lifetime is bounded by the existing authorization and persisted outbox deadline. Accepted quote expiry is the offer
acceptance deadline, not a new payout lifetime; its immutable acceptance is
already checked by admission. Existing
outbox freshness checks still apply. A signing/transport crash after the claim
loses availability: it cannot mint a replacement. There is no re-export API.

The caller's TEST signing authority is independently pinned at terminal enrollment.
The standalone `Envelope::precommit` constructor is a fixture issuer, not proof
that M6-C7 ran: an honest locally configured issuer is an explicit trust assumption.
Do not accept a public key learned from a QR envelope as that authority.

| Operation | Offline behavior in this implementation |
| --- | --- |
| Display/scan identity; prove key possession | Supported as scoped TEST evidence |
| Import a prior terminal-specific TEST commitment | Supported after pinned signature/policy checks |
| Authenticate exact committed holder consent once | Durable `consumed_held`, no execution ticket |
| Receive terminal-signed conflicting claims | Preserve complete envelopes; keep held |
| Check current external custody, sanctions, delegation or policy updates | Unavailable; no online freshness is invented |
| Create new credit, deposit value, change quote/route/beneficiary/terminal | Unavailable |
| Spend an uncommitted balance or share offline allocation across terminals | Unavailable |
| Dispense cash, authorize hardware, retry/redirect withdrawal, release/refund collateral | Unavailable, including after expiry or reported no delivery |
| Settle bilateral obligations or declare final delivery | Unavailable; requires independent online reconciliation |

A future physical executor needs its **own independently provisioned hardware
permission and permanent execution fence**, separate from optical authentication.
Even a first `AuthenticatedHeld` result is not accepted by a hardware API. The
current ATM observation/journal behavior remains unchanged: partial delivery,
missing sensors, retracts, silence and contradictions remain uncertain. Cameras
must never be promoted to items-taken sensors, custody evidence or delivery proof.
The M6-C8 synthetic execution rail remains independent: optical acceptance
never returns its dispatch ticket. Export consumes the shared outbox claim and
discards the synthetic dispatch, so neither rail can subsequently claim it again.

## Canonical signed envelope

Exactly **533 bytes**, unsigned integers in big-endian order, identities the full
32-byte Ed25519 Hawala public key. No JSON, optional fields, padding, trailing
bytes, alternative field order or implicit default values. All fields are signed.

| Offset | Length | Field |
| --- | ---: | --- |
| 0 | 8 | ASCII `HWOPT001` |
| 8 | 1 | kind: 1 precommitment, 2 challenge, 3 response, 4 observation |
| 9 | 32 | network ID |
| 41 | 32 | transfer ID |
| 73 | 32 | accepted bilateral terms digest |
| 105 | 32 | permanent attempt ID |
| 137 | 32 | QR-presented holder identity |
| 169 | 32 | QR-presented terminal identity |
| 201 | 32 | origin hawaladar identity |
| 233 | 32 | disbursing hawaladar identity |
| 265 | 32 | full M6-C7 coordinated admission binding |
| 297 | 8 | exact payout in integer minor units |
| 305 | 8 | disclosed payout fee in integer minor units |
| 313 | 8 | pinned authority epoch |
| 321 | 3 | uppercase currency code |
| 324 | 32 | signer identity |
| 356 | 32 | session ID |
| 388 | 32 | terminal challenge nonce |
| 420 | 32 | parent envelope digest |
| 452 | 8 | issue time, Unix seconds |
| 460 | 8 | exclusive expiration time, Unix seconds |
| 468 | 1 | report: 0 handshake; 1 claimed delivery; 2 claimed no delivery; 3 unknown |
| 469 | 64 | Ed25519 signature |

Signature input: ASCII `hawala/optical-TEST-evidence/v1` followed by one NUL byte,
then bytes 0 through 468 inclusive. Verification uses Ed25519 strict verification.
Envelope digest: BLAKE3 derive-key context `hawala/optical-TEST-envelope/v1` over
all 533 signed bytes. These TEST domains cannot authorize existing money APIs.
The M6-C7 digest commits the admitted exact quote, postings and both custody
requests; the envelope does not replace their independently retained evidence.

Network, transfer, terms, attempt and coordinated binding must be nonzero. Holder
and terminal differ; the two hawaladars differ. Amount and epoch are positive;
fee may be zero. Every identity is parsed as a verifying key. Precommitments have
zero session/nonce/parent and report 0. Other kinds require nonzero session,
nonce and parent, with session unequal to nonce. Handshake kinds require report 0;
observations require report 1–3. Expiration must exceed a nonzero issue time.
Maximum lifetime: precommitment 300 seconds, other kinds 120 seconds. There is no
clock-skew grace. Future-issued or expired authentication fails closed.

## Bounded multiframe optical transport

Each QR text is `hawala:optical-test:v1:` plus **lowercase hexadecimal** of:

| Length | Field |
| ---: | --- |
| 8 | ASCII `HWQRT001` |
| 32 | message digest |
| 2 | zero-based fragment index |
| 2 | fragment count |
| 4 | total signed envelope byte length |
| up to 192 | exact contiguous fragment bytes |

Message digest uses BLAKE3 derive-key context `hawala/optical-TEST-frames/v1`
over the complete envelope. Frames are not independently signed; the final
signature and independently expected scope decide authenticity. A forged frame
can cause denial of service, never authorization. The actual fixed envelope uses
three frames (192, 192, 149 payload bytes); at most eight frames are permitted by
the transport policy, and version 1 additionally requires exactly 533 total bytes
and the mathematically implied count. Maximum QR text size is 503 characters.
No oversized allocation occurs before checking count/index/length/text limits.

One receiver handles one message, at most 32 frame observations including errors
and duplicates, under a non-renewable 10-second assembly deadline. Identical
fragments and out-of-order delivery are accepted. Conflicting duplicate fragments,
mixed message digests, malformed headers, invalid indices/counts/lengths, uppercase
hex, trailing data, backward time, timeout or final hash/signature failure poison
that receiver. A completed receiver emits once. Missing fragments never emit.
New receiver objects may restart transport, but cannot restart authorization.
A separate [TEST raster experiment](ATM_OPTICAL_MEDIA_EXPERIMENT.md) now renders
and decodes generated QR images using a resource-bounded child. `observe_no_text`
accounts for blank/unreadable images without renewing this deadline. Physical
camera drivers, compressed video decoding, host-wide camera quotas and an OS
security sandbox remain future work.

## Authentication exchange

1. Independently scan both full identity QRs; compare the visible codes/names.
   Provision the terminal's network/identity/TEST authority/epoch locally.
2. Transfer the prior signed TEST commitment optically (or through independently
   controlled offline provisioning). Terminal imports one permanent transfer and
   attempt under that terminal-specific scope. The holder independently pins the
   TEST authority; a QR-supplied authority key is insufficient.
3. Terminal generates independent cryptographically random session/challenge
   values in a future adapter. The deterministic tests inject explicit nonzero
   values; this library is not an RNG. It durably stores exactly one signed
   challenge before displaying it. Parent = precommitment digest. Its entire
   scope equals the imported commitment; its time window lies inside that
   commitment. Nonces and sessions are unique throughout the terminal database.
4. Holder authenticator verifies the pinned commitment, scanned terminal key and
   challenge signature, exact scope/parent/lifetime, and its own QR identity.
   Review the amount, fee, currency, terminal and bilateral transaction on its
   independent trusted display. `respond` is a test signing primitive, not proof
   that a person actually reviewed the display. Response parent = challenge
   digest; scope/session/nonce/times are unchanged. The holder signs it.
5. Terminal camera delivers QR text. In one SQLite `BEGIN IMMEDIATE` transaction,
   terminal verifies the independently scanned holder, stored challenge and
   commitment, exact parent/scope/session/nonce/times and current validity, then
   stores the complete response and transitions `challenged → consumed_held`.
   Commit occurs before acknowledgment. Identical replays return only
   `IdenticalReplayHeld`; changed responses reject. There is no reusable permit.
6. A signed observation can reference that response, retaining identical scope,
   session, nonce and time window. Only the pinned terminal key can sign it.
   Different report values are retained as conflicting complete envelopes and
   return `ConflictHeld`. A signature attests authorship, not sensor truth.

Terminal challenge signatures authenticate the terminal to the holder; holder
response signatures authenticate the holder to the terminal. Neither proves
physical co-location. Another terminal, attempt, session, amount or challenge
cannot accept a relayed response. An **unchanged live relay to the intended
terminal remains possible** and is an explicit adversarial test. No cryptographic
distance bounding or human-presence guarantee is claimed. Camera compromise can
record, suppress, reorder or relay signed traffic, and deny service; without the
independent signing keys it cannot change consent. A compromised signer/trusted
controller or dishonest issuer is outside this test authentication guarantee.

## Durability, interruption and recovery

`TerminalLab::create` exclusively creates a new file and enrolls one immutable
policy; an existing file (including damaged/empty stores) rejects enrollment.
`open` requires existing policy/schema and exact matching policy, and never
recreates missing intent tables. SQLite uses WAL/FULL, file-backed storage,
serialized write transactions and uniqueness constraints for transfer, attempt,
commitment, session and nonce. Memory databases and SQLite URI paths reject.
A trusted-time high-water mark commits **even for rejected/expired messages**;
a later rollback of supplied time fails. Each consumption transaction rechecks
that high-water mark to handle independent connections safely.

A challenge is permanent. Restart can finish that same still-valid exchange;
it cannot issue a new one. Expiry, interruption, abandoned frames, lost ACK and
process death never reset an attempt. No cleanup/recycling API exists. The TEST
lab accepts observations only inside the session lifetime; later evidence must
follow the independent online reconciliation workflow, not revive authentication.
There is no financial release, balance mutation or retry in the terminal database.

These guarantees assume one authoritative unrolled-back store per terminal,
honest durable storage, a protected signer and a trustworthy clock. Restoring
old backups or cloning the same identity/authority into independent stores can
break local single-use protection. Terminal binding prevents replay at a different
honestly provisioned terminal; it does **not** solve conflicting offline issuance,
malicious duplicated keys or global offline double spending. No independent
terminal can discover current remote allocations or custody while disconnected.
Prior online allocation/fencing plus reconciliation are indispensable, and actual
cash operations remain unavailable in this increment.

The existing M6-C7 financial state machine is unchanged. Export reuses its
already-modeled outbox claim; no new financial state transition is introduced.
The optical state machine has adversarial Rust tests, not a universal Lean/Rust
refinement proof or a formal physical-security proof. Existing financial
Lean/Quint gates and workspace tests remain required by CI.

## Reproduce

```sh
cargo test -p hawala-optical-test --all-targets
cargo test -p hawala-commit-sim --test atomic optical
cargo test --workspace --all-targets
```

Tests cover bidirectional bounded reassembly, replayed videos, future/expired and
delayed frames, gaps/reordering/duplicates/conflicts, malicious headers and sizes,
every single-byte envelope mutation and every truncation, wrong authority/QR/
terminal/network/epoch/session/scope, camera forgery, explicit live-relay limits,
interruption and restart, lost acknowledgment, abrupt process death without
Rust destructors, concurrent independent SQLite connections, ignored/failed writes,
contradictory signed claims, duplicate transfers/attempts and policy replacement.
Integration tests use actual M6-C7 admission for all four human/machine pairings,
check both responsible hawaladars, prohibit uncommitted/aborted/legacy/changed/
already-claimed exports, and compare financial projections before/after the
optical exchange. Existing terminal tests independently exercise uncertain cash.

Related: [TEST image experiment](ATM_OPTICAL_MEDIA_EXPERIMENT.md), [ATM contract](ATM_FIRST.md), [live temporary QR](TEMPORARY_QR_AUTH.md),
[M6-C7 admission](M6_C7_COORDINATED_ADMISSION.md),
[synthetic outbox](M6_C4_TEST_OUTBOX.md),
[ATM observations](ATM_PROVIDER_SIMULATOR.md),
[Iroh identity binding](iroh-identity-binding.md).
