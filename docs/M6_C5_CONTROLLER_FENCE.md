# M6-C5: Durable custody-controller anti-equivocation fence

**Reference/test-only — not money, custody, legal mandate, or distributed consensus.**

The existing `hawala-custody-model` verifies signatures for one independently
configured escrow or federated-custody request. It relies on an assumption
that each honest controller will **not sign two different transitions** for
one account state. That assumption needs durable implementation evidence.

`hawala-controller-fence` implements a conservative LOCAL instance of
that property: **one enrolled controller, one pinned custody policy/account,
one authority epoch, one externally verified account checkpoint/version,
one distinct signed request**.

## Protocol

A controller must independently receive an authoritative `CustodyPolicy`,
the private signing key assigned to that controller, and the authenticated
current custody account version. The library checks policy validity and
membership before opening its file. Policy digest, controller key, epoch and
account version are stored durably with WAL + synchronous FULL and an
exclusive sibling `.lock` file held for the lifetime of the signer.

`sign(request)` checks the complete request against the pinned policy,
including network/account/asset currency, depositing dar, mandate,
policy digest, epoch, expected version, command, exact amount,
obligation/allocation, and nonce/request ID. An `IMMEDIATE` SQLite
transaction persists request digest and Ed25519 signature **before**
returning the approval.

- First valid request: one durable approval is issued.
- Exact replay: the same stored signature is returned.
- A *different* valid request at the same version: rejected, never signed.
- Different account version, policy epoch, controller key or mandate:
  fail-closed. No implicit authority migration or reset.
- Storage/locking errors: fail-closed. There is no in-memory fallback.

The controller may **not advance** its signed checkpoint through this
interface. Production advancement requires authoritative custody proof of
the preceding committed transition, with epoch fencing, cross-controller
state replication and restoration/rollback protection; these capabilities
are deliberately missing.

## Finite federation argument, limited in scope

For an n-member controller roster with declared f Byzantine members,
the reference custody policy checks `n >= 3f+1`, `q > f`, and
`2q > n+f`. Under the strong assumptions that at most f members are
Byzantine and each other controller obeys a single non-equivocating
state machine, any two approving quorums share at least one honest signer.

Test profiles:

- Four controllers: f=1, q=3.
- Eight controllers: f=2, q=6.
- One independent escrow controller: the registered signer is separate
  from the depositing dar.

The local fence helps test ONE assumption. It does **not** prove
controller independence, actual fault bounds, network consensus, a
reliable broadcast implementation, legally enforceable custody, or
real banknote/escrow control. All signatures remain claims.

## Security limits

Two independent copies of the SAME private key with separately copied
SQLite databases can both sign conflicting operations. Rollback to an
old database backup can likewise defeat signing monotonicity. Hostile
storage admins and compromised signing keys remain out of scope.
Production needs hardware-backed monotonic state, attested checkpoints,
synchronized custody authority, key/epoch succession and a documented
fault model. This model must not be wired to customer money or external
ATM interfaces.

Tests cover identical replay and conflicting requests after restart,
concurrent signing threads, exclusive process authority, escrow signing,
4- and 8-controller finite quorum approvals, strict changes of epoch,
controller, mandate, request, account version and invalid fault profiles.

**Next engineering gate:** independently authenticated version-transition
receipts, durable controller checkpoint progression, forced process exit
and backup rollback fault injection, and live Iroh quorum/partition tests.
Do not claim a production settlement authority from signed requests alone.
