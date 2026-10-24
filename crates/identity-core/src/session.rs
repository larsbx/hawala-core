//! QR identity ↔ authenticated transport endpoint binding.
//! An Iroh adapter MUST obtain the peer endpoint key from an authenticated
//! QUIC connection, not from a claimed address or peer-supplied message.
use ed25519_dalek::SigningKey;
use crate::identity::{IdentityId, ReplayGuard};
use crate::{check_signature, encode, signature, valid_label, Error, Result};

pub const MAX_SESSION_PROOF_SECS: u64 = 300;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndpointChallenge {
    pub verifier: IdentityId,
    pub subject: IdentityId,
    pub endpoint_key: [u8; 32],
    pub nonce: [u8; 32],
    pub purpose: String,
    pub issued_at: u64,
    pub expires_at: u64,
}
#[derive(Clone, Debug)]
pub struct SignedEndpointProof {
    pub subject: IdentityId,
    pub signature: [u8; 64],
}
/// Authenticated session association, NEVER a money-movement permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifiedPeer {
    identity: IdentityId,
    endpoint_key: [u8; 32],
    expires_at: u64,
}
impl VerifiedPeer {
    pub fn identity(&self) -> IdentityId { self.identity }
    pub fn endpoint_key(&self) -> [u8; 32] { self.endpoint_key }
    pub fn expires_at(&self) -> u64 { self.expires_at }
}
fn validate(c: &EndpointChallenge) -> Result<()> {
    if c.nonce == [0; 32]
        || c.endpoint_key == [0; 32]
        || !valid_label(&c.purpose)
        || c.expires_at <= c.issued_at
        || c.expires_at - c.issued_at > MAX_SESSION_PROOF_SECS
    {
        return Err(Error::InvalidInput);
    }
    Ok(())
}
fn statement(c: &EndpointChallenge) -> Vec<u8> {
    encode(b"iroh-transport-identity-binding/v1", &[
        c.verifier.as_bytes(), c.subject.as_bytes(), &c.endpoint_key, &c.nonce,
        c.purpose.as_bytes(), &c.issued_at.to_be_bytes(), &c.expires_at.to_be_bytes(),
    ])
}
/// Caller must confirm `owned_endpoint_key` was read from its OWN Iroh
/// endpoint. Refuse to bind a Hawala identity to an unrelated transport key.
pub fn sign_endpoint_proof(
    key: &SigningKey,
    owned_endpoint_key: [u8; 32],
    c: &EndpointChallenge,
) -> Result<SignedEndpointProof> {
    validate(c)?;
    let subject = IdentityId::from_signing_key(key);
    if subject != c.subject || owned_endpoint_key != c.endpoint_key {
        return Err(Error::PermissionDenied);
    }
    Ok(SignedEndpointProof {subject, signature: signature(key, &statement(c))})
}
/// `observed_endpoint_key` MUST come from Iroh's authenticated remote_id.
/// `guard` must atomically and durably consume nonces for production use.
pub fn verify_endpoint_proof(
    scanned_identity: IdentityId,
    observed_endpoint_key: [u8; 32],
    c: &EndpointChallenge,
    proof: &SignedEndpointProof,
    now: u64,
    guard: &impl ReplayGuard,
) -> Result<VerifiedPeer> {
    validate(c)?;
    if c.subject != scanned_identity || proof.subject != scanned_identity
        || observed_endpoint_key != c.endpoint_key {
        return Err(Error::PermissionDenied);
    }
    if now < c.issued_at { return Err(Error::NotYetValid); }
    if now >= c.expires_at { return Err(Error::Expired); }
    check_signature(&scanned_identity, &statement(c), &proof.signature)?;
    if !guard.consume_once(c.verifier, scanned_identity, c.nonce)? {
        return Err(Error::Replay);
    }
    Ok(VerifiedPeer {identity: scanned_identity, endpoint_key: observed_endpoint_key, expires_at: c.expires_at})
}
