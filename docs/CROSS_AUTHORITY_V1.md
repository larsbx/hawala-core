# Cross-authority evidence and blocking commit, version 1

Status: **Proposed proof increment**, 2026-10-28. Canonical owner:
`hawala-core`. This refines ADR 0005 G1–G3, G10 and G11; it does not accept
that ADR, close the host-conformance gate, or supply a financial deployment.
M6-C6/C7 remain local synthetic implementations. There is **no distributed
financial implementation** in this increment. This profile is bilateral: two
independent financial authorities, one immutable manifest/attempt, and a
separately mandated scoped decision coordinator. More participants require a
new profile and proof. Independent non-biz-kit Iroh peers remain supported.

## 1. Trust boundary and typed business states

A signature verifies an attributable assertion, not the truth of cash delivery
or legal finality. A relying instance must independently pin the authority,
execution observer, settlement attestor, scope, role, keys, effective epoch
chain, finality policy and recovery mandate. The embedded issuer, transport,
biz-kit log inclusion, ActivityPub discovery and coordinator selection are
never trust roots. G5's discovery/rotation protocol remains open.

`formal/lean/FinancialEvidence.lean` defines the evidence shapes and business
gates. Cryptographic/mandate/custody/finality verification is an **explicit
external input** to those gates, not a theorem proved by the shapes.

| State | Required evidence and accountable signer | What the relying instance must establish |
| --- | --- | --- |
| `Proposed` | Two participant-device `Proposed` signatures on the manifest; accepted dual-signed terms referenced by it | Both consents, live identity/QR proof, exact business revision, destination, amount/fees, disclosure, attempt and policy binding. No reservation or payment claim |
| `Prepared` (protocol substate, not a business success) | One `Prepared` envelope from each scoped financial authority | Durable exclusive non-expiring reservation for that authority's leg, independently accepted custody profile and exact binding; no inferred prepare at the other authority |
| `Committed(intent)` | Coordinator `CommitDecision` binding both prepares, **and both** authorities' `CommitAck` envelopes | Both prepare signatures, mandate and epoch chain; decision's exact prepare-set digest; each ACK's decision, reservation and posting digest; nonce/attempt consumption and postings durably ordered locally. A coordinator certificate alone is an irrevocable decision direction, not a complete business intent receipt |
| `Executed(observed)` | Complete committed evidence plus each leg's `ExecutionObserved` envelope from its pinned execution observer | Original ACK/attempt, destination/amount, external execution ID and retrievable observation evidence. Observation is not delivery/finality |
| `Settled` | Complete observed evidence plus each leg's `Settled` envelope from its **separately pinned settlement attestor**, under the manifest's exact finality policy | Finality evidence content, jurisdiction/provider conditions, stays/disputes, exact beneficiary/amount, original attempt, all required legs and no conflicting evidence. A coordinator has no settlement-attestor power unless separately mandated, which this profile does not grant |
| `Held(lastProven, reason)` | Local fail-closed status, optionally source-signed `Held` diagnostic | Preserve last independently proven evidence, reservations, IDs and uncertainty. A diagnostic confers no authority to release, retry or advance |
| `Aborted` | Coordinator `AbortDecision`, and both authorities' `AbortAck` envelopes for complete business cancellation | Irrevocable abort tombstone, exact reservation binding, no commit or execution permitted for this attempt. Partial ACKs remain `Held` for business cancellation |

Preparation abort is legal **only before the terminal commit decision**. There
is no `Committed → Aborted` or `Executed → Aborted` transition. A dispute or
unknown execution after commit produces `Held(lastProven, reason)`; refunds,
replacement payments and compensation are **new**, funded, dual-signed
obligations linked to the original, never rewritten history (G4 remains open).
A settlement receipt is historical evidence: later disputes suspend new actions
and preserve evidence rather than erase it. `Held` is an overlay, not a
backward change to the durable terminal decision.

| Consuming workflow/action | Minimum independently verified state |
| --- | --- |
| Any workflow: record proposal/request a quote | `Proposed`; grants and consent still required |
| Checkout: reserve a cancelable order slot | `Committed(intent)`; no goods release or paid status |
| Checkout: release goods, fulfill an order, mark invoice paid | `Settled` for all contracted payment legs |
| Dispatch: mark fee paid/complete the paid-service release | `Settled`; starting service on business credit requires a separately authorized new credit obligation |
| Group buy: display/record conditional pledge | `Proposed`; this does not certify funded escrow |
| Group buy: release escrow or initiate irrevocable fulfillment | `Settled` plus independently verified group threshold, release condition and grants |
| Remittance: display an execution observation | `Executed(observed)`, explicitly labeled observation |
| Remittance: tell the recipient funds were delivered/complete remittance | `Settled` under the destination's pinned finality policy |

`Held`, missing legs, stale grants/epochs, invalid evidence and contradictions
block every advancing action. No workflow may lower its threshold merely to
handle timeout. This version's irreversible actions deliberately require
`Settled`; alternative credit-risk policies require separate review/proof.
There is no live custody/finality profile or attestor authorization here, so
this specification does not enable any real workflow to report `Settled`.

## 2. Exact bytes: manifest and signed evidence

All concatenations below are byte concatenations. No JSON, CBOR, Unicode
normalization, decimal parsing or host-native integer representation occurs in
a signing input. `B32` means exactly 32 bytes, `U64` an unsigned eight-byte
big-endian integer, `U16` an unsigned two-byte big-endian integer. `H` is
SHA-256. An ID is an opaque B32, not a human-presented participant identifier.
QR remains the sole presented participant identity. There are no optional
fields, trailing bytes, implicit zeros or alternate encodings.

Amounts and fees are `U64` representations restricted to **0..2^63−1** minor
units, identically in wire and biz-kit ledger schema. Signs belong to typed
posting direction, never the amount. Each currency's scale is fixed by the
accepted terms/custody profile, not inferred from a currency string. Each
computed sum/product must be range-checked before admission; this increment
chooses the encoding but does not prove accounting arithmetic or close D5.3.
Currency is exactly three uppercase ASCII bytes; `TST` vectors are synthetic.

Manifest bytes, in order:

```text
"hawala-cross-authority-manifest/v1\0"
network_id:B32 business_id:B32 business_revision:U64 attempt_id:B32
accepted_terms_digest:B32 participant_consent_digest:B32
permitted_disclosure_policy_digest:B32 finality_policy_digest:B32
recovery_mandate_digest:B32 coordinator_key:B32 coordinator_epoch:U64
participant_count:U16 (=2)
[ authority_key:B32 scope_id:B32 authority_epoch:U64 allocation_id:B32
  obligation_digest:B32 funding_lot_id:B32 payee_key:B32 currency:3 ASCII
  amount:U64 fee:U64 destination_digest:B32 custody_profile_digest:B32 ] × 2
latest_prepare_time:U64
```

Legs are strictly sorted by unsigned lexicographic
`authority_key || scope_id`, with **distinct authority keys**, distinct
exclusive allocation IDs and explicit source attribution. The manifest digest
is `H(manifest_bytes)` and is immutable across retries and recovery. Business
ID/revision, network, attempt and financial terms cannot be substituted.
Accepted terms/consent digests reference the original retrievable dual-signed
Hawala terms/QR bytes; there is no self-reference to the manifest's proposal
signatures. Each authority independently verifies that those terms authorize
exactly its manifest leg, fees, funding, allocation and destination. The prepare deadline is an integer count of UTC seconds since the Unix
epoch (leap seconds ignored), checked against each authority's trusted clock
and uncertainty policy before admission. The
prepare deadline applies only before reservation; **prepared reservations do
not expire**. Expiry is never an abort fence.

For every evidence record, the exact Ed25519 signing input is:

```text
"hawala-cross-authority-evidence/v1\0" kind:U8 manifest_digest:B32
issuer_key:B32 signer_epoch:U64 decision_coordinator_epoch:U64 serial:U64
payload:(fixed fields selected by kind below)
```

The transmitted envelope is `signing_input || signature:64 bytes`.
`evidence_id = H(envelope)`. The signature covers **all** input bytes. The
kind is part of the signing domain; unknown kinds fail closed. `serial` is a
durable per-issuer/epoch record ID, never a clock or authority grant. An exact
replay returns the original evidence/result. A changed body under an existing
issuer/epoch/serial conflicts; the receiver retains the original and `HOLD`s.
Receivers also index the immutable manifest/attempt, so a new serial cannot
reopen a terminal attempt or consume its QR nonce twice. Replay storage and
atomic append integration are requirements, **not implemented by the codec**.

All payload fields are B32 unless marked U64. Referenced evidence is retained
verbatim and is independently retrievable; a digest without its evidence is
insufficient for advancing a workflow.

| Kind byte/name | Payload fields, in order | Permitted signer under pinned mandate |
| --- | --- | --- |
| 1 `Proposed` | empty | Each consenting participant device; signer epoch 0 in this profile |
| 2 `PrepareRequest` | recovery mandate digest | Decision coordinator |
| 3 `Prepared` | reservation evidence digest | Scoped authority for exactly one leg |
| 4 `NoVote` | refusal reason digest | Scoped authority; durable refusal before prepare |
| 5 `CommitDecision` | prepared-envelope set digest | Scoped decision coordinator |
| 6 `AbortDecision` | abort reason digest | Scoped decision coordinator |
| 7 `CommitAck` | commit-decision ID, local posting digest, reservation evidence digest | Authority for that leg, only after its durable financial append |
| 8 `ExecutionObserved` | commit-ACK ID, external execution ID, observation evidence digest | Separately pinned execution observer for that leg |
| 9 `Settled` | commit-ACK ID, observation-envelope ID, finality policy digest, finality evidence digest | Separately pinned settlement attestor for that leg |
| 10 `Held` | last evidence ID (zero if none), diagnostic reason digest | Attributable coordinator/authority/observer only within its role; unsigned local HOLD is also permitted and never authority |
| 11 `FenceAck` | recovery mandate digest, durable history digest, successor epoch:U64 | Each participant authority and the actual old-coordinator signing boundary's independently pinned fence operator |
| 12 `Recovery` | recovery mandate digest, fence-ACK set digest, complete history digest, prior decision ID | Named recovery resolver and successor coordinator, with all participants' acceptance required |
| 13 `AbortAck` | abort-decision ID, reservation evidence digest (zero if never prepared) | Each participant authority after durable abort consumption/release fence |
| 14 `MandateAcceptance` | recovery mandate digest | Coordinator, both authorities, named recovery resolver and boundary fence operator, separately signed |
| 15 `RecoveryLog` | complete history digest, prior decision ID, source checkpoint digest | Pinned durable decision-log recovery authority |

For an evidence set: sort evidence IDs strictly by unsigned byte order, reject
duplicates, then compute
`H("hawala-cross-authority-evidence-set/v1\0" || count:U16 || IDs)`.
A commit requires the **two distinct, complete** prepared envelopes for the
same manifest, not two signatures by one authority. Each reservation evidence
must establish exclusive durable coverage under the accepted custody policy.
For an abort, no prepare-set digest is required: the irrevocable decision
fences late prepares, whose reservations may release only against that abort.

The recovery mandate's canonical body is:

```text
"hawala-cross-authority-recovery-mandate/v1\0"
network_id:B32 coordinator_key:B32 coordinator_epoch:U64
successor_key:B32 successor_epoch:U64 recovery_resolver_key:B32
boundary_fence_operator_key:B32 log_recovery_authority_key:B32
participant_count:U16 (=2)
[ authority_key:B32 scope_id:B32 authority_epoch:U64 ] × 2 (same order as manifest)
alert_after_seconds:U64 (=300) escalate_after_seconds:U64 (=3600)
```

The successor epoch is exactly predecessor+1 (checked without overflow).
The manifest pins this body's digest; kind 14 acceptances bind it to this
attempt. One role does not inherit another role's signing powers. Zero prior
decision ID in recovery means *certified absence in complete, fenced durable
history*, not failure to find a receipt. A log checkpoint is only acceptable
under independently established anti-rollback/continuity policy; a self-issued
claim of completeness cannot prove absence. The deployment must name and pin
all these operators; none is provided by this increment.

## 3. Scoped coordinator and blocking two-phase decision

The coordinator's mandate permits ordering **one commit or abort for the named
manifest and scopes**, requesting prepares and publishing terminal evidence.
It confers no custody, authority over other portfolios, participant consent,
execution authority or settlement power. It may be a third Hawala peer; it is
not a required global operator, registry or ActivityPub server.

1. Each authority verifies the full manifest, device consent, current grants,
   custody mandate, terms, amount, attempt/nonce and epoch before acting. The
   coordinator sends signed `PrepareRequest`. Unavailable information means
   `HOLD`, not assumed success.
2. Before prepare, an authority may append a permanent `NoVote` tombstone.
   It cannot later prepare that same attempt. This is a **local refusal**, not
   a global final abort receipt and not evidence about the other authority.
3. To vote yes, each authority durably and atomically reserves its exact
   allocation, pins the manifest and coordinator mandate, and persists its
   `Prepared` record/outbox before publishing it. Any previously consumed
   refusal/terminal record fences that transition. No execution at prepare.
4. The coordinator independently verifies both prepares. It may append a
   terminal commit only with both; it may append terminal abort before commit.
   The decision and its publication outbox share one non-rollback durable
   append. This is **one coordinator decision order**, not one shared
   financial ledger or a transaction spanning the two authorities.
5. Participants verify the signed decision and prerequisites themselves. A
   commit locally appends the authorized intent/postings, consumes the attempt
   and QR nonce, and persists its ACK/outbox atomically. The other authority's
   ACK is never assumed. A missing local admission prerequisite after prepare
   yields `HOLD` and resolution, **never an opposing abort decision**. Production
   refinement must show prepare guarantees commit-admissibility despite policy
   changes; that proof is still open. The formal model abstracts ACK processing
   and does not prove the financial append.
6. Abort is consumed durably before release; late confirmations remain bound
   to the same abort. Prepared authorities cannot unilaterally abort or release
   on timeout, lost connection, coordinator crash or cached decision absence.
   Committed allocations remain encumbered until a separately authorized
   discharge protocol applies. Kind 13 cannot undo kind 5.
7. Delivery is asynchronous and may duplicate, reorder, be delayed or be lost.
   Query original IDs; resend **the same** durable records. Unknown financial
   outcome forbids a replacement execution attempt. Never create an automatic
   new attempt to conceal uncertainty. Execution requires the separately
   authorized existing commit-before-execute boundary, which this model does
   not implement.

A participant may know commit while its peer still holds. Agreement means
**no two terminal decisions conflict**, not simultaneous knowledge, simultaneous
postings or guaranteed delivery. Coordinator signatures rely on its pinned
non-equivocating durable order; Byzantine equivocation/key theft, rollback,
cryptographic failure and untrusted storage are outside the proof. A future
replicated coordinator requires a separate consensus/fault-model refinement.

## 4. Epochs, crash recovery and mandatory HOLD

Authority epochs, coordinator epochs, device identity and business grant
epochs are separate. Admission uses exact pinned authority epoch and effective
coordinator epoch. Old live prepare/execute commands are fenced at the **actual
signing, financial append and custody boundaries**, not just a UI epoch field.
Historical terminal evidence remains evidence for its original manifest under
the authenticated predecessor chain; numeric `epoch <= current` alone is not
an authorization rule. An old terminal record cannot authorize a fresh command.

Ordinary restart restores the same complete durable log and outbox before
publishing decisions; it never initializes an empty replacement log. If this
cannot be established, remain held. Participant restart preserves prepare,
refusal, decision, nonce and reservation records; volatile connectivity or
process liveness does not clear them.

Coordinator succession has an explicit mandate and is deliberately blocking:

1. Obtain an effective old-coordinator signing-boundary `FenceAck`, including
   durable history/checkpoint. Suspecting it failed is not fencing it.
2. Obtain both participant-authority fence ACKs. Each stops old-epoch live
   commands at its actual financial/custody boundaries, retains reservations
   and supplies its durable vote/decision history. Missing ACK means `HOLD`.
3. The named log recovery authority supplies a `RecoveryLog` covering the
   **complete** durable order, including any unacknowledged terminal decision.
   The named resolver reconciles it with both participant histories. Conflicts,
   rollback doubt or an unavailable decision source mean `HOLD` indefinitely.
4. The resolver and successor sign the same `Recovery`; all participants
   explicitly accept it under the pinned mandate. Install epoch+1 durably at
   every relevant boundary before new live commands. Preserve all original
   attempt IDs, prepares, decisions, envelopes, checkpoints, outboxes and
   terminal tombstones. Existing commit/abort is immutable. If certified complete
   history is undecided, the successor may decide under the same rule (commit
   only with both valid prepares), or abort; it cannot infer a prior abort.
5. Recover/publicize the original terminal record or append the sole permitted
   decision, then deliver it idempotently. An old delayed decision that conflicts
   with recovered history proves a violated assumption: retain evidence and
   hold; never release funds on either conflicting statement.

The recovery resolver is the key/operator named in the accepted mandate, not
whichever host is currently reachable. This proposed profile alerts the scoped
operators after **300 seconds**, escalates to that resolver after **3600
seconds**, and retains audit evidence permanently. These are operational
response deadlines, **not promised resolution times or lock expiries**. There
is no finite terminal-resolution bound during an unbounded partition or lost
history. After all messages, fences and history are available, the specified
finite recovery/decision sequence can complete under fair scheduling; no
liveness/fairness theorem is claimed here. Refuse enrollment if the deployment
cannot accept indefinite HOLD or provide the named mandates. No arbitrary
administrator release or backup rollback can substitute for recovery evidence.

Authority succession inside an outstanding attempt, participant-set changes,
more than one modeled coordinator succession and real signing-boundary fences
are future refinements. The current model keeps
both authority epochs fixed and rejects mismatches. A new authority epoch does
not silently rebind an outstanding reservation; it needs its own explicit,
verified continuation protocol, otherwise `HOLD`.

## 5. Formal increment and independently reproducible checks

`CrossAuthority.lean` proves invariant preservation and agreement for arbitrary
finite reachable event sequences, durable terminal permanence, both prepares
before commit, refusal after prepare rejected, HOLD retaining the abstract
lock, wrong-attempt/authority-epoch rejection, old-coordinator prepare rejection,
incomplete recovery stuttering, and recovery retaining votes/terminal history.
`FinancialEvidence.lean` proves that unverified/held evidence cannot advance a
workflow and that irreversible actions require the settled evidence shape.
No proof uses `sorry` or a custom axiom; standard Lean logical axioms are
reported by `#print axioms`.

`crossAuthority.qnt` independently models two durable participant votes and
terminal observations, prepare requests/votes, coordinator decision publication,
duplicate/foreign-context messages, independent crashes/restarts, two partitioned
links and acknowledged recovery fences. The authenticated-message abstraction
only permits current-context terminal evidence from the published decision log;
valid foreign evidence exercises context/epoch rejection. Signature truth and
real boundary fencing are assumptions, not transitions inferred from silence.
Its TLC safety/agreement check exhausts **115,360 states** (depth 27) for fixed
authority epochs and coordinator epochs 0 and 1. This is exhaustive within that
finite model, not a proof for arbitrary deployments or Byzantine faults.

Five deliberately unsafe extensions must yield actual TLC invariant violations.
Parse/type/download/backend failures **fail** the gate:

| Negative control | Retained counterexample demonstrates |
| --- | --- |
| `unsafeTimeoutStep` | A prepared participant treats timeout as abort/unlock while no global abort exists |
| `unsafeSingleStep` | Coordinator commits after only one received prepare |
| `unsafeRecoveryStep` | Recovery erases a terminal log while a participant retains its decision |
| `unsafeReplayStep` | A valid prior-attempt commit is mis-bound to this attempt |
| `unsafeZombieStep` | An unfenced predecessor publishes commit against a durable abort |

Actual TLC state traces are retained in
`formal/traces/crossAuthority-negative.json`. A separate checker verifies safe
prefixes, the violated invariant and the intended final mutation. These traces
are not hypothetical exploit claims about deployed software.

`ExportCrossAuthority.lean` generates **232 selected prefix witnesses** across
13 named failure/recovery scenarios, exported as JSON and independently replayed
by Quint against every expected full state. CI regenerates and diffs both
exports. This is selected Lean/Quint conformance, **not** complete graph-edge
coverage, universal semantic equivalence or Rust/Elixir host refinement. The
canonical JSON is the seed for future Rust/Elixir conformance, not evidence
that those hosts have already passed it.

The read-only Node codec and **20 deterministic signed TEST vectors** cover all
15 record kinds, fixed bytes, Ed25519 verification against pinned expected
issuer/role, manifest/epoch/domain/kind substitution, truncation/trailing bytes,
corrupted signatures, duplicate evidence, incomplete intent graphs and both
money boundaries. Test keys and synthetic finality evidence have no mandate.
The codec is **not** a production verifier: it does not implement durable replay
consumption, custody proof verification, finality policy evaluation or live ports.
The Lean and Quint models are not cryptographic or accounting proofs.

Reproduce with the repository's pinned Lean 4.28.0, Quint 0.32.0, Java 17 and
Node 24:

```sh
npm ci --prefix formal --ignore-scripts
(cd formal/lean && lake build)
bash tools/check_trace_artifacts.sh
bash tools/check_cross_authority.sh
```

All existing formal checks remain mandatory. This change adds no live-money
mutation endpoints, new custody privileges, Iroh protocol activation or checkout
orchestration. G1/G2 now have a reviewable specification candidate; G3 remains
open until formal review and actual host conformance, G4–G8 remain separate,
G9 retains its existing status, and G10 still needs arithmetic/host refinement.
G11's identical normative text is reviewable in the paired PRs; it closes only
when accepted in both. The architecture remains **Proposed**.
