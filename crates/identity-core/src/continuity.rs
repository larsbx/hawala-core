//! Append-only continuity evidence. No identity revocation or financial authority.
use std::collections::BTreeSet;
use ed25519_dalek::SigningKey;
use crate::{check_signature, encode, signature, valid_label, Error, IdentityId, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContinuityClaim {
    pub predecessor: IdentityId,
    pub successor: IdentityId,
    pub scope: String,
    /// Opaque reference to independently obtained recovery policy/evidence.
    /// Presence of this digest proves neither its content nor prior existence.
    pub context: [u8; 32],
    pub nonce: [u8; 32],
}

impl ContinuityClaim {
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        if self.predecessor == self.successor || !valid_label(&self.scope)
            || self.context == [0; 32] || self.nonce == [0; 32] {
            return Err(Error::InvalidInput);
        }
        Ok(encode(b"continuity-claim-v1", &[
            self.predecessor.as_bytes(), self.successor.as_bytes(),
            self.scope.as_bytes(), &self.context, &self.nonce,
        ]))
    }
}

#[derive(Clone, Debug)]
pub struct ContinuityEndorsement {
    pub signer: IdentityId,
    pub signature: [u8; 64],
}

pub fn endorse(key: &SigningKey, claim: &ContinuityClaim) -> Result<ContinuityEndorsement> {
    Ok(ContinuityEndorsement {
        signer: IdentityId::from_signing_key(key),
        signature: signature(key, &claim.canonical_bytes()?),
    })
}

pub fn verify_endorsement(claim: &ContinuityClaim, e: &ContinuityEndorsement) -> Result<()> {
    check_signature(&e.signer, &claim.canonical_bytes()?, &e.signature)
}

/// Verifies all supplied signatures and requires successor acceptance. Returns
/// evidence ONLY: no witness trust, threshold, human continuity or authorization.
/// Caller must bound input counts before allocating/decoding network bundles.
pub fn verified_signers(claim: &ContinuityClaim, evidence: &[ContinuityEndorsement])
    -> Result<BTreeSet<IdentityId>> {
    claim.canonical_bytes()?;
    let mut signers = BTreeSet::new();
    for e in evidence {
        verify_endorsement(claim, e)?;
        signers.insert(e.signer);
    }
    if !signers.contains(&claim.successor) { return Err(Error::PermissionDenied); }
    Ok(signers)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimRelation { Identical, Competing, Distinct }

/// Compare claims AFTER verifying their endorsements. No history completeness
/// or rightful winner is established. Different contexts/nonces still compete
/// within the same predecessor and scope; scopes cannot resolve each other.
pub fn compare(a: &ContinuityClaim, b: &ContinuityClaim) -> Result<ClaimRelation> {
    a.canonical_bytes()?;
    b.canonical_bytes()?;
    Ok(if a == b { ClaimRelation::Identical }
        else if a.predecessor == b.predecessor && a.scope == b.scope {
            ClaimRelation::Competing
        } else { ClaimRelation::Distinct })
}
