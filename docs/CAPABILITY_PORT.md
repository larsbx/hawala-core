# `HawalaCapabilityV1` local port

Status: **reference admission logic; no transport, no financial authority.**
Addresses ADR 0005 gaps **G6** (port authentication) and **G8**
(participant keys stay on participant devices).

The `hawala-capability-port` crate defines what a Hawala service checks
before it processes a call from its host platform, for example a biz-kit
installation, and how the host authenticates the reply. It is pure: the
transport (a local socket) and durable replay storage are injected.

## Host → service: `admit_call`

The host signs each `HostCall` with its pinned host key over the
`hawala/capability-port/call/v1` domain. The service admits a call only if
all of these hold, checked in this order:

1. The local `PortPolicy` is well formed. `host_keys` contains the host key
   and not the service key, and the TTL bound is at most 60 s.
2. The call's version is `VERSION`, it is addressed to this service, and it
   comes from the pinned host.
3. The signature verifies, `issued_at ≤ now < expires_at`, and the TTL fits
   the policy.
4. The host grant `{id, digest}` exactly matches a pinned grant that permits
   this operation.
5. `body_digest` equals the digest of the body the service actually
   received.
6. The call nonce is fresh. It is consumed **only after** steps 1–5, so a
   rejected call never burns it.

`AdmittedCall` proves port admission and nothing else. It is not a
reservation, a financial permission or payment evidence.

## Participant consent: `admit_authorization`

The host's word that a customer consented is never accepted. For
`SubmitAuthorization`:

- The service re-verifies the participant-signed transaction-QR
  authorization itself, through `hawala-qr-auth::verify_signed`.
- It refuses the authorization if its signer is **any** key the host holds
  (`HostSignedForParticipant`). The host may hold only its own node and
  operator keys. Consent is signed on the participant's own device.
- The call must commit to exactly that signed object (`body_digest` equals
  the authorization's proof digest).

The transaction-QR nonce is **not** consumed here. It stays with the
financial authority, atomically with its reservation.

## Service → host: `verify_response`

The service signs each `ServiceResponse` over the
`hawala/capability-port/response/v1` domain. The response binds the call
digest, a port-level `Status` (`Rejected`, `Held` or `Received`) and a
time. The host accepts it only under the pinned service key and only for
the call it sent. `Received` carries no financial meaning; financial states
are ADR 0005 gap G1.

## Signing domains

Both domains begin with `hawala`, like every Hawala signing input. biz-kit's
ledger signer refuses that prefix (ADR 0004 D3, gap G9). A host must
therefore sign calls with a dedicated host key, never with a biz-kit ledger
key.

## Not yet covered

- The socket transport and its version negotiation.
- Durable replay storage.
- How biz-kit grants map onto `GrantPolicy`.
- Operation-specific bodies beyond `SubmitAuthorization`.
