# Signed TEST endpoints and durable business outbox

Status: **restricted synthetic implementation**, 2026-10-28. ADR 0005 remains
**Proposed**. Hawala owns this protocol and its financial semantics. This
profile layers local signed RPC and a business outbox over the persistent
[TEST actors](CROSS_AUTHORITY_HOST_CONFORMANCE.md); it does not implement the
financial evidence in [CROSS_AUTHORITY_V1](CROSS_AUTHORITY_V1.md). Hawala's
independent Iroh protocol and ActivityPub's public-discovery-only role remain
unchanged. No domain checkout, goods release or live-money endpoint invokes it.

## Typed operations and consumer gates

| Operation byte | Name | Required stage | Successful TEST stage | Business action permitted |
| --- | --- | --- | --- | --- |
| 1 | `create` | 0 absent | 1 `test_proposed` | Retain synthetic attempt correlation |
| 2 | `prepare` | 1 | 2 `test_prepared` | Display synthetic preparation only |
| 3 | `commit_intent` | 2 | 3 `test_reserved` | Retain synthetic reservation only |
| 4 | `status` | Any | Current independently read actor projection, or HOLD | Resolve the original outbox item if verified and sufficient |

Every response encodes `financial_authority=0` and `paid=0`. Both verifiers
reject even correctly service-signed nonzero claims or a stage above 3.
`test_reserved` is neither `Executed(observed)` nor `Settled`; it does not
satisfy a financial workflow gate. The service key attests TEST journal state,
not participant consent, funds, custody or finality. No abort, release, refund,
compensation, evidence-issuance or arbitrary actor-event endpoint is exposed.
The canonical six financial states and workflow requirements remain unchanged.

## Exact signed bytes

Integers are unsigned big-endian, bytes are concatenated without padding, and
`B32` means exactly 32 bytes. Domain strings below include one terminating zero
byte. Ed25519 signs the entire preceding payload directly, without prehashing.
One connection carries one `U32BE length || body` request and response; bodies
are nonempty and at most 4096 bytes. Parsers require exact lengths and domains.

Request payload, followed by a 64-byte host Ed25519 signature:

```text
"hawala/TEST-endpoints/request/v1\0"
operation:U8
pinned_service_key:B32  enrolled_host_key:B32  grant_id:B32  authority_epoch:U64
request_id:B32  attempt_id:B32  immutable_binding:B32
issued_at_seconds:U64  expires_at_seconds:U64
```

The service verifies the signature with its enrolled host key, the pinned
service, grant, epoch and operation mask, nonzero request/attempt/binding IDs,
and `issued <= now < expires`, with `0 < expires-issued <= 60`. An embedded key
never establishes trust. Freshness is checked before cached receipt lookup;
an expired original mutation must be resolved by a fresh status request.
The enrollment hash binds both keys, grant, epoch and operation mask. Existing
storage refuses a changed pin; there is no implicit epoch migration or new
recovery mandate. These are local TEST grants, not participant mandates.

Response payload, followed by a 64-byte service Ed25519 signature:

```text
"hawala/TEST-endpoints/response/v1\0"
pinned_service_key:B32  SHA256(full_signed_request):B32
attempt_id:B32  immutable_binding:B32
stage:U8  held:U8  financial_authority:U8=0  paid:U8=0
request_authority_epoch:U64
```

The relying outbox independently verifies every field, the pinned service
signature, the exact request hash, stage range and Boolean values. A response
from another operation, request, attempt, binding, key or epoch cannot advance
it. The binding is an opaque immutable synthetic manifest digest; this profile
does not validate a real financial manifest or signatures on it. An exact replay
returns the historical signed receipt, not a fresh assertion of current state.

Outbox request IDs are `SHA256("hawala/TEST-outbox/id/v1\0" || attempt_id ||
immutable_binding || counter:U64)` with a durable monotonically increasing
counter. Enrollment pins host/service keys, grant, epoch, attempt and binding;
the socket locator is not a trust root. Public TEST seeds in fixtures are never
credentials. Node's standard crypto library independently produces
`formal/evidence/testEndpoints.tsv`; Rust and Elixir compare exact request and
response bytes against it, including both signing domains.

## Persistence, failure and recovery

The Rust endpoint journal records the signed request and pending target in one
SQLite WAL/FULL transaction **before** actor effects. The coordinator and two
participants retain independent SQLite journals. Phase 2 runs a fixed prepare
sequence, and phase 3 a fixed commit/publish/deliver sequence. Callers cannot
inject votes, epoch fences, successor mandates or arbitrary model events.
There is no transaction shared across these journals.

A permanent request ID binds its exact signed body; substitution is rejected
before any recovery write. The first valid create binds attempt ownership and
manifest digest. Duplicate or later requests cannot replace either. A pending
target blocks new mutation effects, including requests with new IDs. A status
query may finish the endpoint journal only after reading complete durable actor
effects for that target; it never sends those effects again. A partial prepare,
missing actor, aborted history or uncertain completion stays HOLD. There is no
automatic abort, empty-history enrollment, speculative retry or timeout release.
Every fresh call rechecks actor availability even after an earlier complete ACK;
a new request for an old phase cannot bypass missing-history HOLD.

The Elixir outbox records and syncs the complete signed dispatch before send,
then records a verified response or uncertain result. Restart never resends a
mutation. A lost response retains the original pending attempt; another mutation
is refused. Only a newly signed status request can resolve it. Uncertain append
stops the outbox; logs are never silently repaired. Enrollment/pin mismatches
fail closed. The outbox is an independently persisted business record, separate
from all financial TEST actor journals. Its ownership registry fences only
within one BEAM VM; trusted local storage, a single owning process and retained
history are assumptions. Rollback, malicious storage and cross-process fencing
are not proved. A local signed service receipt is not a distributed financial
certificate.

The actual cross-process test drops the successful commit reply after durable
completion, restarts the Elixir outbox, refuses a replacement commit, and resolves
through signed status with exactly three mutation dispatches and `paid=false`.
Rust failure injection also interrupts the endpoint completion transaction after
actor commit, recovers by observation on restart, and leaves partial prepare
held without replay. Missing history after completion makes fresh status held.

## Proof increment and negative controls

`EndpointOrchestration.lean` proves the transition invariant by induction for
arbitrary finite histories: at most one TEST commit effect, client progress no
higher than completed service state, no paid state, invalid/duplicate admission
stuttering, no replacement while pending, timeout preserving effects/outbox, and
no completion of unresolved effects. A held business state blocks new mutation
dispatch even when there is no pending mutation; only a verified status reply
may clear that hold. Signature validity, request freshness,
durable storage and complete actor observation are abstract premises. The
partial actor implementation is covered by the earlier cross-authority model
and targeted runtime failures; no end-to-end refinement theorem is asserted.

`endpointOrchestration.qnt` independently defines this safety machine. TLC
exhaustively checks its 101 reachable states. The Lean exporter contains a finite
search certificate: the kernel checks every witness path and closure under all
15 model actions. Its 1,515 generated edge witnesses are compared against Quint.
The restart action assumes retained durable state; it does not prove filesystem
power-loss or database recovery. No availability, fairness or bounded recovery
liveness theorem is claimed: partitions and missing history may block forever.

Four mutant step relations must produce actual TLC invariant violations:
acknowledgment treated as paid, unbound response advancing the client, forgotten
committed history and mutation retry executing twice. Retained normalized traces
are in `formal/traces/endpoint-negative.json`, with safe-prefix and terminal
mutation checks. An infrastructure error cannot pass the negative-control gate.

```sh
(cd formal/lean && lake build)
bash tools/check_trace_artifacts.sh
bash tools/check_endpoint_orchestration.sh
cargo test -p hawala-cross-authority-test --features conformance
cargo build -p hawala-cross-authority-test --features conformance --bin hawala-test-service
# In paired biz-kit/apps/hawala_capability, with absolute binary path:
HAWALA_TEST_SERVICE_BIN=/absolute/path/to/hawala-test-service MIX_ENV=test mix test --no-start
```

## Transport and deployment limits

The opt-in Rust binary requires `--synthetic-only`; default Cargo builds exclude
it. It accepts either a Unix socket (mode 0600, refuses an existing path) or an
ephemeral **127.0.0.1-only** listener whose port is written to a new private file.
There is no public bind option, HTTP routing or production application wiring.
Connection reads/writes have timeouts and framing bounds. The loopback locator
file remains after exit; restart uses a new file and the same pinned identity.
`--initialize` explicitly creates a new store; omitting it requires existing
history. `--drop-first-commit-reply` is solely a test fault injection.

The local sandbox denies Unix socket bind, so cross-process validation uses
loopback; the Unix transport is implemented but has not been exercised here.
Biz-kit compiles all endpoint/outbox helpers only under `MIX_ENV=test`; pairing
CI builds the exact pinned Rust service and exercises the real process exchange.
No production modules, host-held participant keys or Iroh message changes are
introduced. G3/D5 actual financial-writer refinement, G4–G8 real capability,
trust, privacy and key gates, independent review and production authorization
remain open. This restricted TEST orchestration does not authorize real checkout
fulfillment or demonstrate distributed financial settlement readiness.
