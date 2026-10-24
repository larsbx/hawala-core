use std::collections::BTreeSet;

use ed25519_dalek::SigningKey;

use crate::identity::IdentityId;
use crate::{check_signature, encode, signature, valid_label, Error, Result};

/// Verified attributes are NOT financial permission grants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VerificationKind {
    Person,
    Business,
    Merchant,
    Hawaladar,
    AtmOperator,
    Professional,
}

impl VerificationKind {
    fn tag(self) -> &'static [u8] {
        match self {
            Self::Person => b"person",
            Self::Business => b"business",
            Self::Merchant => b"merchant",
            Self::Hawaladar => b"hawaladar",
            Self::AtmOperator => b"atm-operator",
            Self::Professional => b"professional",
        }
    }
}

#[derive(Clone, Debug)]
pub struct CredentialClaim {
    pub credential_id: [u8; 16],
    pub issuer: IdentityId,
    pub subject: IdentityId,
    pub kind: VerificationKind,
    /// Exact policy scope, e.g. "US-PA" or "merchant.example"; never inferred.
    pub scope: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug)]
pub struct SignedCredential {
    pub claim: CredentialClaim,
    pub signature: [u8; 64],
}

/// A verifier's LOCAL issuer-authority and revocation policy, not a global
/// registry. A trusted identity verifier does not automatically authorize a
/// money transmitter, a merchant, or an ATM operator.
#[derive(Clone, Debug)]
pub struct VerificationPolicy {
    grants: BTreeSet<(VerificationKind, String, IdentityId)>,
    revoked: BTreeSet<[u8; 16]>,
    /// Unix seconds of the latest authoritative revocation check.
    pub revocation_checked_at: u64,
    pub max_revocation_age_seconds: u64,
}

impl VerificationPolicy {
    pub fn new(revocation_checked_at: u64, max_revocation_age_seconds: u64) -> Self {
        Self {
            grants: BTreeSet::new(),
            revoked: BTreeSet::new(),
            revocation_checked_at,
            max_revocation_age_seconds,
        }
    }

    pub fn authorize_issuer(&mut self, kind: VerificationKind, scope: &str, issuer: IdentityId) -> Result<()> {
        if !valid_label(scope) {
            return Err(Error::InvalidInput);
        }
        self.grants.insert((kind, scope.to_owned(), issuer));
        Ok(())
    }

    pub fn revoke(&mut self, credential_id: [u8; 16]) {
        self.revoked.insert(credential_id);
    }

    pub fn allows(&self, kind: VerificationKind, scope: &str, issuer: IdentityId) -> bool {
        self.grants.contains(&(kind, scope.to_owned(), issuer))
    }
}

fn claim_message(c: &CredentialClaim) -> Vec<u8> {
    encode(
        b"verification-credential",
        &[
            &c.credential_id,
            c.issuer.as_bytes(),
            c.subject.as_bytes(),
            c.kind.tag(),
            c.scope.as_bytes(),
            &c.issued_at.to_be_bytes(),
            &c.expires_at.to_be_bytes(),
        ],
    )
}

fn validate(c: &CredentialClaim) -> Result<()> {
    if c.credential_id == [0; 16]
        || !valid_label(&c.scope)
        || c.expires_at <= c.issued_at
    {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

pub fn issue_credential(key: &SigningKey, claim: CredentialClaim) -> Result<SignedCredential> {
    validate(&claim)?;
    if claim.issuer != IdentityId::from_signing_key(key) {
        return Err(Error::PermissionDenied);
    }
    let signature = signature(key, &claim_message(&claim));
    Ok(SignedCredential { claim, signature })
}

/// Checks signature, subject, exact credential kind and scope, issuer authority,
/// expiry and revocation freshness. The caller supplies authoritative policy.
pub fn verify_credential(
    credential: &SignedCredential,
    subject: IdentityId,
    kind: VerificationKind,
    scope: &str,
    policy: &VerificationPolicy,
    now: u64,
) -> Result<()> {
    let c = &credential.claim;
    validate(c)?;
    if c.subject != subject || c.kind != kind || c.scope != scope {
        return Err(Error::PermissionDenied);
    }
    check_signature(&c.issuer, &claim_message(c), &credential.signature)?;
    if now < c.issued_at {
        return Err(Error::NotYetValid);
    }
    if now >= c.expires_at {
        return Err(Error::Expired);
    }
    if policy.revocation_checked_at > now
        || now - policy.revocation_checked_at > policy.max_revocation_age_seconds
    {
        return Err(Error::StaleRevocations);
    }
    if policy.revoked.contains(&c.credential_id) {
        return Err(Error::Revoked);
    }
    if !policy.allows(c.kind, &c.scope, c.issuer) {
        return Err(Error::UntrustedIssuer);
    }
    Ok(())
}
