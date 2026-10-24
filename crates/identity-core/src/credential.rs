use std::collections::BTreeSet;

use ed25519_dalek::SigningKey;

use crate::identity::IdentityId;
use crate::{check_signature, encode, signature, valid_label, Error, Result};

/// A signed claim is historical evidence, not an active network permission.
/// Short-lived credentials can describe *current* verified status; past claims
/// remain verifiable as evidence of what an issuer attested at issuance.
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
    /// Exact local policy scope, e.g. "US-PA"; never inferred.
    pub scope: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug)]
pub struct SignedCredential {
    pub claim: CredentialClaim,
    pub signature: [u8; 64],
}

/// LOCAL relying-party trust in credential issuers for a kind and scope.
/// This is not a revocation registry or an authority over an identity.
///
/// Financial eligibility, including an agent's current license/status, MUST be
/// established independently by hawala-protocol before moving money.
#[derive(Clone, Debug, Default)]
pub struct VerificationPolicy {
    grants: BTreeSet<(VerificationKind, String, IdentityId)>,
}

impl VerificationPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn authorize_issuer(
        &mut self,
        kind: VerificationKind,
        scope: &str,
        issuer: IdentityId,
    ) -> Result<()> {
        if !valid_label(scope) {
            return Err(Error::InvalidInput);
        }
        self.grants.insert((kind, scope.to_owned(), issuer));
        Ok(())
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

/// Verifies that the issuer signed a claim for this exact subject/kind/scope
/// during its stated validity interval and is trusted by LOCAL policy.
/// No network-wide revocation state exists in identity-core.
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
    if !policy.allows(c.kind, &c.scope, c.issuer) {
        return Err(Error::UntrustedIssuer);
    }
    Ok(())
}
