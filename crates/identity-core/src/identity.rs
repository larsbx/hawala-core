use crate::{check_signature, encode, signature, valid_label, Error, Result};
use ed25519_dalek::{SigningKey, VerifyingKey};

/// Long-lived identity, independent of Iroh endpoint IDs and transport keys.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct IdentityId([u8; 32]);

impl IdentityId {
    pub fn from_signing_key(key: &SigningKey) -> Self {
        Self(key.verifying_key().to_bytes())
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self> {
        VerifyingKey::from_bytes(&bytes).map_err(|_| Error::InvalidKey)?;
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// QR payload text. Rendering as an image is an application-layer concern.
    /// This public identifier is NOT proof of possession or permission to withdraw.
    pub fn qr_payload(&self) -> String {
        format!("hawala:id:v1:{}", hex::encode(self.0))
    }

    pub fn parse_qr(payload: &str) -> Result<Self> {
        let encoded = payload.strip_prefix("hawala:id:v1:").ok_or(Error::InvalidQr)?;
        if encoded.len() != 64 {
            return Err(Error::InvalidQr);
        }
        let raw = hex::decode(encoded).map_err(|_| Error::InvalidQr)?;
        let bytes: [u8; 32] = raw.try_into().map_err(|_| Error::InvalidQr)?;
        Self::from_bytes(bytes)
    }
}

/// Verifier-generated challenge. Nonce MUST be cryptographically random and
/// unique per verifier; expires_at is Unix seconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Challenge {
    pub verifier: IdentityId,
    pub nonce: [u8; 32],
    pub purpose: String,
    pub expires_at: u64,
}

#[derive(Clone, Debug)]
pub struct ChallengeResponse {
    pub claimant: IdentityId,
    pub signature: [u8; 64],
}

/// Production implementations MUST atomically and durably consume the nonce.
/// No global in-memory Set can enforce network-wide at-most-once semantics.
pub trait ReplayGuard {
    fn consume_once(&self, verifier: IdentityId, claimant: IdentityId, nonce: [u8; 32]) -> Result<bool>;
}

fn challenge_message(claimant: IdentityId, c: &Challenge) -> Vec<u8> {
    encode(
        b"identity-possession-challenge",
        &[claimant.as_bytes(), c.verifier.as_bytes(), &c.nonce, c.purpose.as_bytes(), &c.expires_at.to_be_bytes()],
    )
}

fn validate(c: &Challenge) -> Result<()> {
    if c.nonce == [0; 32] || !valid_label(&c.purpose) || c.expires_at == 0 {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

pub fn sign_challenge(key: &SigningKey, challenge: &Challenge) -> Result<ChallengeResponse> {
    validate(challenge)?;
    let claimant = IdentityId::from_signing_key(key);
    Ok(ChallengeResponse {
        claimant,
        signature: signature(key, &challenge_message(claimant, challenge)),
    })
}

/// Authenticates the scanned QR identity against a fresh, scoped challenge.
/// The caller must generate the nonce securely and enforce its single use.
pub fn verify_and_consume_challenge(
    scanned_identity: IdentityId,
    expected: &Challenge,
    response: &ChallengeResponse,
    now: u64,
    guard: &impl ReplayGuard,
) -> Result<()> {
    validate(expected)?;
    if response.claimant != scanned_identity {
        return Err(Error::PermissionDenied);
    }
    if now >= expected.expires_at {
        return Err(Error::Expired);
    }
    check_signature(
        &scanned_identity,
        &challenge_message(scanned_identity, expected),
        &response.signature,
    )?;
    if !guard.consume_once(expected.verifier, scanned_identity, expected.nonce)? {
        return Err(Error::Replay);
    }
    Ok(())
}
