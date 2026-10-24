# M4-A — On-wire Iroh mutual identity handshake

Status: **prototype**. This documents the wire exchange implemented in
`hawala-iroh-bridge::handshake`, not a money-movement or wallet protocol.

## Configuration

- Transport: Iroh 1.4.1 QUIC, fully completed TLS authentication.
- ALPN: `hawala/peer-identity/1`.
- Identity: persistent Ed25519 key represented by QR, alongside a readable
  public code and optional display name. Each side independently scans the
  OTHER party's full identity QR before starting.
- Iroh EndpointId: transport/device identifier authenticated by the QUIC
  handshake, not the permanent participant identity.
- Messages: exactly four ordered, distinct uni-directional QUIC streams.
  Each finished stream carries one complete fixed-layout frame. Receivers
  enforce a 192-byte maximum and exact message lengths (no JSON).
- Frame header: eight ASCII bytes `HWHAND01`, one message kind byte.
  Network byte order for u64 timestamps. The payload is binary.
- Message 1: HELLO = initiator identity 32 + expected responder identity 32 +
  initiator CSPRNG nonce 32 + issued_at u64 + expires_at u64.
- Message 2: RESPOND = responder identity 32 + responder Ed25519 signature 64
  + responder CSPRNG nonce 32 + issued_at u64 + expires_at u64.
- Message 3: PROVE = initiator identity 32 + Ed25519 signature 64.
- Message 4: ACK = BLAKE3 derivation-context hash of all previous three
  complete frames, individually length-prefixed, 32 bytes.

## Signature verification

Each signature uses the M2 `EndpointChallenge` v1 domain-separated encoding.
Signed fields: verifier identity, subject identity, subject authenticated
transport EndpointId, nonce, purpose `peer-auth`, issued_at, expires_at.
Both sides validate the QR-scanned full public key, live authenticated
`remote_id()`, bounded lifetime (120 sec), signature, timestamp freshness,
and `ReplayGuard::consume_once` before accepting identity.

The acceptor does not trust the initiator's claimed public key: it compares
it against a separately scanned expected identity before signing its proof.
The initiator similarly must already know and verify the acceptor's full
QR identity, NOT infer it from the response.

## Integration test

`live_two_peer_iroh_handshake_binds_qr_identity_to_transport` brings up
two real Iroh endpoints using local/direct `presets::Minimal`, connects them
through the ALPN, exchanges all frames, and checks both authenticated
identity-to-EndpointId bindings. No hosted network, relay guarantee,
and no physical or financial transfer is involved.

## Remaining security gates

1. An optional SQLite guard makes nonce consumption durable **only within
   one verifier authority sharing the SAME database**. Identity-key reuse
   across devices requires a separate coordinated authority; local storage
   cannot prevent cross-device replay.
2. Each frame now has a 20-second deadline, including peer stream receipt
   acknowledgment. Connection dialing, overall handshake budget, per-peer
   rate limits, and concurrent-session limits are still application gates.
3. No hardware-backed device key custody or secure loss/compromise recovery.
4. Clock skew requires policy; timestamp now is local system wall clock.
5. Mutual authentication here only establishes identity and transport,
   not business licensing, financial permission, exposure, settlement,
   reputation or an obligation's acceptance.
6. No 0-RTT financial operations. Never put customer financial records on
   public gossip, and use separate domain-separated financial authorizations.
7. More adversarial network-level tests needed: wrong scanner expectation,
   reordered streams, partition, reconnect, relay failure and identity swap.
8. Endpoint addressing/discovery must not depend on phone number, username
   or account number for participant identification.
