# M6-C3 — custody approval policy simulator (not real custody)

**Research/conformance only. No source of customer money or asset control.**

`hawala-custody-model` implements two **independent selection modes** under
one typed request/signature-verification contract:

- **Independent escrow**: a named controller authorized by an externally
  established custody mandate signs the exact request.
- **Federated dar custody**: an explicit roster of controller public keys,
  threshold q, and claimed maximum Byzantine faults f sign identical bytes.

All commands require account, depositor dar, currency, request ID,
allocation/obligation binding, exact minor-unit amount, expected state
version, policy epoch, mandate digest and policy digest. Signatures use
domain-separated, length-prefixed bytes. The policy digest is
order-invariant over the enrolled roster, not derived from user-provided
untrusted controller gossip.

## Finite quorum intersection

For n enrolled controllers, with a declared maximum f Byzantine members,
this model requires:

- n ≥ 3f + 1;
- q > f;
- 2q > n + f.

The last inequality means any two q-subsets intersect in MORE than f
members, so at least one intersection member must be honest **assuming**
the fault model and truly independent keys hold. Two explicit examples:

- n=4, f=1, q=3;
- n=8, f=2, q=6.

These are conservative reference profiles, **not a chosen live federation
constitution**. They assume honest controllers refuse to sign conflicting
state transitions, which this library does not enforce across machines.
Even valid quorums do not prove legally effective custody.

## Simulation decisions

`examine` accepts an independently pinned policy and exact request.
It rejects unregistered/duplicate/forged signatures, changed commands,
amounts, versions, epoch, mandate or controller roster.

It returns either:

- `Hold{got,needed}`: insufficient signers, including during network
  partition. **No funds or collateral may be released** on this result.
- `SufficientSignatures(SignerEvidence)`: threshold evidence under
  the configured hypothetical policy. **Not** `Committed`, not a
  verified escrow asset, not a payout or release authorization.

For escrow the threshold is exactly its registered one controller; the
controller cannot be the depositing dar in the initial conservative model.
For federation, the depositor dar is also excluded from the registered
controller roster. More complex conflict-of-interest allowances require
a separately audited authority constitution.

## What is still missing

- Actual custody account / legal ownership / mandate / liens / solvency.
- Membership vetting, independent operator control and authenticated
  federation rosters; signed declarations alone can be self-dealing.
- One durable ordered state machine per asset/credit allocation; signing
  requests is **not consensus**, not actual release, and not bank finality.
- Correct recovery, quorum epoch changes, anti-equivocation storage, and
  hardware withdrawal execution fencing.
- Connection between the approval simulator, real control, and the M6-C2
  synthetic accounting journal. No economic action is authorized here.

## Tests

Escrow pins one signer and exact command. Four-controller and eight-controller
simulations hold below threshold, validate exact approval counts and
reject unsafe fault/quorum profiles. Wrong signer, repeated signer, altered
currency, mandate, command, state version and stale epochs all fail closed.

Next: implement a two-peer/federated offline stress harness with durable
controller equivocation detection, epoch-fenced authority handoff, and
externally witnessed escrow assets before wiring real funds.
