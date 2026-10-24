# ATM-first provider event simulator (M5-C)

This is a deterministic **mock**, NOT a driver for XFS4IoT or any deployed ATM.
The purpose is to run P0 Hawala crash/partition/reconciliation tests before
any real equipment or customer funds are connected.

`hawala_atm::simulator::simulate` takes a trusted terminal identity key,
a payout attempt context and normalized dispenser events. It derives a
domain-separated digest of the event sequence and issues a signed immutable
`TerminalObservation`. It does not trigger hardware, issue QR withdrawal
tokens, charge accounts, release collateral or initiate a payout.

Normalized events: transported amount in minor units, customer access opened,
hardware items-taken sensor event, retracted amount, finished and
indeterminate (power cut/jam/timeout). The result is classified using the
trusted capabilities, not a claim embedded by the customer or a P2P peer.

The tests exercise: full observed cash collection, lack of sensor, jam,
partial movement, retract, no movement, malformed sequence, arithmetic
overflow, wrong signer, changed event order and distinct attempt identities.

## Actual provider adapters required

**Direct ATM / recycler operators:** Implement a separate, privileged
XFS4IoT-compatible gateway with transport authentication to the device and
verified ownership of the terminal's device-signing key. Normalize actual
unit/denomination reports with independently validated currency and cassette
configuration. Require customer-access, items-taken and retract telemetry
only when the device actually supports those sensors. Deposit, cash-in
rollback and recycler cassette counts must be a different, audited workflow.

**Existing ATM networks:** Negotiate a network operator's approved cardless
API and status reconciliation. NCR Atleos ReadyCode is an example of a
cardless-network offering but does not establish native QR-only support or
direct device telemetry for this project. Network passcodes must remain an
internal provider integration detail, not an alternative identity identifier.

## Mandatory safe ordering

1. QR/name/code plus mutual Iroh peer verification.
2. Authenticator-bound, temporary signed transaction authorization.
3. Independently enforced financial eligibility, available collateral,
   bilateral trust, funds reservation and regulatory controls.
4. Commit exactly one `begin_atm_attempt` with pinned terminal/attempt ID.
5. Only the regulated, separately authenticated operator sends the actual
   hardware dispense command. The core itself has no such API.
6. Gateway obtains genuine device events, signs an observation and submits
   it to the durable journal.
7. Journal records an outcome **as uncertain for execution purposes**,
   even if device evidence claims full delivery or no delivery. A distinct
   beneficiary receipt, cash inventory and external settlement verification
   are required for financial finality or rollback.

No self-reported transaction success, retry, or cash ledger credit is safe
on the basis of this simulator.
