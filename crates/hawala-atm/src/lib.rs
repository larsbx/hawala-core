//! ATM-first financial *evidence*, not money movement or physical cash proof.
//!
//! A gateway signs what its terminal reports. A signature proves authorship,
//! not that a note actually reached the customer. No physical command or
//! automatic payout retry exists here. The human-visible terminal name/code
//! accompanies its QR identity; only the full QR public key authenticates it.
pub mod simulator;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hawala_identity_core::IdentityId;

/// Capabilities must originate from a locally trusted operator/device policy;
/// never accept them from an untrusted peer's self-declared payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub dispense: bool,
    pub deposit: bool,
    pub recycle: bool,
    pub items_taken_sensor: bool,
}
impl Capabilities {
    pub fn valid(self) -> bool {
        (!self.recycle || (self.deposit && self.dispense))
            && (self.dispense || !self.items_taken_sensor)
    }
}

/// XFS4IoT ItemsTakenEvent is meaningful only when supported by the device.
/// 'NotObserved' is NOT evidence that the customer did not take the notes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemsTaken {
    ObservedTaken,
    ObservedNotTaken,
    NotObserved,
}
impl ItemsTaken {
    fn tag(self) -> u8 {
        match self {
            Self::ObservedTaken => 1,
            Self::ObservedNotTaken => 2,
            Self::NotObserved => 3,
        }
    }
}

/// Immutable, opaque evidence of a single ATM dispense attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalObservation {
    pub terminal: IdentityId,
    pub transfer_id: [u8; 32],
    pub terms_digest: [u8; 32],
    pub attempt_id: [u8; 32],
    pub observation_id: [u8; 32],
    pub payout_minor: u64,
    pub currency: [u8; 3],
    /// Notes transported toward an output position. NOT notes taken.
    pub moved_to_output_minor: u64,
    pub customer_access_possible: bool,
    pub items_taken: ItemsTaken,
    pub retracted_minor: u64,
    pub terminal_finished: bool,
    /// Digest of restricted terminal logs/counters retained by operator.
    pub terminal_log_digest: [u8; 32],
    pub recorded_at: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedTerminalObservation {
    pub observation: TerminalObservation,
    pub signature: [u8; 64],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportClass {
    /// Operator claims fully taken with a trusted items-taken sensor.
    ReportedDelivered,
    /// Operator claims zero cash entered any customer-accessible path.
    ReportedNotDelivered,
    /// Partial/ambiguous/no sensor/jam/timeout/other unknown state.
    Uncertain,
}
impl ReportClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReportedDelivered => "reported_delivered",
            Self::ReportedNotDelivered => "reported_not_delivered",
            Self::Uncertain => "uncertain",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    WrongTerminal,
    BadSignature,
}
pub type Result<T> = std::result::Result<T, Error>;

fn encode(parts: &[&[u8]]) -> Vec<u8> {
    let mut result = b"hawala-atm/terminal-observation/v1\0".to_vec();
    for part in parts {
        result.extend_from_slice(&(part.len() as u32).to_be_bytes());
        result.extend_from_slice(part);
    }
    result
}

impl TerminalObservation {
    pub fn validate(&self) -> Result<()> {
        if self.transfer_id == [0; 32]
            || self.terms_digest == [0; 32]
            || self.attempt_id == [0; 32]
            || self.observation_id == [0; 32]
            || self.terminal_log_digest == [0; 32]
            || self.payout_minor == 0
            || self.recorded_at == 0
            || !self.currency.iter().all(u8::is_ascii_uppercase)
        {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }
    fn statement(&self) -> Vec<u8> {
        encode(&[
            self.terminal.as_bytes(), &self.transfer_id, &self.terms_digest,
            &self.attempt_id, &self.observation_id,
            &self.payout_minor.to_be_bytes(), &self.currency,
            &self.moved_to_output_minor.to_be_bytes(),
            &[self.customer_access_possible as u8], &[self.items_taken.tag()],
            &self.retracted_minor.to_be_bytes(),
            &[self.terminal_finished as u8], &self.terminal_log_digest,
            &self.recorded_at.to_be_bytes(),
        ])
    }
}

pub fn sign_observation(key: &SigningKey, observation: TerminalObservation)
    -> Result<SignedTerminalObservation>
{
    observation.validate()?;
    if IdentityId::from_signing_key(key) != observation.terminal {
        return Err(Error::WrongTerminal);
    }
    let signature = key.sign(&observation.statement()).to_bytes();
    Ok(SignedTerminalObservation { observation, signature })
}

impl SignedTerminalObservation {
    /// expected_terminal is pinned by local operator policy or pre-authorized
    /// payout-intent binding, NOT taken from this observation as an authority.
    pub fn verify(&self, expected_terminal: IdentityId) -> Result<()> {
        self.observation.validate()?;
        if self.observation.terminal != expected_terminal {
            return Err(Error::WrongTerminal);
        }
        let pk = VerifyingKey::from_bytes(expected_terminal.as_bytes())
            .map_err(|_| Error::BadSignature)?;
        pk.verify_strict(
            &self.observation.statement(),
            &Signature::from_bytes(&self.signature),
        ).map_err(|_| Error::BadSignature)
    }

    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_derive_key("hawala-atm/signed-evidence/v1");
        hasher.update(&self.observation.statement());
        hasher.update(&self.signature);
        *hasher.finalize().as_bytes()
    }

    /// Classification is deliberately conservative. No class unfreezes the
    /// Hawala journal, credits a beneficiary, or authorizes another attempt.
    pub fn classify(&self, trusted_capabilities: Capabilities) -> ReportClass {
        let o = &self.observation;
        if !trusted_capabilities.valid()
            || !trusted_capabilities.dispense || !o.terminal_finished
        {
            return ReportClass::Uncertain;
        }
        if trusted_capabilities.items_taken_sensor
            && o.items_taken == ItemsTaken::ObservedTaken
            && o.customer_access_possible
            && o.moved_to_output_minor == o.payout_minor
            && o.retracted_minor == 0
        {
            return ReportClass::ReportedDelivered;
        }
        if o.moved_to_output_minor == 0
            && !o.customer_access_possible
            && o.retracted_minor == 0
            && o.items_taken != ItemsTaken::ObservedTaken
        {
            return ReportClass::ReportedNotDelivered;
        }
        ReportClass::Uncertain
    }
}
