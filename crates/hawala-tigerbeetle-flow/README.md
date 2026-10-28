# TigerBeetle-first recording and persistence ports

`record` waits only for a trusted TigerBeetle financial decision. `recover`
resolves original IDs without a replacement financial effect. `persist_once`
drains complete native groups into one independent durable destination.

This crate has no network/database adapter and is not wired to a financial
writer or execution service. Plans and structurally checked records are not
signed authorizations. FinancialPort, CommitSource and DurableSink are trusted
ports with explicit security, complete-history and atomic-durability contracts.
The tests simulate those contracts; they do not prove a native adapter meets them.

See the selected [node-books profile](../../docs/NODE_BOOKS_TIGERBEETLE_SPEC.md)
for group metadata, retained evidence, supported posted-transfer profile,
financial ownership, recovery, business admission and remaining native tests.

```sh
cargo test -p hawala-tigerbeetle-flow --locked
cargo clippy -p hawala-tigerbeetle-flow --all-targets -- -D warnings
python3 tools/check_tb_first_mutations.py
```
