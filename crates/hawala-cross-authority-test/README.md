# TEST host conformance

This optional, **test-only** crate implements the canonical cross-authority
transition boundary using three separate SQLite journals. It has no library
surface unless `conformance` is explicitly enabled. No other workspace crate
depends on it. It has no network, signing, custody, payment or execution API.

```sh
cargo test -p hawala-cross-authority-test --features conformance
```

The coordinator, left authority and right authority store only their own fields
and permanent request receipts. Local state and receipt commit together under
`BEGIN IMMEDIATE`, WAL and `synchronous=FULL`; expected versions come from the
same local snapshot used to calculate the transition. No transaction spans
financial authorities. Exact replay returns the original local receipt;
queries show current state, and payload changes conflict. Missing or incorrectly
enrolled stores cannot be initialized by recovery.

A separate fault driver owns availability/network controls. Its accepted events
are **abstract test facts**, not independently verified wire messages, effective
hardware fences, custody confirmations or recovery mandates. The harness reads
other test journals to construct those abstract inputs; it is not a distributed
financial protocol implementation. The production signing/append boundary must
verify the actual evidence and grants before supplying analogous facts.

Tests replay all 232 Lean-generated prefixes and 72 evidence-shape gates,
reopen journals after every prefix, inject SQLite receipt-write failure, race
independent coordinator connections, and exit a subprocess after commit before
publishing or acknowledging it. This demonstrates selected conformance and
local persistence behavior under the specified tests, not universal refinement,
Byzantine/rollback safety, power-loss guarantees or financial settlement.
