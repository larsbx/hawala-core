# Iroh-bound QR identity — milestone 2

This milestone bridges cryptographically authenticated Iroh transport with
QR-presented, human-readable Hawala identity. It is **not** a full P2P message
exchange and must not be interpreted as authorization to transfer funds.

## Identity handshake (protocol contract, not yet an on-wire implementation)

1. The user scans a full Hawala identity QR and compares the adjacent human
   name/code. The short code cannot substitute for the full public key.
2. A verifier establishes an Iroh v1.4.1 QUIC connection using the dedicated
   `hawala/peer-identity/1` ALPN. The peer EndpointId is obtained from
   `Connection::remote_id()` **after** the TLS handshake.
3. The verifier generates a cryptographically random, nonzero 32-byte nonce,
   constrains proof expiry to 300 seconds, and builds an EndpointChallenge
   naming the expected QR identity, observed endpoint ID, verifier, purpose,
   issuance time and expiry. Neither party may trust an endpoint ID supplied
   by unauthenticated application data.
4. The identity holder checks that the challenge matches its **own** Iroh
   endpoint, and evaluates the verifier/purpose before signing. The holder
   responds with `SignedEndpointProof`, signed under its long-lived identity key.
5. The verifier checks the full signed statement, observed Iroh endpoint,
   expected QR key, time bounds, and atomically persists nonce consumption.
   A successful verification returns `VerifiedPeer`, bound to this endpoint
   and valid only until expiry.
6. BOTH participants must be independently authenticated before exposing
   private information or accepting financial instructions.

## Important trust boundaries

- Iroh's TLS certificate authenticates an **endpoint**, not a human or
  business. The signed proof binds these separately.
- This milestone deliberately retains the identity-core decision **not to
  introduce an identity or reputation revocation service**. Short-lived
  session proof expiration is a freshness property, not revocation.
- Names and readable codes are public recognition hints; verified-name
  claims require issuer-specific credentials. They are not auth factors.
- A VerifiedPeer is not Hawala financial-agent authorization, bilateral
  credit approval, KYC, sanctions clearance or permission to debit funds.
- No app should treat this as a cash-out primitive. Authorization must be
  amount-, currency-, recipient-, agent-, session- and transaction-bound,
  with durable once-only execution in the financially responsible node.

## Not yet implemented

Authenticated on-wire framing and serialization, Iroh connection lifecycle
and discovery, session teardown, key rotation/recovery, CSPRNG nonce creation,
durable nonce storage, denial-of-service controls, reputation sharing,
signed obligations, clearing and settlement. A real online two-peer test,
including relay and network partition, is required before production.

The `hawala-iroh-bridge` crate is intentionally thin: it accepts an already
established Iroh `Connection` and reads `remote_id()` itself. No API exists
that accepts a client-supplied endpoint ID as proof of transport identity.

**Next:** Implement framed peer-handshake exchange (bounded messages, mutual
authentication, replay persistence), then signed, bilateral Hawala obligation
state transitions with deterministic reconciliation.
