//! A *verification-only* gate for signed cash-out evidence.
//!
//! An independent policy issuer attests current eligibility, an independent
//! custodian signs a hold assertion, and any human/machine principal signs
//! explicit device delegation. This only proves signatures and exact scope.
//! It does NOT independently establish the existence of custody assets,
//! licensing, finality, liquidity, governance quorum or ledger state.
//! No API in this crate issues money, authorizes a physical dispenser, or
//! consumes a nonce in the same commit as a real financial reservation.
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hawala_flow::{bind, FlowKind, Role};
use hawala_identity_core::IdentityId;
use hawala_interaction::{AgreedInteraction, Participant, ParticipantKind};
use hawala_protocol::SignedTerms;
use hawala_qr_auth::{verify_signed, SignedAuthorization};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    InvalidAuthorization,
    WrongPrincipal,
    WrongIssuer,
    WrongKind,
    WrongContext,
    InvalidSignature,
    MissingDelegation,
    UnexpectedDelegation,
    Expired,
    NotYetValid,
    UntrustedPolicy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Eligibility, CustodyHold, DelegateHolder, DelegateCashPeer }
impl Kind {
    fn tag(self) -> u8 {
        match self {
            Self::Eligibility => 1, Self::CustodyHold => 2,
            Self::DelegateHolder => 3, Self::DelegateCashPeer => 4,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Authorities {
    pub eligibility_issuer: IdentityId,
    pub custody_issuer: IdentityId,
    /// Pinned policy/custody epoch established by the responsible authority.
    /// A peer-provided number is never evidence that the epoch is current.
    pub epoch: u64,
    /// Expected exact fee from an independently agreed customer-facing quote.
    pub expected_fee_minor: u64,
    /// Require at least this externally checked custody account version.
    pub min_custody_version: u64,
}
impl Authorities {
    fn validate(&self, terms: &SignedTerms) -> Result<()> {
        let t = terms.terms();
        if self.epoch == 0 || self.min_custody_version == 0
            || self.eligibility_issuer == self.custody_issuer
            || self.eligibility_issuer == t.beneficiary
            || self.eligibility_issuer == t.disburser
            || self.custody_issuer == t.beneficiary
            || self.custody_issuer == t.disburser
        { return Err(Error::UntrustedPolicy); }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub transfer_id: [u8; 32],
    pub terms_digest: [u8; 32],
    pub attempt_id: [u8; 32],
    pub qr_digest: [u8; 32],
    pub beneficiary: IdentityId,
    pub disburser: IdentityId,
    pub holder: IdentityId,
    pub cash_peer: IdentityId,
    pub amount_minor: u64,
    pub currency: [u8; 3],
    pub fee_minor: u64,
    pub epoch: u64,
}
impl Context {
    fn message(&self, kind: Kind, issuer: IdentityId, state_version: u64,
               evidence_digest: [u8; 32], issued: u64, expires: u64) -> Vec<u8> {
        let mut out = b"hawala/admission-assertion/v1\0".to_vec();
        let tag = [kind.tag()];
        let amount = self.amount_minor.to_be_bytes();
        let fee = self.fee_minor.to_be_bytes();
        let epoch = self.epoch.to_be_bytes();
        let version = state_version.to_be_bytes();
        let since = issued.to_be_bytes();
        let until = expires.to_be_bytes();
        let parts: [&[u8]; 17] = [
            &tag, issuer.as_bytes(),
            &self.transfer_id, &self.terms_digest, &self.attempt_id,
            &self.qr_digest, self.beneficiary.as_bytes(), self.disburser.as_bytes(),
            self.holder.as_bytes(), self.cash_peer.as_bytes(),
            &amount, &self.currency, &fee, &epoch, &version, &evidence_digest, &since,
        ];
        for p in parts.iter().copied().chain(std::iter::once(until.as_slice())) {
            out.extend_from_slice(&(p.len() as u32).to_be_bytes());
            out.extend_from_slice(p);
        }
        out
    }
}
/// A signed authority claim remains an *attestation*, not truth about actual
/// funds or an enforceable legal mandate. Proofs can be stale or dishonest.
#[derive(Clone, Debug)]
pub struct SignedAssertion {
    pub kind: Kind,
    pub issuer: IdentityId,
    pub context: Context,
    pub state_version: u64,
    pub evidence_digest: [u8; 32],
    pub issued_at: u64,
    pub expires_at: u64,
    pub signature: [u8; 64],
}
impl SignedAssertion {
    fn validate(&self, now: u64) -> Result<()> {
        if self.issued_at == 0 || self.expires_at <= self.issued_at
            || self.expires_at - self.issued_at > 300
            || self.evidence_digest == [0;32] || self.state_version == 0
        { return Err(Error::Invalid); }
        if now < self.issued_at { return Err(Error::NotYetValid); }
        if now >= self.expires_at { return Err(Error::Expired); }
        let pk = VerifyingKey::from_bytes(self.issuer.as_bytes())
            .map_err(|_| Error::InvalidSignature)?;
        pk.verify_strict(
            &self.context.message(self.kind, self.issuer, self.state_version,
                                  self.evidence_digest, self.issued_at, self.expires_at),
            &Signature::from_bytes(&self.signature),
        ).map_err(|_| Error::InvalidSignature)
    }
}
pub fn sign_assertion(
    key: &SigningKey, kind: Kind, context: Context,
    state_version: u64, evidence_digest: [u8;32],
    issued_at: u64, expires_at: u64,
) -> Result<SignedAssertion> {
    if context.amount_minor == 0 || context.epoch == 0
        || context.attempt_id == [0;32] || context.qr_digest == [0;32]
        || state_version == 0 || evidence_digest == [0;32]
        || issued_at == 0 || expires_at <= issued_at
        || expires_at - issued_at > 300
    { return Err(Error::Invalid); }
    let issuer = IdentityId::from_signing_key(key);
    let signature = key.sign(&context.message(
        kind, issuer, state_version, evidence_digest, issued_at, expires_at,
    )).to_bytes();
    Ok(SignedAssertion {
        kind, issuer, context, state_version,
        evidence_digest, issued_at, expires_at, signature,
    })
}

#[derive(Clone, Debug)]
pub struct EvidenceBundle {
    pub eligibility: SignedAssertion,
    pub custody_hold: SignedAssertion,
    pub holder_delegation: Option<SignedAssertion>,
    pub cash_peer_delegation: Option<SignedAssertion>,
}

/// A report of passing **cryptographic prerequisites**, not an entitlement.
/// It may NOT be presented to a dispenser or used for a real ledger debit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvaluatedEvidence {
    pub transfer_id: [u8;32],
    pub attempt_id: [u8;32],
    pub proof_digest: [u8;32],
    pub cash_peer: IdentityId,
    pub custody_version: u64,
}

fn check_proof(
    proof: &SignedAssertion,
    kind: Kind,
    issuer: IdentityId,
    context: &Context,
    now: u64,
) -> Result<()> {
    if proof.kind != kind { return Err(Error::WrongKind); }
    if proof.issuer != issuer { return Err(Error::WrongIssuer); }
    if &proof.context != context { return Err(Error::WrongContext); }
    proof.validate(now)
}

fn check_delegation(
    p: Participant, principal: IdentityId,
    kind: Kind, given: Option<&SignedAssertion>,
    context: &Context, now: u64,
) -> Result<()> {
    match p.kind {
        ParticipantKind::Human => {
            if p.identity != principal || p.operator.is_some() { return Err(Error::WrongPrincipal); }
            if given.is_some() { return Err(Error::UnexpectedDelegation); }
            Ok(())
        }
        ParticipantKind::Machine => {
            if p.operator != Some(principal) || p.identity == principal {
                return Err(Error::WrongPrincipal);
            }
            let delegation = given.ok_or(Error::MissingDelegation)?;
            check_proof(delegation, kind, principal, context, now)
        }
    }
}

/// Enforces signature/identity/fee/epoch alignment for ALL four oriented peer
/// combinations. Caller supplies `Authorities` from independently anchored
/// policy and MUST also reconcile actual custodian/ledger state/version.
pub fn evaluate(
    terms: &SignedTerms,
    agreement: &AgreedInteraction,
    qr: &SignedAuthorization,
    expected_qr_nonce: [u8;32],
    expected_challenge: [u8;32],
    expected_session: [u8;32],
    authorities: &Authorities,
    evidence: &EvidenceBundle,
    now: u64,
) -> Result<EvaluatedEvidence> {
    authorities.validate(terms)?;
    let flow = bind(terms, agreement, now).map_err(|_| Error::InvalidAuthorization)?;
    if flow.kind() != FlowKind::CashOut { return Err(Error::InvalidAuthorization); }
    let intent = agreement.intent();
    let (holder, cash_peer) = match flow.initiator_role() {
        Role::Left => (intent.initiator, intent.responder),
        Role::Right => (intent.responder, intent.initiator),
    };
    let v = verify_signed(qr, terms, agreement,
        expected_qr_nonce, expected_challenge, expected_session,
        cash_peer.identity, now
    ).map_err(|_| Error::InvalidAuthorization)?;
    let t = terms.terms();
    if qr.request().disclosed_fee_minor != authorities.expected_fee_minor {
        return Err(Error::WrongContext);
    }
    let context = Context {
        transfer_id: t.transfer_id,
        terms_digest: t.digest(), attempt_id: v.attempt_id(),
        qr_digest: v.proof_digest(),
        beneficiary: t.beneficiary, disburser: t.disburser,
        holder: holder.identity, cash_peer: cash_peer.identity,
        amount_minor: t.payout_minor, currency: t.payout_currency,
        fee_minor: authorities.expected_fee_minor, epoch: authorities.epoch,
    };
    check_proof(&evidence.eligibility, Kind::Eligibility,
        authorities.eligibility_issuer, &context, now)?;
    check_proof(&evidence.custody_hold, Kind::CustodyHold,
        authorities.custody_issuer, &context, now)?;
    if evidence.custody_hold.state_version < authorities.min_custody_version {
        return Err(Error::UntrustedPolicy);
    }
    check_delegation(holder, t.beneficiary, Kind::DelegateHolder,
        evidence.holder_delegation.as_ref(), &context, now)?;
    check_delegation(cash_peer, t.disburser, Kind::DelegateCashPeer,
        evidence.cash_peer_delegation.as_ref(), &context, now)?;
    Ok(EvaluatedEvidence {
        transfer_id:t.transfer_id, attempt_id:v.attempt_id(),
        proof_digest:v.proof_digest(), cash_peer:cash_peer.identity,
        custody_version:evidence.custody_hold.state_version,
    })
}
