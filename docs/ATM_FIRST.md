# Automated tellers first — Hawala cash access contract

**Priority decision (2026-10-24):** Automated tellers and cash recyclers
are the first HARDWARE INTEGRATION targets, not exclusive peers. The
universal Hawala network supports human↔human, human↔machine and
machine↔machine from the beginning; see [PEER_MODES.md](PEER_MODES.md).
The economic core remains **Hawala over Iroh**.

## Participant model

- The **individual** scans the ATM's public identity QR and compares the
  readable machine code/name. The terminal's gateway similarly verifies the
  individual's QR-presented full key using a time-limited, bound challenge.
- The **ATM** has a persistent Hawala device identity and a separate operator
  identity. Device-signing keys are provisioned by the verified operator,
  ideally within tamper-resistant hardware. Operator trust and device
  capabilities are governed independently of the identity protocol.
- The **origin and disbursing hawaladars** agree on the obligation and
  payout currency/amount beforehand. One operator may act as a hawaladar
  only when independently authorized. Signing a device event does not
  confer money-transmitter status.
- **Iroh** carries peer authentication and authorization/evidence exchanges,
  not ATM cash cassette commands directly from arbitrary network peers.

## Withdrawal sequence (design; real hardware not implemented)

1. Verify both Hawaladars' accepted terms, available funds/credit,
   jurisdiction, KYC/AML/sanctions controls, and terminal/operator rights.
2. Customer scans ATM QR plus its displayed name/code. Device verifies
   customer QR. Generate an authenticator-bound, temporary authorization
   that signs exact amount, currency, beneficiary, disburser, terminal
   identity, unique attempt ID, fee and expiry. Static QR is never a bearer
   cash-out token.
3. Atomically reserve funds/obligations **before** any device command.
   The durable journal binds a single trusted terminal and attempt ID
   while transitioning registered -> dispensing. No second attempt is
   possible by calling `begin_atm_attempt` on the same record.
4. Only a separately authorized operator gateway issues the vendor-specific
   dispense command, and it must use the same idempotency/attempt ID where
   the hardware supports it.
5. Collect actual device telemetry: cash transported, presented, items-taken
   events (only if a certified sensor is present), retracts, errors,
   partial outcomes, and terminal logs. The trusted gateway signs a
   `TerminalObservation` bound to the terms digest, attempt and device.
6. `record_terminal_report` checks the signature and exact durable binding,
   records one outcome, and keeps the journal in UNCERTAIN. Even a signed
   `ReportedDelivered` is a device/operator claim, not beneficiary
   acknowledgment, external legal finality, or permission to credit/retry.
7. Distinct beneficiary/disburser attestations, device-log reconciliation,
   and external custody evidence are needed to resolve final disposition.
   Contradictory outcome reports are preserved as disputes, not overwritten.

## Outcome model

- **ReportedDelivered**: a trusted operator *reports* the full amount
  moved to an accessible position and a supported items-taken sensor
  observed removal, with no retract. This is not indisputable cash proof.
- **ReportedNotDelivered**: the operator *reports* zero cash transported
  to output and no customer access. It still does not automatically unlock
  a retry, reversal or transfer of liability.
- **Uncertain**: includes partial dispenses, sensor absence, customer
  access followed by retract, transport jams, timeouts, incomplete logs,
  a compromised operator, or contradictory events.

Hardware trust and independent reconciliation remain required. A gateway
signing a claim is not proof its underlying hardware events are accurate.

## Terminal capability contract

`Capabilities` explicitly records dispensing, depositing, recycling
and items-taken sensor support. A recycler must both deposit and dispense.
Capabilities and pinned terminal identities MUST come from trusted
operator policy, not a QR label or self-declared P2P packet.

XFS4IoT defines separate cash-dispenser, cash-acceptor and cash-management
commands/events. In particular `CashManagement.ItemsTakenEvent` is
conditional on items-taken sensor support. We map those events in a future
operator gateway, not in generic Hawala financial code.

References:
- https://xfs4iot.github.io/Specifications-Preview.github.io/html/index.html
- https://www.ncratleos.com/banking/atm-network-allpoint/readycode

**Existing-network APIs** such as ReadyCode may provide cardless access,
but are not assumed to support our native QR or terminal-signed telemetry.
Their platform-specific authorization must be bridged by an approved
provider, with reconciliation and commercial/regulated agreements.
Never invent direct XFS4IoT access to third-party network ATMs.

## Deposit and cash recycling (next milestone)

Deposit and recycle have different safety constraints: notes must be
recognized and accepted, anti-counterfeit result must be settled, customer
has a right to reject/reclaim before acceptance, and recycler inventory
must be independently reconcilable. A tentative credit is NOT universally
redeemable until committed collateral/custody and acceptance conditions
are satisfied. Device deposit capability does not imply permission to
create Hawala wallet credits.

## Production blockers

No released hardware adapter, no operator provisioning or cash keys,
no financial release engine, no transaction-bound authenticator QR,
no independent final-outcome adjudication, no trusted external custody
provider, no double-entry cash inventory, no multidevice consensus,
no legal clearance and no production security audit.

**Deliver next:** mocked XFS4IoT cash events and provider simulator with
failure/partial/retract recovery tests, followed by real provider sandbox
and independently verified custody/reservation integration.
