//! Transaction-specific QR *authorization evidence*, NOT customer money.
//!
//! A dynamic QR transports an opaque nonce, not a reusable bearer token or
//! personal information. The signed details must be retrieved via an already
//! mutually authenticated P2P session and checked against the local financial
//! proposal. The verifier must separately authorize current funds/custody,
//! operator delegation, jurisdiction, amounts/fees, and bind nonce consumption
//! atomically with a ledger mutation; this crate does NOT do that.
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hawala_flow::{bind, FlowKind, Role};
use hawala_identity_core::{IdentityId, ReplayGuard};
use hawala_interaction::{Action, AgreedInteraction};
use hawala_protocol::SignedTerms;

pub const MAX_QR_AUTH_SECS: u64 = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidRequest,
    UnsupportedOperation,
    WrongSignedFlow,
    WrongSigner,
    WrongAudience,
    WrongChallenge,
    WrongSession,
    WrongQr,
    InvalidSignature,
    NotYetValid,
    Expired,
    Replay,
    Storage,
}
pub type Result<T> = std::result::Result<T, Error>;

/// Every field is signed; source and payout fees require independent agreed
/// quote validation by the external financial engine. This packet alone
/// cannot verify exchange-rate disclosure or collateral.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorizationRequest {
    pub holder: IdentityId,
    pub verifier: IdentityId,
    pub transfer_id: [u8; 32],
    pub terms_digest: [u8; 32],
    pub interaction_nonce: [u8; 32],
    pub action: Action,
    pub amount_minor: u64,
    pub currency: [u8; 3],
    pub disclosed_fee_minor: u64,
    /// Durable payout attempt reference, or cash acceptance intent ID.
    pub attempt_id: [u8; 32],
    /// Fresh challenge issued by the scanning recipient/agent/ATM.
    pub challenge: [u8; 32],
    /// Dedicated random QR nonce displayed only for this authorization.
    pub qr_nonce: [u8; 32],
    /// Live authenticated Iroh session/channel binding provided by adapter.
    pub session_digest: [u8; 32],
    pub issued_at: u64,
    pub expires_at: u64,
}
impl AuthorizationRequest {
    pub fn qr_payload(&self) -> String {
        format!("hawala:auth:v1:{}", hex::encode(self.qr_nonce))
    }

    fn validate(&self) -> Result<()> {
        if self.holder == self.verifier || self.transfer_id == [0;32]
            || self.terms_digest == [0;32] || self.interaction_nonce == [0;32]
            || self.attempt_id == [0;32] || self.challenge == [0;32]
            || self.qr_nonce == [0;32] || self.session_digest == [0;32]
            || self.challenge == self.qr_nonce || self.amount_minor == 0
            || self.issued_at == 0 || self.expires_at <= self.issued_at
            || self.expires_at - self.issued_at > MAX_QR_AUTH_SECS
            || !self.currency.iter().all(u8::is_ascii_uppercase)
            || !matches!(self.action, Action::AcceptCash | Action::DistributeCash)
        {
            return Err(Error::InvalidRequest);
        }
        Ok(())
    }

    fn message(&self) -> Vec<u8> {
        let amount = self.amount_minor.to_be_bytes();
        let fee = self.disclosed_fee_minor.to_be_bytes();
        let issued = self.issued_at.to_be_bytes();
        let expires = self.expires_at.to_be_bytes();
        let action: &[u8] = match self.action {
            Action::AcceptCash => b"cash-in",
            Action::DistributeCash => b"cash-out",
            _ => b"invalid",
        };
        let fields: [&[u8]; 15] = [
            self.holder.as_bytes(), self.verifier.as_bytes(),
            &self.transfer_id, &self.terms_digest, &self.interaction_nonce,
            action, &amount, &self.currency, &fee,
            &self.attempt_id, &self.challenge, &self.qr_nonce,
            &self.session_digest, &issued, &expires,
        ];
        let mut out = b"hawala/temporary-qr-authorization/v1\0".to_vec();
        for part in fields {
            out.extend_from_slice(&(part.len() as u32).to_be_bytes());
            out.extend_from_slice(part);
        }
        out
    }

    /// Opaque QR reference must only resolve over an authenticated P2P path.
    pub fn parse_qr(payload: &str) -> Result<[u8; 32]> {
        let s = payload.strip_prefix("hawala:auth:v1:").ok_or(Error::WrongQr)?;
        if s.len() != 64 { return Err(Error::WrongQr); }
        let raw = hex::decode(s).map_err(|_| Error::WrongQr)?;
        let out: [u8;32] = raw.try_into().map_err(|_| Error::WrongQr)?;
        if out == [0;32] { return Err(Error::WrongQr); }
        Ok(out)
    }
}

#[derive(Clone, Debug)]
pub struct SignedAuthorization {
    request: AuthorizationRequest,
    signature: [u8; 64],
}
impl SignedAuthorization {
    pub fn request(&self) -> &AuthorizationRequest { &self.request }
    /// Identifies the exact authorization evidence retained by the
    /// transaction journal, not a financial settlement or withdrawal ID.
    pub fn digest(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new_derive_key("hawala/qr-auth-proof/v1");
        h.update(&self.request.message());
        h.update(&self.signature);
        *h.finalize().as_bytes()
    }
}

/// This verified evidence is deliberately NOT a spend or custody permit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifiedAuthorization {
    holder: IdentityId,
    verifier: IdentityId,
    transfer_id: [u8; 32],
    terms_digest: [u8; 32],
    attempt_id: [u8; 32],
    qr_nonce: [u8; 32],
    proof_digest: [u8; 32],
    expires_at: u64,
}
impl VerifiedAuthorization {
    pub fn holder(&self) -> IdentityId { self.holder }
    pub fn verifier(&self) -> IdentityId { self.verifier }
    pub fn transfer_id(&self) -> [u8; 32] { self.transfer_id }
    pub fn terms_digest(&self) -> [u8; 32] { self.terms_digest }
    pub fn attempt_id(&self) -> [u8; 32] { self.attempt_id }
    pub fn qr_nonce(&self) -> [u8; 32] { self.qr_nonce }
    pub fn proof_digest(&self) -> [u8; 32] { self.proof_digest }
    pub fn expires_at(&self) -> u64 { self.expires_at }
}

/// Checks accepted Hawala terms and both signed peer roles before signing
/// the exact transaction. It DOES NOT validate machine delegation, account
/// balance, the full quoted fee or actual issuance of the challenge.
fn check_flow(
    r: &AuthorizationRequest,
    accepted: &SignedTerms,
    agreed: &AgreedInteraction,
    now: u64,
) -> Result<()> {
    r.validate()?;
    let flow = bind(accepted, agreed, now).map_err(|_| Error::WrongSignedFlow)?;
    let intent = agreed.intent();
    let expected_kind = match flow.kind() {
        FlowKind::CashIn => Action::AcceptCash,
        FlowKind::CashOut => Action::DistributeCash,
        _ => return Err(Error::UnsupportedOperation),
    };
    if r.action != expected_kind || intent.interaction_nonce != r.interaction_nonce
        || r.transfer_id != flow.transfer_id() || r.terms_digest != flow.terms_digest()
        || r.issued_at < intent.created_at || r.issued_at >= intent.expires_at
    {
        return Err(Error::WrongSignedFlow);
    }
    let t = accepted.terms();
    let (amount, currency) = match flow.kind() {
        FlowKind::CashIn => (t.funding_minor, t.funding_currency),
        FlowKind::CashOut => (t.payout_minor, t.payout_currency),
        _ => return Err(Error::UnsupportedOperation),
    };
    if r.amount_minor != amount || r.currency != currency {
        return Err(Error::WrongSignedFlow);
    }
    let (holder, verifier) = match flow.initiator_role() {
        Role::Left => (intent.initiator.identity, intent.responder.identity),
        Role::Right => (intent.responder.identity, intent.initiator.identity),
    };
    if r.holder != holder || r.verifier != verifier {
        return Err(Error::WrongAudience);
    }
    Ok(())
}

pub fn sign_authorization(
    key: &SigningKey,
    request: AuthorizationRequest,
    accepted: &SignedTerms,
    agreed: &AgreedInteraction,
    now: u64,
) -> Result<SignedAuthorization> {
    check_flow(&request, accepted, agreed, now)?;
    if IdentityId::from_signing_key(key) != request.holder {
        return Err(Error::WrongSigner);
    }
    let signature = key.sign(&request.message()).to_bytes();
    Ok(SignedAuthorization { request, signature })
}

/// Only verifies/consumes a signed approval. The caller MUST use a durable
/// authorization authority that atomically couples once-only consumption
/// with the actual financial reservation; a standalone local ReplayGuard
/// alone is NOT enough for physical cash dispensing.
pub fn verify_and_consume(
    signed: &SignedAuthorization,
    accepted: &SignedTerms,
    agreed: &AgreedInteraction,
    expected_qr_nonce: [u8; 32],
    expected_challenge: [u8; 32],
    expected_session: [u8; 32],
    expected_verifier: IdentityId,
    now: u64,
    guard: &impl ReplayGuard,
) -> Result<()> {
    let proof = verify_signed(
        signed, accepted, agreed, expected_qr_nonce, expected_challenge,
        expected_session, expected_verifier, now,
    )?;
    match guard.consume_once(proof.verifier(), proof.holder(), proof.qr_nonce()) {
        Ok(true) => Ok(()),
        Ok(false) => Err(Error::Replay),
        Err(_) => Err(Error::Storage),
    }
}

/// Signature-only verification for callers that must consume a nonce in the
/// SAME SQLite transaction as an intent/attempt mutation. Do not use this
/// API in isolation as proof that a financial transaction was approved or
/// actually executed.
pub fn verify_signed(
    signed: &SignedAuthorization,
    accepted: &SignedTerms,
    agreed: &AgreedInteraction,
    expected_qr_nonce: [u8; 32],
    expected_challenge: [u8; 32],
    expected_session: [u8; 32],
    expected_verifier: IdentityId,
    now: u64,
) -> Result<VerifiedAuthorization> {
    let r = &signed.request;
    check_flow(r, accepted, agreed, now)?;
    if r.verifier != expected_verifier { return Err(Error::WrongAudience); }
    if r.qr_nonce != expected_qr_nonce { return Err(Error::WrongQr); }
    if r.challenge != expected_challenge { return Err(Error::WrongChallenge); }
    if r.session_digest != expected_session { return Err(Error::WrongSession); }
    if now < r.issued_at { return Err(Error::NotYetValid); }
    if now >= r.expires_at { return Err(Error::Expired); }
    let pk = VerifyingKey::from_bytes(r.holder.as_bytes()).map_err(|_| Error::InvalidSignature)?;
    pk.verify_strict(&r.message(), &Signature::from_bytes(&signed.signature))
        .map_err(|_| Error::InvalidSignature)?;
    Ok(VerifiedAuthorization {
        holder: r.holder, verifier: r.verifier,
        transfer_id: r.transfer_id, terms_digest: r.terms_digest,
        attempt_id: r.attempt_id, qr_nonce: r.qr_nonce,
        proof_digest: signed.digest(), expires_at: r.expires_at,
    })
}
