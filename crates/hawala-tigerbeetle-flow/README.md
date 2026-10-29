# TigerBeetle-first recording and persistence ports

`record` waits only for a trusted TigerBeetle financial decision. `recover`
resolves original IDs without a replacement financial effect. `persist_once`
drains complete native groups into one independent durable destination.

This crate has no network/database adapter and is not wired to a financial
writer or execution service. Plans and structurally checked records are not
signed authorizations. FinancialPort, CommitSource and DurableSink are trusted
ports with explicit security, complete-history and atomic-durability contracts.
The tests simulate those contracts; they do not prove a native adapter meets them.

`replay::HistorySource` now implements `CommitSource` over a trusted paged
`NativeHistory` reader. Its bounded decoder reconstructs groups from persisted
metadata, retains partial suffixes without advancing a complete cursor, rejects
unsupported native modes, and resumes from each destination's durable cursor.
The SDK-specific reader and actual durable sink are still not connected.

The finite certificate covers all 1700 edges of a 100-state model. Quint/TLC
exhausts that finite projection, and Rust checks the same certificate through
submission, recovery, decoding and independent sink persistence with synthetic
trusted ports. Lean sources and a byte-identical exporter are present; local
Lean startup is blocked before elaboration by sandbox process-path access.
Require the hosted pinned Lean build/export check before claiming Lean acceptance.

See the selected [node-books profile](../../docs/NODE_BOOKS_TIGERBEETLE_SPEC.md)
for group metadata, retained evidence, supported posted-transfer profile,
financial ownership, recovery, business admission and remaining native tests.

```sh
cargo test -p hawala-tigerbeetle-flow --locked
cargo clippy -p hawala-tigerbeetle-flow --all-targets -- -D warnings
python3 tools/check_tb_first_mutations.py
bash tools/check_tb_first_quint.sh
```
