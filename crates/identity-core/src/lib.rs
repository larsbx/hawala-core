//! Reusable identity primitives. No network, filesystem, wallet or Hawala ledger dependency.
//! This is a protocol foundation, NOT a complete identity provider or money-movement system.
//! Identity and signed history are not revocable; application permissions are separate.
pub mod continuity;
mod credential;
mod identity;
mod reputation;

pub use credential::{
    issue_credential, verify_credential, CredentialClaim, SignedCredential, VerificationKind,
    VerificationPolicy,
};
pub use identity::{
    sign_challenge, verify_and_consume_challenge, Challenge, ChallengeResponse, IdentityId,
    IdentityPresentation, ReplayGuard,
};
pub use reputation::{
    evaluate_reputation, sign_rating, sign_receipt, Interaction, ReputationSummary, SignedInteraction,
    SignedRating, Rating,
};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    InvalidQr,
    InvalidKey,
    BadSignature,
    Expired,
    NotYetValid,
    UntrustedIssuer,
    PermissionDenied,
    Replay,
    DuplicateRating,
}

pub type Result<T> = std::result::Result<T, Error>;

/// All signed objects use length-prefixed, domain-separated binary encodings.
/// Do not replace this with ambiguous string concatenation or ordinary JSON.
fn encode(domain: &[u8], parts: &[&[u8]]) -> Vec<u8> {
    let mut out = b"hawala-identity-core/v1\0".to_vec();
    out.extend_from_slice(&(domain.len() as u32).to_be_bytes());
    out.extend_from_slice(domain);
    for part in parts {
        out.extend_from_slice(&(part.len() as u32).to_be_bytes());
        out.extend_from_slice(part);
    }
    out
}

fn valid_label(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)
}

fn signature(key: &SigningKey, message: &[u8]) -> [u8; 64] {
    key.sign(message).to_bytes()
}

fn check_signature(id: &IdentityId, message: &[u8], sig: &[u8; 64]) -> Result<()> {
    let key = VerifyingKey::from_bytes(id.as_bytes()).map_err(|_| Error::InvalidKey)?;
    key.verify_strict(message, &Signature::from_bytes(sig))
        .map_err(|_| Error::BadSignature)
}

