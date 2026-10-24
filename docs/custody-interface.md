# Custody interface v0.1

Status: proposed normative interface, supporting BOTH independent escrow and
federated dar custody. The user selected both modes on 2026-10-24. No provider,
asset, threshold, adjudicator or live custody arrangement has been selected.
This interface is a protocol contract; no live custody adapter is implemented.

## 1. Common boundary

Hawala asks a custody adapter to attest assets, reserve capacity, release
discharged security, withdraw free assets and execute an authorized award.
The adapter returns authenticated, durable results under its custody mandate.
An adapter cannot approve a customer transfer, decide a dispute or revoke an
identity merely because it controls collateral. The Hawala ledger cannot
invent a deposit merely because an adapter acknowledges a request.

Each account binds:

- network ID, interface version, adapter ID and mode;
- immutable custody-account ID, depositing dar, asset/currency and asset-lot
  references, with independent evidence of control and existing encumbrances;
- custody agreement digest, release/withdrawal mandate and award-execution mandate;
- current authority-policy digest and monotonically increasing authority epoch;
- monotonically increasing state version and authenticated ledger/checkpoint link;
- segregation, reconciliation, external lien and insolvency arrangements;
- service/finality limits, evidence retention and available query/recovery paths.

Different adapters can coexist in one deployment. Each collateral allocation
references exactly one account and authority epoch; neither mode is a fallback
authorization for the other. Modes do not create different customer currencies
or automatically change the original dars' liabilities.

## 2. Authority profiles

| Requirement | Independent escrow | Federated dar custody |
| --- | --- | --- |
| Asset control | Named escrow custodian under custody agreement | Named jointly controlled custody arrangement with documented actual asset control |
| Command approval | Custodian's authenticated authority under its mandate | Exact approval rule over independently enrolled controllers in the pinned authority policy |
| Independence | Disclose ownership/conflicts and reliance on custodian | Disclose common control, collusion assumptions and which controllers may be the depositing dar |
| Ordering | Durable serialized state and fenced external executions | Agreement protocol enforcing one durable state/order despite its stated fault model |
| Recovery | Mandated custodian succession with retained locks | Authorized membership/roster succession with retained locks and fenced old execution authority |

No universal quorum is prescribed. A federation MUST specify exact controller
keys, eligibility, approval rule, fault assumptions, quorum intersection or other
equivalent safety argument, membership succession and partition behavior.
Several signatures alone are neither consensus nor proof of independence.
Controller signatures MUST cover identical command bytes, expected state,
authority epoch and policy digest. A controller attestation does not establish
that a bank or vault will obey the federation; prove the actual custody mandate.

## 3. Command envelope and receipts

Every command binds interface version, network ID, adapter/account ID, command
kind, unique request ID, complete payload digest, depositing dar, currency,
expected account state version, authority epoch/policy digest, mandate digest,
authorized requester identity, audience and a validity interval where relevant.
Signatures use the existing payment protocol's canonical, domain-separated
encoding. No ordinary JSON is signed. Request IDs are permanent per account;
expiry never permits their reuse. Asset amounts use exact minor units.

A receipt binds all those fields, a result, pre/post state versions, allocation
changes, asset/execution evidence references, external execution ID where
applicable, and the required custodian/controller approvals. Hashes reference
retrievable authenticated evidence, not arbitrary self-asserted text. A successful
signature check alone does not establish authority: verify the independently
anchored mandate, epoch, state continuity and mode-specific approval policy.

| Result | Meaning | Caller action |
| --- | --- | --- |
| PENDING | Accepted for processing, not final | Do not count new security or release assets |
| CONFIRMED | Durable final result under the pinned profile, with required evidence | Apply once, subject to cross-ledger commitment rules |
| REJECTED | Terminal rejection; certifies no effect and fences execution of that exact request | Retain rejection and its ID; do not retry with changed payload |
| UNKNOWN | Timeout, missing evidence, stale view or indeterminate external effect | Query by original request ID; HOLD affected capacity |

Query operations can return PENDING/UNKNOWN without certifying a financial
state change. Timeouts and HTTP/transport success are not finality. REJECTED is
invalid if the adapter cannot establish no effect; it must return UNKNOWN.
Identical retries return the same terminal receipt (or current pending status).
The same ID with a different payload is a conflict, not a new command.

## 4. Operations

| Operation | Required evidence / conditions | Confirmed effect |
| --- | --- | --- |
| AttestDeposit | Verified asset/control, reconciliation, mandate, liens and account identity | Evidence of eligible assets at a particular version; does not alone reserve capacity |
| QueryAccount / QueryRequest | Authenticated requester and permitted disclosure | Signed state/status with checkpoint; not a new approval |
| Reserve | Obligation/allocation ID, amount, liability agreement and ledger prepare reference; sufficient free capacity | Capacity encumbered under one serialized state transition |
| ReleaseDischarged | Allocation ID, amount, verified discharge or authorized replacement evidence, ledger commitment link | Only the covered discharged amount becomes free |
| WithdrawFree | Owner-approved destination, unique execution ID, amount and final verified free capacity | Assets leave custody and eligible amount decreases; reserve and withdrawal cannot both spend the same capacity |
| EncumberDispute | Case/allocation reference, authority and maximum affected amount | Existing security is retained/marked disputed; no asset seizure or duplicate reservation |
| ExecuteAward | Final/stay-cleared award, exact claimant/destination/currency, remaining claim and recovery ID | Authorized recovery, eligible assets and covered liability updated once for actual execution |
| MigrateAuthority | Authorized governance evidence, new policy/epoch, complete outstanding state and execution-fencing evidence | New authority controls the SAME obligations and account history |

Partial release/recovery MUST identify exact remaining allocation/claim amounts.
Multiple awards and adapters MUST reconcile cumulative recoveries against the
same claim so that compensation cannot be duplicated. Custody fees and actual
enforcement costs are separately agreed; neither adapter may improvise a
percentage forfeiture or interest increment.

## 5. Ledger/custody atomicity and uncertainty

Physical assets and two independent ledgers do not become atomic through a
receipt. Reserve before making new credit spendable; reference its confirmed
allocation in the financial commit. Both dar allocations and the financial
commit need an explicitly defined common transaction/coordination profile.

If custody reserves but the financial commit is uncertain, retain the lock.
Release requires an authenticated final abort/noncommit certificate that fences
any delayed financial commit, or evidence of discharge/replacement after commit.
A reservation lease MUST NOT expire and free assets while a delayed commitment
can still finalize. Missing messages or local timeout are not abort certificates.
If that fencing cannot be established, HOLD rather than freeing the allocation.

For WithdrawFree and ExecuteAward, the external custody rail must enforce
one-time execution or provide an authoritative reconciliation path. A repeated
network request cannot instruct another bank transfer. Indeterminate external
execution keeps the funds encumbered; no second payout or restored free capacity
until reconciled. An adapter that cannot provide these guarantees cannot
advertise support for the operation or secure immediately available credits.

Account snapshots cannot be reused as spending authorizations. Every mutation
uses expected state version and current epoch, with durable replay records and
restart recovery. Stale version or old-epoch commands fail without effect.
Offline peers can inspect historical receipts but cannot establish current
capacity, withdraw collateral or finalize new secured exposure.

## 6. Migration, failure and exit

Changing controllers, custodian or adapter requires an authorized migration
record and continuity of every active reservation, dispute and pending external
execution. Never initialize a replacement adapter with an apparently free
copy of previously pledged assets. Old execution authorities must be fenced at
the asset-control layer, not merely removed from a local configuration file.

For migration between custody accounts/providers: old assets remain encumbered
until verified replacement security is committed with legally effective
assignment/release. Both accounts remain visible during transition; do not count
both as free collateral or release the old security on a mere destination
receipt. Shared custody can migrate to independent escrow and vice versa only
under these same rules. Identity succession alone authorizes neither migration.

Custodian/federation failure suspends new exposure on affected accounts. It does
not extinguish customers' claims. Custodian insolvency, controller collusion,
asset fraud and legal unenforceability remain risks, not problems solved by
interface uniformity. Each profile must publish the applicable recourse.

## 7. Deterministic adapter conformance scenarios

These are required adapter integration vectors, not claims of implemented tests.

| ID | Input sequence | Expected outcome |
| --- | --- | --- |
| CU01 | Eligible 100 at v7; Reserve 60 with expected v7 | CONFIRMED v8; reserved 60, free 40 |
| CU02 | Retry CU01 ID and identical payload | Original confirmed receipt; state stays v8 |
| CU03 | Retry CU01 ID with amount 61 | Conflict; unchanged assets/capacity |
| CU04 | Reserve 60 and WithdrawFree 60 both expect v7 | At most one confirms; other stale/insufficient; no negative free capacity |
| CU05 | Reserve confirms; financial commit reply lost | Allocation stays encumbered until final status/fenced abort is established |
| CU06 | Withdraw request times out after possible external execution | UNKNOWN; no duplicate instruction; reconcile same execution ID |
| CU07 | Award 80; prior recovered 30 across adapters | Aggregate new recovery at most 50, regardless of receiving adapter |
| CU08 | Federation signs different command digests or fails its approval rule | No confirmed receipt |
| CU09 | Migrate v8 to epoch 4; replay epoch-3 release | Reject; existing allocations remain |
| CU10 | Replace adapter but omit active allocation from migration state | Reject migration; do not manufacture free capacity |
| CU11 | One asset is presented under two custody adapters | No independent double capacity; require asset reconciliation and enforceable exclusive encumbrance |
| CU12 | Correctly signed deposit attestation but no independently anchored mandate | No eligible collateral or reserve authority established |
| CU13 | Custodian proposes withdrawal solely from identity succession claim | No withdrawal authority |

Both modes MUST satisfy identical economic invariants; their proof of authority,
custody control and finality differ. The reference CollateralBook is only local
arithmetic and reservation logic, not either production adapter.
