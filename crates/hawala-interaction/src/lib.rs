//! The same QR-presented bilateral interaction contract applies to humans,
//! devices, and any pairing of the two. Human/machine classification is
//! descriptive data, not verified authorization or proof of payment.
//!
//! This crate signs a proposed interaction *context*. It does not issue
//! funds, execute cash acceptance/disbursement, or approve a financial claim.
//! Real execution still requires signed Hawala obligation terms, custody,
//! regulatory eligibility, human consent or delegated device authority,
//! and local operator-specific controls.
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hawala_identity_core::IdentityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticipantKind {
    Human,
    Machine,
}
impl ParticipantKind {
    fn tag(self) -> u8 {
        match self {
            Self::Human => 1,
            Self::Machine => 2,
        }
    }
}

/// Direction is retained: HumanToMachine and MachineToHuman are both part of
/// the human/machine family, but the initiator and responder are not silently
/// interchangeable because they may have different duties and controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionMode {
    HumanToHuman,
    HumanToMachine,
    MachineToHuman,
    MachineToMachine,
}
impl InteractionMode {
    pub fn is_human_machine(self) -> bool {
        matches!(self, Self::HumanToMachine | Self::MachineToHuman)
    }
}

/// Identity is the FULL public key resolved by QR, not the displayed human
/// name or short companion code. Machines require a separately identified
/// operator, whose authority still needs independently verified evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Participant {
    pub identity: IdentityId,
    pub kind: ParticipantKind,
    pub operator: Option<IdentityId>,
}
impl Participant {
    pub fn human(identity: IdentityId) -> Self {
        Self { identity, kind: ParticipantKind::Human, operator: None }
    }
    pub fn machine(identity: IdentityId, operator: IdentityId) -> Result<Self> {
        if identity == operator { return Err(Error::InvalidParticipant); }
        Ok(Self { identity, kind: ParticipantKind::Machine, operator: Some(operator) })
    }
    fn validate(self) -> Result<()> {
        match (self.kind, self.operator) {
            (ParticipantKind::Human, None) => Ok(()),
            (ParticipantKind::Machine, Some(operator)) if operator != self.identity => Ok(()),
            _ => Err(Error::InvalidParticipant),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    AcceptCash,
    DistributeCash,
    TransferValue,
    ReconcileObligation,
}
impl Action {
    fn tag(self) -> u8 {
        match self {
            Self::AcceptCash => 1,
            Self::DistributeCash => 2,
            Self::TransferValue => 3,
            Self::ReconcileObligation => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidParticipant,
    InvalidIntent,
    Expired,
    NotYetValid,
    WrongSigner,
    BadSignature,
    IdentityMismatch,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Canonical context signed by both parties; binding to an accepted Hawala
/// obligation digest does not independently verify that obligation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionIntent {
    pub initiator: Participant,
    pub responder: Participant,
    pub action: Action,
    pub hawala_terms_digest: [u8; 32],
    pub interaction_nonce: [u8; 32],
    pub created_at: u64,
    pub expires_at: u64,
}
impl InteractionIntent {
    pub fn mode(&self) -> InteractionMode {
        match (self.initiator.kind, self.responder.kind) {
            (ParticipantKind::Human, ParticipantKind::Human) => InteractionMode::HumanToHuman,
            (ParticipantKind::Human, ParticipantKind::Machine) => InteractionMode::HumanToMachine,
            (ParticipantKind::Machine, ParticipantKind::Human) => InteractionMode::MachineToHuman,
            (ParticipantKind::Machine, ParticipantKind::Machine) => InteractionMode::MachineToMachine,
        }
    }

    pub fn validate(&self) -> Result<()> {
        self.initiator.validate()?;
        self.responder.validate()?;
        if self.initiator.identity == self.responder.identity
            || self.hawala_terms_digest == [0; 32]
            || self.interaction_nonce == [0; 32]
            || self.created_at == 0
            || self.expires_at <= self.created_at
            || self.expires_at - self.created_at > 300
        {
            return Err(Error::InvalidIntent);
        }
        Ok(())
    }

    fn canonical(&self) -> Vec<u8> {
        let mut out = b"hawala-interaction/v1\0".to_vec();
        // All signed fields are length-prefixed and ordered. A device cannot
        // substitute a claimed operator, kind, role or action after signing.
        let initiator_kind = [self.initiator.kind.tag()];
        let responder_kind = [self.responder.kind.tag()];
        let action = [self.action.tag()];
        let created = self.created_at.to_be_bytes();
        let expires = self.expires_at.to_be_bytes();
        let initiator_operator = self.initiator.operator.as_ref()
            .map(|id| &id.as_bytes()[..]).unwrap_or(&[]);
        let responder_operator = self.responder.operator.as_ref()
            .map(|id| &id.as_bytes()[..]).unwrap_or(&[]);
        let fields: [&[u8]; 11] = [
            &self.initiator.identity.as_bytes()[..],
            &initiator_kind,
            initiator_operator,
            &self.responder.identity.as_bytes()[..],
            &responder_kind,
            responder_operator,
            &action,
            &self.hawala_terms_digest,
            &self.interaction_nonce,
            &created,
            &expires,
        ];
        for bytes in fields {
            out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            out.extend_from_slice(bytes);
        }
        out
    }
}

/// Signatures are from the actual participant's registered identity key.
/// A machine's operator ID alone cannot sign in place of its device identity,
/// and the presence of either signature is not a financial permission.
pub fn sign_intent(key: &SigningKey, intent: &InteractionIntent) -> Result<[u8; 64]> {
    intent.validate()?;
    let signer = IdentityId::from_signing_key(key);
    if signer != intent.initiator.identity && signer != intent.responder.identity {
        return Err(Error::WrongSigner);
    }
    Ok(key.sign(&intent.canonical()).to_bytes())
}

fn verify_signature(participant: Participant, data: &[u8], sig: &[u8; 64]) -> Result<()> {
    let pk = VerifyingKey::from_bytes(participant.identity.as_bytes())
        .map_err(|_| Error::BadSignature)?;
    pk.verify_strict(data, &Signature::from_bytes(sig)).map_err(|_| Error::BadSignature)
}

/// The only constructor for mutually signed intent evidence. Both parties
/// must accept identical bytes, including orientation, operator, mode,
/// action, Hawala terms digest and nonce.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgreedInteraction {
    intent: InteractionIntent,
    initiator_signature: [u8; 64],
    responder_signature: [u8; 64],
}
impl AgreedInteraction {
    pub fn intent(&self) -> &InteractionIntent { &self.intent }
    pub fn verify(&self, now: u64) -> Result<()> {
        self.intent.validate()?;
        if now < self.intent.created_at { return Err(Error::NotYetValid); }
        if now >= self.intent.expires_at { return Err(Error::Expired); }
        let msg = self.intent.canonical();
        verify_signature(self.intent.initiator, &msg, &self.initiator_signature)?;
        verify_signature(self.intent.responder, &msg, &self.responder_signature)?;
        Ok(())
    }
}
pub fn agree(
    intent: InteractionIntent,
    initiator_signature: [u8; 64],
    responder_signature: [u8; 64],
    now: u64,
) -> Result<AgreedInteraction> {
    let out = AgreedInteraction {intent, initiator_signature, responder_signature};
    out.verify(now)?;
    Ok(out)
}
