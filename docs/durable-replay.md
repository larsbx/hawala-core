# M4-C — durable QR identity replay protection

This is a **local** crash-durable implementation of the shared
`hawala_identity_core::ReplayGuard` trait.

The `hawala-identity-sqlite` crate stores only verifier key, claimant key
and nonce (3 x 32 bytes), with no balances or personal details, in a
file-backed SQLite database. WAL journaling and FULL synchronous settings
provide durable, atomic single-insert semantics on a correctly configured
filesystem. A unique composite key ensures that races from multiple threads
or SQLite handles sharing the same file yield exactly one success.

On any database failure the guard returns `IdentityError::Storage` and
the identity authentication must be rejected. Filesystem access and
encryption are the responsibility of the host; no memory-database fallback
exists. One-time nonce tombstones are never pruned automatically.

## Usage

```rust,no_run
use hawala_identity_sqlite::SqliteReplayGuard;
let guard = SqliteReplayGuard::open("/var/lib/hawala/identity-replay.sqlite")?;
// Pass &guard to handshake::initiate/accept; do not create one DB per attempt.
# Ok::<(), hawala_identity_sqlite::OpenError>(())
```

## Explicit limits

- This does NOT coordinate multiple devices or hosts that maintain separate
  databases under the same identity. Such deployments must share a real
  transactional authority or use distinct operational identity keys.
- Nonce consumption is **identity proof replay protection only**; it is
  not an at-most-once cash dispense, ledger reservation, liability, or
  Hawala transfer operation.
- Existing signed identity evidence and ratings remain append-only;
  this is not an identity revocation registry.
- Strong SQLite durability depends on trusted local storage, fsync support,
  reliable locks, correct filesystem behavior and operational backups.
  Never share the file over unreliable network filesystems.
- Operational recovery from backup needs anti-rollback controls; restoring
  old nonce state can admit old challenges if still within validity.
- A 20-second frame deadline helps prevent peers stalling one QUIC stream;
  overall connection limits and resource caps are not in this crate.

## Validation

- Restart and replay: consumed nonce remains consumed after reopen.
- Concurrency: 16 local threads racing on one nonce yield one winner.
- Separate handles on one SQLite file reject duplicate inserts.
- In-memory DB and all-zero nonces are rejected.

**Next:** M5 crash-safe payment journal with externally reconciled cash
execution and an explicit indeterminate state. This is materially separate
from identity authentication.
