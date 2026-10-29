# Native storage conformance increment

Opt-in official TigerBeetle **0.17.9** Rust SDK adapter, signed admission custody,
and an independent durable SQLite signed-outcome destination. No binary, service,
production financial writer, executor, settlement attestor or deployment is added.
`native-conformance` is off by default and the native constructor accepts only an
explicit loopback socket. This is a TEST integration surface, not release approval.

The adapter implements `FinancialPort` and `NativeHistory`. It requires the
original signed plan to be retained durably before submission; never allocates
replacement IDs during retry; submits exactly one linked batch; and reads every
original transfer back, normalizing by ID rather than response position. Exact
native fields, posted mode, per-leg link flags, epoch/count/index metadata and
consecutive native timestamps must match. Partial groups and changed bodies are
conflicts. SDK failures and missing records remain unknown, never a terminal
rejection of an earlier uncertain submission. Same-body retries use the same
operation anchor and whole ID range. Native accounting remains the only financial
commit, irrespective of destination availability.

`EvidenceStore` uses SQLite WAL + `synchronous=FULL`, exact original signature
bytes, a pinned authority/context/registry/cluster/generation and transactional
reservations of every transfer ID. It is evidence custody, not a financial
journal or spend authorization. The TEST admission signature binds exact native
plan bytes and original business-request bytes. The deterministic compiler is
re-run on verification. This test authority cannot substitute for real participant
consent, policy, effective writer fencing, spend-critical guards or business
admission. An externally governed allocator is still required across independent
writers/evidence stores; local reservations prove only this one store's range
uniqueness. Missing original evidence blocks replay rather than being recreated
from a hash, a new signature or a projection.

`SignedSink` uses a separate durable SQLite database per destination. One IMMEDIATE
transaction inserts the entire destination-bound signed outcome and predecessor,
checks the expected cursor and advances that cursor. It signs and retains the
original admission, exact plan and complete native timestamps. Exact delivery is
idempotent, changed body/predecessor is a conflict. Reopen verifies all signatures,
body/identity bindings and cursor continuity. Another destination has its own
key, database and cursor. Replay reads native history and has no financial-submit
call. Whole-database rollback to an older valid prefix needs an external checkpoint;
this local sink does not claim anti-rollback protection against its storage owner.

## History authentication boundary

Unfiltered native `query_transfers` pages cover the isolated test cluster in
native order. Every row is checked against retained signed admission and an
independent whole-group original-ID lookup. A partial/malformed group, foreign
history, missing evidence or mode drift cannot advance a destination. There is
no ledger/code/user-data filter that silently hides unknown history.

**TigerBeetle does not cryptographically sign its query results.** These native
reads trust the loopback server, OS/network boundary and complete unfiltered SDK
query semantics. A malicious transport that omits a whole group is not detected
by count/index metadata alone. This increment authenticates financial bodies to
a node-scoped TEST admission key; it does not implement authenticated production
transport or prove adversarial history completeness. Those remain separate gates.

The pinned 0.17.9 `state_machine.zig` stores `t.flags` and returns the stored
transfer on lookup. The adapter verifies its exact expected linked bit before
normalizing to the core decoder's posted-transfer representation (`flags=0`).
Group association still uses persisted count/index and operation metadata, not
the flag alone. This corrects the blanket "flags are not retained" statement
in the earlier architecture description. Native server acceptance remains required.

## PR #50 reconciliation

The registry encoding is byte-compatible with #50 at
`995a37d40ce57a386d4004cd6dbb656fe2191360`; a verbatim independent fixture proves
that equality. Account identities map injectively, ledgers carry currency/scale,
and successors preserve prior rows. The compiler preserves ledger/account sorting,
minimum pairing, positive i64 amounts, <=256 postings, <=255 transfers and
`first_id + index` allocation. Independent #50 hand-specified pairs are reused.

TB-first changes are explicit: `first_id == operation`, `user_data_128` is that
immutable anchor, `user_data_64` is the scoped epoch, `user_data_32` is count/index.
Full business event identity and signatures remain in retained evidence. #50's
SQL financial-sequence/projection checkpoint is **not** reused as commit authority.
Its entire conflicting draft is not silently merged or declared accepted.

## EventStore integration boundary

This sink is an independent durable signed-outcome archive. It is **not** a
biz-kit EventStore append or a replacement for `CoopSubstrate.Log`. EventStore
consumption must verify this source evidence and create an admitted registered
business envelope through `Log.append`/`append_if`; it must not insert into
EventStore tables directly. Append-before-cursor idempotency must be established
by that actual host adapter before enabling it. This increment leaves that host
adapter unconnected and preserves execution-observer and settlement-attestor gates.

## Validation and release gate

```sh
cargo test -p hawala-tigerbeetle-flow -p hawala-tigerbeetle-native --features native-conformance --locked
cargo clippy -p hawala-tigerbeetle-native --features native-conformance --all-targets --locked -- -D warnings
python3 tools/check_tb_storage_mutations.py
bash tools/check_tb_storage_quint.sh
TB_BINARY=/absolute/path/to/tigerbeetle cargo test -p hawala-tigerbeetle-native --features native-conformance --locked --test native -- --ignored --skip native_worker --nocapture --test-threads=1
```

The mandatory native workflow downloads the exact server archive by SHA-256.
Three real-service tests cover interrupted queued submission, lost application
acknowledgment after financial commit, original-ID restart recovery, last-leg
linked rejection, partial history, missing evidence, duplicate delivery, locked
SQLite destination, independently recovering cursors, server restart and exact
final native balances/counts. The interrupted request may or may not commit before
the worker exits; recovery must accept either outcome using the same IDs. It does
not claim a deterministic mid-consensus crash or a three-replica partition test.

Local results: eight checks pass (seven durable/accounting tests and one native
wire-normalization unit test), all four unsafe Rust controls fail their intended
assertions; Quint exhausts 18 states and rejects both unsafe model controls. The
three real-service tests compile but local TigerBeetle formatting fails with
`io_uring ... PermissionDenied`. They are ignored by ordinary workspace tests
and explicitly required without skips in their own workflow. Lean 4.28 startup
fails before elaboration (`failed to locate application`); hosted kernel checking
is still required. Lean/Quint model atomic storage transitions under named trusted
predicates, not a verified implementation of SQLite, the SDK or cryptography.
Native and exact-head hosted gates must pass before accepting this draft; passing
them alone does not enable production financial writes.

Official references (version-specific source is authoritative for this adapter):
- https://github.com/tigerbeetle/tigerbeetle/tree/0.17.9/src/clients/rust
- https://github.com/tigerbeetle/tigerbeetle/blob/0.17.9/src/state_machine.zig
- https://docs.tigerbeetle.com/reference/requests/create_transfers/
- https://docs.tigerbeetle.com/reference/requests/query_transfers/


Durable constructors require an explicit filesystem filename, reject temporary,
memory and SQLite URI profiles, and verify the effective WAL, FULL synchronous
and foreign-key settings. Merely executing a PRAGMA request is insufficient:
SQLite may return another journal mode without an SQL error. This checks the
storage configuration assumed by the abstract model; filesystem/fsync honesty
and external anti-rollback anchoring remain separate assumptions.
