//! Deterministic normalized cash-terminal event simulator.
//!
//! This is NOT a hardware driver, XFS4IoT protocol implementation, cash
//! issuer, or production attestor. Its signed reports exercise safety paths
//! and provider adapter contracts with no cash connected.
use ed25519_dalek::SigningKey;
use hawala_identity_core::IdentityId;
use crate::{sign_observation, Capabilities, ItemsTaken, SignedTerminalObservation, TerminalObservation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalizedEvent {
    /// A provider maps actual denomination/unit counts into minor units.
    TransportedMinor(u64),
    /// Cash became accessible at an output position.
    CustomerAccessOpened,
    /// Only valid when operator-approved hardware has an items-taken sensor.
    ItemsTakenSensorFired,
    RetractedMinor(u64),
    /// All movement complete, no further events expected.
    Finished,
    /// Crash, jam, timeout, missing device receipt or unresolved interruption.
    Indeterminate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MockRequest {
    pub terminal: IdentityId,
    pub transfer_id: [u8; 32],
    pub terms_digest: [u8; 32],
    pub attempt_id: [u8; 32],
    pub payout_minor: u64,
    pub currency: [u8; 3],
    pub recorded_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimulationError {
    WrongSigner,
    InvalidRequest,
    ArithmeticOverflow,
    MalformedEventSequence,
}

impl MockRequest {
    fn validate(&self) -> std::result::Result<(), SimulationError> {
        if self.transfer_id == [0; 32] || self.terms_digest == [0; 32]
            || self.attempt_id == [0; 32] || self.payout_minor == 0
            || self.recorded_at == 0 || !self.currency.iter().all(u8::is_ascii_uppercase)
        {
            return Err(SimulationError::InvalidRequest);
        }
        Ok(())
    }
}

/// Produces an operator-signed simulated observation. Never accepts a
/// transaction whose full identity does not match the signing key.
/// In production only an approved gateway derives these normalized events
/// from authenticated device telemetry, never from a customer or P2P packet.
pub fn simulate(
    terminal_key: &SigningKey,
    request: MockRequest,
    capabilities: Capabilities,
    events: &[NormalizedEvent],
) -> std::result::Result<SignedTerminalObservation, SimulationError> {
    request.validate()?;
    if !capabilities.valid() || !capabilities.dispense
        || request.terminal != IdentityId::from_signing_key(terminal_key)
    {
        return Err(SimulationError::WrongSigner);
    }
    if events.is_empty() { return Err(SimulationError::MalformedEventSequence); }
    let mut moved = 0u64;
    let mut retracted = 0u64;
    let mut presented = false;
    let mut taken = ItemsTaken::NotObserved;
    let mut finished = false;
    let mut indeterminate = false;
    let mut log = blake3::Hasher::new_derive_key("hawala-atm/normalized-event-log/v1");
    for event in events {
        // A provider must reject further device events after final completion.
        if finished { return Err(SimulationError::MalformedEventSequence); }
        match *event {
            NormalizedEvent::TransportedMinor(n) if n > 0 => {
                moved = moved.checked_add(n).ok_or(SimulationError::ArithmeticOverflow)?;
                log.update(&[1]); log.update(&n.to_be_bytes());
            }
            NormalizedEvent::CustomerAccessOpened => {
                presented = true;
                log.update(&[2]);
            }
            NormalizedEvent::ItemsTakenSensorFired => {
                if capabilities.items_taken_sensor {
                    taken = ItemsTaken::ObservedTaken;
                } else {
                    // Impossible or untrusted sensor observation; remain
                    // uncertain even if other signals claim a full payout.
                    indeterminate = true;
                }
                log.update(&[3]);
            }
            NormalizedEvent::RetractedMinor(n) if n > 0 => {
                retracted = retracted.checked_add(n).ok_or(SimulationError::ArithmeticOverflow)?;
                log.update(&[4]); log.update(&n.to_be_bytes());
            }
            NormalizedEvent::Finished => {
                finished = true;
                log.update(&[5]);
            }
            NormalizedEvent::Indeterminate => {
                indeterminate = true;
                log.update(&[6]);
            }
            _ => return Err(SimulationError::MalformedEventSequence),
        }
    }
    // An interruption or impossible sensor signal cannot count as finished.
    if indeterminate { finished = false; }
    let log_hash = *log.finalize().as_bytes();
    let mut obs_id_hash = blake3::Hasher::new_derive_key("hawala-atm/sim-observation/v1");
    obs_id_hash.update(&request.transfer_id);
    obs_id_hash.update(&request.attempt_id);
    obs_id_hash.update(request.terminal.as_bytes());
    obs_id_hash.update(&log_hash);
    let observation_id = *obs_id_hash.finalize().as_bytes();
    let observation = TerminalObservation {
        terminal: request.terminal,
        transfer_id: request.transfer_id,
        terms_digest: request.terms_digest,
        attempt_id: request.attempt_id,
        observation_id, payout_minor: request.payout_minor,
        currency: request.currency, moved_to_output_minor: moved,
        customer_access_possible: presented,
        items_taken: taken, retracted_minor: retracted,
        terminal_finished: finished,
        terminal_log_digest: log_hash, recorded_at: request.recorded_at,
    };
    sign_observation(terminal_key, observation)
        .map_err(|_| SimulationError::InvalidRequest)
}
