//! Bind dual-signed Hawala interaction evidence to exact accepted transfer
//! terms, with ONE rule-set for human↔human, human↔machine, and machine↔machine.
//!
//! This crate is **structural validation only**. An `AgreedInteraction` is a
//! proposal acknowledged by two identity keys, *not* authority to move money.
//! In particular, a machine's self-claimed operator is not a delegation proof.
//! Custody, risk, agent eligibility, liquidity, transaction-bound authenticator
//! authorization, actual physical fulfillment and clearing all live elsewhere.
use hawala_identity_core::IdentityId;
use hawala_interaction::{Action, AgreedInteraction, Error as InteractionError, InteractionMode, Participant, ParticipantKind};
use hawala_protocol::SignedTerms;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowKind {
    CashIn,
    CashOut,
    AgentTransfer,
    AgentReconciliation,
}
impl FlowKind {
    pub fn action(self) -> Action {
        match self {
            Self::CashIn => Action::AcceptCash,
            Self::CashOut => Action::DistributeCash,
            Self::AgentTransfer => Action::TransferValue,
            Self::AgentReconciliation => Action::ReconcileObligation,
        }
    }
    fn principals(self, accepted: &SignedTerms) -> (IdentityId, IdentityId) {
        let t = accepted.terms();
        match self {
            Self::CashIn => (t.sender, t.origin),
            Self::CashOut => (t.beneficiary, t.disburser),
            Self::AgentTransfer | Self::AgentReconciliation => (t.origin, t.disburser),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Beneficiary or sender for the cash leg, origin hawaladar for agent leg.
    Left,
    /// Local cash-handling hawaladar or destination hawaladar for agent leg.
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInteraction(InteractionError),
    WrongObligation,
    UnsupportedAction,
    AmbiguousPrincipals,
    PrincipalMismatch,
    WrongParticipantType,
}
pub type Result<T> = std::result::Result<T, Error>;

/// Proof that the signed interaction describes the proper parties and exact
/// Hawala terms, *not* a financial permit, funds reservation or execution grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundFlow {
    transfer_id: [u8; 32],
    terms_digest: [u8; 32],
    kind: FlowKind,
    mode: InteractionMode,
    initiator_role: Role,
}

impl BoundFlow {
    pub fn transfer_id(&self) -> [u8; 32] { self.transfer_id }
    pub fn terms_digest(&self) -> [u8; 32] { self.terms_digest }
    pub fn kind(&self) -> FlowKind { self.kind }
    pub fn mode(&self) -> InteractionMode { self.mode }
    pub fn initiator_role(&self) -> Role { self.initiator_role }
    /// Machine self-description must be corroborated by an independent,
    /// current and transaction-scoped operator delegation and legal policy.
    pub fn requires_machine_authority(&self) -> bool {
        !matches!(self.mode, InteractionMode::HumanToHuman)
    }
}

/// Map a participant to the claimed principal, but never infer that a machine
/// is legitimately delegated because it names an operator in its own input.
/// Caller MUST independently check that assertion against trusted policy.
fn represents(p: &Participant, principal: IdentityId) -> bool {
    match p.kind {
        ParticipantKind::Human => p.identity == principal && p.operator.is_none(),
        ParticipantKind::Machine => p.identity != principal && p.operator == Some(principal),
    }
}

pub fn bind(accepted: &SignedTerms, agreed: &AgreedInteraction, now: u64) -> Result<BoundFlow> {
    agreed.verify(now).map_err(Error::InvalidInteraction)?;
    let intent = agreed.intent();
    let kind = match intent.action {
        Action::AcceptCash => FlowKind::CashIn,
        Action::DistributeCash => FlowKind::CashOut,
        Action::TransferValue => FlowKind::AgentTransfer,
        Action::ReconcileObligation => FlowKind::AgentReconciliation,
    };
    if intent.hawala_terms_digest != accepted.terms().digest() {
        return Err(Error::WrongObligation);
    }

    let (left, right) = kind.principals(accepted);
    if left == right { return Err(Error::AmbiguousPrincipals); }

    let (a, b) = (&intent.initiator, &intent.responder);
    let forward = represents(a, left) && represents(b, right);
    let reverse = represents(a, right) && represents(b, left);
    let initiator_role = match (forward, reverse) {
        (true, false) => Role::Left,
        (false, true) => Role::Right,
        _ => return Err(Error::PrincipalMismatch),
    };

    Ok(BoundFlow {
        transfer_id: accepted.terms().transfer_id,
        terms_digest: accepted.terms().digest(),
        kind,
        mode: intent.mode(),
        initiator_role,
    })
}

/// A payout *review* checklist, not a token for the ATM or an agent.
/// The financially responsible node must still commit the unique attempt
/// before calling an authorized cash adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequiredEvidence {
    pub separate_financial_policy: bool,
    pub durable_once_only_attempt: bool,
    pub transaction_bound_person_authorization: bool,
    pub trusted_device_delegation: bool,
    pub device_or_human_fulfillment_evidence: bool,
}

pub fn requirements(flow: &BoundFlow) -> RequiredEvidence {
    RequiredEvidence {
        separate_financial_policy: true,
        durable_once_only_attempt: true,
        transaction_bound_person_authorization: matches!(flow.kind, FlowKind::CashIn | FlowKind::CashOut),
        trusted_device_delegation: flow.requires_machine_authority(),
        device_or_human_fulfillment_evidence: matches!(flow.kind, FlowKind::CashIn | FlowKind::CashOut),
    }
}
