# M6-C7: one local TEST coordination/admission commit

Status: **test-only, synthetic evidence and balances**. This adds a common local
decision order to the existing `Simulator`; it does not establish actual custody,
funding finality, operational authority, customer payouts, settlement or ATM
permission. The separate [M6-C6 journal](M6_C6_COORDINATION.md) remains an isolated
protocol lab and cannot supply a financial certificate to this implementation.

## Boundary and immutable preparation

`prepare_test_coordination` permanently selects a coordinated test profile for a
database before any legacy financial event exists. It pins the existing admission
binding plus the origin dar's complete custody request. That includes accepted
terms, both responsible dars, the QR/attempt, quote, postings and version/epoch
references already hashed by admission, and both custody requests' canonical
digests, policies, mandates, accounts, request IDs and allocation IDs.

The origin request must reserve the exact funding amount/currency for the origin
dar in the accepted terms. The disburser request and provenance lot remain subject
to the existing admission verifier's exact payout constraints. Neither request is
a deposit or proof of controlled funds. Both are independently pinned SIGNED TEST
policy claims checked through the existing `CustodyAuthority` interface and
`hawala-custody-model` signature verifier.

The preparation journal permanently retains each transfer binding, both request
and allocation IDs, and each `(custody account, epoch, version)` slot. A logical
slot cannot prepare two operations, including across the two dar roles. This is
not proof against presenting one actual asset through different account IDs or
providers; independent asset/control and lien reconciliation remains required.
Abort does not recycle those slots or free real collateral.

`confirm_test_reservation` records a signed test claim for the pinned side and
request at a current checkpoint. A partition, unsupported real custodian or
unavailable evidence leaves its flag unchanged; invalid signatures reject.
Late confirmations may still be recorded after abort, but cannot reopen it.
New changes require the current financial authority epoch and expected
coordination version. Repeated confirmations or terminal status queries stutter;
these APIs return current test status, not external custody certificates.

## Atomic financial commit and final abort

`admit_coordinated` calls the existing admission implementation. It adds a gate
inside the same `BEGIN IMMEDIATE` transaction: the immutable binding must match,
both side claims must exist, the decision must be undecided, and its version and
financial epoch must be current. The origin signed test authority/checkpoint is
revalidated; the existing admission logic revalidates the disburser authority,
signed eligibility/delegation/QR evidence, reviews, lot and exact reference book.

The same SQLite commit then retains:

- the original balanced liability hold postings and event;
- the original nonce/attempt and provenance-lot consumption;
- both custody checkpoint advances;
- the admission receipt, recovery subject and one synthetic outbox row;
- the bilateral committed decision and its coordination-version advance.

No new accounting or evidence verifier is implemented. The admission binding
hash and prototype state transitions are factored for reuse. A storage failure
even at the final coordination write rolls back every financial effect above.

`abort_test_coordination` enters the same database transaction order. It can
terminally abort only an undecided operation. After abort, delayed coordinated
admission cannot post, consume a nonce or enqueue a dispatch. After commit, abort
cannot alter the decision or release anything. The abort record is a local test
noncommit tombstone, not a certificate accepted by an external custodian.

An identical committed admission replay returns the original non-executable
receipt, even after its evidence has expired. It does not reauthorize execution
or increment either checkpoint. Existing uncertainty returns `Held`. A changed
binding rejects. Once coordinated mode is selected, ordinary `admit` holds and
`record_simulated_hold` rejects throughout that database; another transfer ID
cannot use those older APIs to evade the common order. Existing legacy test
databases retain their original profile and cannot retrofit an old commit.

The existing outbox still claims at most once and marks uncertainty before
returning its synthetic value. An uncertain committed payout remains committed
and encumbered through timeout and restart; it cannot become a preparation abort.
There is no collateral-release or post-commit resolution API in this increment.

## Proof and test evidence

`CoordinatedAdmission.lean` composes the existing `Coordination` and `Hawala`
models. For arbitrary natural-number funding/reservation amounts and arbitrary
finite modeled histories, it proves both component safety predicates, exact
financial conservation, and equivalence of the committed coordination decision
and committed financial intent. Abort permanently fences financial admission;
the legacy event cannot change state. It reuses the existing preservation proofs.

Quint/TLC exhausts an independently authored 18-state instance with 1000 available
and a 900 reservation. Three unsafe mutations must produce counterexamples:
financial commit via legacy bypass, decision-only partial commit, and financial
commit after final abort. A monitor remembers prior aborts for that last check.
Infrastructure errors cannot satisfy the negative controls.

Lean kernel checks establish event-alphabet completeness and graph closure and
export 162 transition paths. Quint checks those paths independently. Rust
replays every prefix through actual SQLite-backed methods for all four
human/machine pairings (648 fixture runs). It checks both decision and financial
projections, exact balances, lot consumption, custody/state versions, nonce,
receipt, posting and outbox counts. Restart is graceful in the trace adapter;
separate child-process tests bypass destructors after commit or abort.

Additional tests cover final-write rollback, concurrent commit/abort, unavailable
or stale custody evidence after confirmation, immutable conflicting requests,
shared custody-checkpoint collisions, legacy bypass/backfill, memory/URI storage
rejection, and uncertain external execution after restart. Admission storage now
rejects SQLite URIs and verifies WAL mode, matching the standalone journal's
durability restriction. Full workspace CI retains the existing live Iroh tests.

These are abstract safety proofs and finite projection conformance, **not** a
universal Rust refinement proof. Confirmation flags abstract independently
verified external reservation finality that this test adapter does not provide.
The model omits post-commit payout lifecycle events, multiple operations/accounts,
concrete signatures and time windows; existing suites test those implementation
surfaces separately. No cross-model universal refinement, progress under a
partition, backup anti-rollback, storage-honesty, or actual distributed custody
atomicity is proved. There is no epoch migration or authority-succession API.

Iroh remains the P2P transport, QR remains the sole presented participant identity,
and both responsible hawaladars remain bound to the operation. Reusable identity
and reputation evidence remains separate from financial permissions.

Reproduce: `lake build` in `formal/lean`; at repository root run
`bash tools/check_trace_artifacts.sh`, `bash tools/check_coordinated_admission.sh`,
and `cargo test -p hawala-commit-sim --test atomic coordinated::`. The full
workspace command remains `cargo test --workspace --all-targets`.
