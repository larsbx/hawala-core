//! Disputes, awards and remedies (ADR 0005 gap G4).
//!
//! A refund, replacement or compensation is a **new** accepted dual-signed
//! obligation linked to the original by its terms digest. The original is
//! never rewritten. Every remedy needs a basis:
//!
//! | Kind | Shape of the new obligation | Permitted basis |
//! | --- | --- | --- |
//! | Refund | original beneficiary → original sender, funding currency | payee consent, or award |
//! | Replacement | original sender → original beneficiary, payout currency | sender consent, or award |
//! | Compensation | award's liable party → claimant, award currency | award only |
//!
//! A dispute opens a case and moves nothing; it holds the workflow. Awards
//! count only from the locally pinned adjudicator under its pinned mandate,
//! and only one award per case. `RemedyBook` caps cumulative refunds at the
//! original funding, replacements at the original payout, and every
//! award-based remedy at its award, so one loss cannot be recovered twice.
//! See `docs/REMEDIES.md`.
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hawala_identity_core::IdentityId;
use hawala_protocol::{SignedTerms, Terms};
use sha2::{Digest, Sha256};

const CONSENT_DOMAIN: &[u8] = b"hawala-remedy-consent/v1\0";
const AWARD_DOMAIN: &[u8] = b"hawala-remedy-award/v1\0";
const DISPUTE_DOMAIN: &[u8] = b"hawala-remedy-dispute/v1\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidSignature,
    WrongSigner,
    NotAParty,
    WrongOriginal,
    InvalidAward,
    BasisNotPermitted,
    WrongAdjudicator,
    AwardMismatch,
    AwardConflict,
    WrongShape,
    DuplicateTransfer,
    ExceedsOriginal,
    ExceedsAward,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Refund = 1,
    Replacement = 2,
    Compensation = 3,
}

fn verify(signer: &IdentityId, message: &[u8], signature: &[u8; 64]) -> Result<()> {
    VerifyingKey::from_bytes(signer.as_bytes())
        .map_err(|_| Error::InvalidSignature)?
        .verify_strict(message, &Signature::from_bytes(signature))
        .map_err(|_| Error::InvalidSignature)
}

fn parties(t: &Terms) -> [IdentityId; 4] {
    [t.sender, t.beneficiary, t.origin, t.disburser]
}

/// Bytes a party signs to consent to one remedy against one original.
pub fn consent_bytes(kind: Kind, original: &SignedTerms, remedy: &SignedTerms) -> Vec<u8> {
    [
        CONSENT_DOMAIN,
        &[kind as u8],
        &original.terms().digest(),
        &remedy.terms().digest(),
    ]
    .concat()
}

pub fn consent(
    key: &SigningKey,
    kind: Kind,
    original: &SignedTerms,
    remedy: &SignedTerms,
) -> [u8; 64] {
    key.sign(&consent_bytes(kind, original, remedy)).to_bytes()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dispute {
    pub case_id: [u8; 32],
    pub original_terms_digest: [u8; 32],
    pub claimant: IdentityId,
    pub reason_digest: [u8; 32],
    pub evidence_digest: [u8; 32],
    pub opened_at: u64,
}

impl Dispute {
    fn signing_bytes(&self) -> Vec<u8> {
        [
            DISPUTE_DOMAIN,
            &self.case_id,
            &self.original_terms_digest,
            self.claimant.as_bytes(),
            &self.reason_digest,
            &self.evidence_digest,
            &self.opened_at.to_be_bytes(),
        ]
        .concat()
    }
}

/// A case opened by a party. It moves nothing: the workflow is held and all
/// evidence retained until an award or a consented remedy resolves it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedDispute {
    dispute: Dispute,
    signature: [u8; 64],
}

impl SignedDispute {
    pub fn dispute(&self) -> &Dispute {
        &self.dispute
    }

    pub fn signing_bytes(&self) -> Vec<u8> {
        self.dispute.signing_bytes()
    }
}

pub fn sign_dispute(key: &SigningKey, dispute: Dispute) -> Result<SignedDispute> {
    if IdentityId::from_signing_key(key) != dispute.claimant {
        return Err(Error::WrongSigner);
    }
    let signature = key.sign(&dispute.signing_bytes()).to_bytes();
    Ok(SignedDispute { dispute, signature })
}

pub fn verify_dispute(signed: &SignedDispute, original: &SignedTerms) -> Result<()> {
    let d = &signed.dispute;
    if d.original_terms_digest != original.terms().digest() {
        return Err(Error::WrongOriginal);
    }
    if !parties(original.terms()).contains(&d.claimant) {
        return Err(Error::NotAParty);
    }
    verify(&d.claimant, &d.signing_bytes(), &signed.signature)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Award {
    pub case_id: [u8; 32],
    pub original_terms_digest: [u8; 32],
    pub mandate_digest: [u8; 32],
    pub kind: Kind,
    pub claimant: IdentityId,
    pub liable: IdentityId,
    pub currency: [u8; 3],
    pub amount: u64,
    pub issued_at: u64,
}

impl Award {
    fn validate(&self) -> Result<()> {
        if self.amount == 0
            || self.claimant == self.liable
            || !self.currency.iter().all(u8::is_ascii_uppercase)
        {
            return Err(Error::InvalidAward);
        }
        Ok(())
    }

    fn signing_bytes(&self) -> Vec<u8> {
        [
            AWARD_DOMAIN,
            &self.case_id,
            &self.original_terms_digest,
            &self.mandate_digest,
            &[self.kind as u8],
            self.claimant.as_bytes(),
            self.liable.as_bytes(),
            &self.currency,
            &self.amount.to_be_bytes(),
            &self.issued_at.to_be_bytes(),
        ]
        .concat()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedAward {
    award: Award,
    signature: [u8; 64],
}

impl SignedAward {
    pub fn award(&self) -> &Award {
        &self.award
    }

    pub fn signing_bytes(&self) -> Vec<u8> {
        self.award.signing_bytes()
    }

    pub fn digest(&self) -> [u8; 32] {
        Sha256::digest([self.signing_bytes().as_slice(), &self.signature].concat()).into()
    }
}

pub fn sign_award(key: &SigningKey, award: Award) -> Result<SignedAward> {
    award.validate()?;
    let signature = key.sign(&award.signing_bytes()).to_bytes();
    Ok(SignedAward { award, signature })
}

/// The adjudicator this relying party pinned for the original's workflow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Adjudicator {
    pub key: IdentityId,
    pub mandate_digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Basis {
    /// Consent of the original beneficiary (refunds only).
    PayeeConsent([u8; 64]),
    /// Consent of the original sender (replacements only).
    SenderConsent([u8; 64]),
    Award(SignedAward),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remedy {
    pub kind: Kind,
    /// The new accepted dual-signed obligation.
    pub terms: SignedTerms,
    pub basis: Basis,
}

impl Remedy {
    fn payout(&self) -> u128 {
        self.terms.terms().payout_minor as u128
    }

    fn award(&self) -> Option<&SignedAward> {
        match &self.basis {
            Basis::Award(a) => Some(a),
            _ => None,
        }
    }
}

/// The remedies admitted against one original. Immutable: `admit` returns a
/// new book and never changes the original or the prior book.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemedyBook {
    original: SignedTerms,
    adjudicator: Adjudicator,
    remedies: Vec<Remedy>,
}

impl RemedyBook {
    pub fn open(original: SignedTerms, adjudicator: Adjudicator) -> Self {
        Self {
            original,
            adjudicator,
            remedies: Vec::new(),
        }
    }

    pub fn original(&self) -> &SignedTerms {
        &self.original
    }

    fn total(&self, kind: Kind) -> u128 {
        self.remedies
            .iter()
            .filter(|r| r.kind == kind)
            .map(Remedy::payout)
            .sum()
    }

    pub fn refunded(&self) -> u128 {
        self.total(Kind::Refund)
    }

    pub fn replaced(&self) -> u128 {
        self.total(Kind::Replacement)
    }

    pub fn admit(&self, remedy: &Remedy) -> Result<Self> {
        if self.remedies.contains(remedy) {
            return Ok(self.clone());
        }
        let (o, t) = (self.original.terms(), remedy.terms.terms());
        if t.transfer_id == o.transfer_id
            || self
                .remedies
                .iter()
                .any(|r| r.terms.terms().transfer_id == t.transfer_id)
        {
            return Err(Error::DuplicateTransfer);
        }
        self.check_basis(remedy)?;
        let (from, to, currency) = match (remedy.kind, remedy.award()) {
            (Kind::Refund, _) => (o.beneficiary, o.sender, o.funding_currency),
            (Kind::Replacement, _) => (o.sender, o.beneficiary, o.payout_currency),
            (Kind::Compensation, Some(a)) => (a.award.liable, a.award.claimant, a.award.currency),
            (Kind::Compensation, None) => return Err(Error::BasisNotPermitted),
        };
        if (t.sender, t.beneficiary, t.payout_currency) != (from, to, currency) {
            return Err(Error::WrongShape);
        }
        let cap = match remedy.kind {
            Kind::Refund => Some(o.funding_minor as u128),
            Kind::Replacement => Some(o.payout_minor as u128),
            Kind::Compensation => None,
        };
        if cap.is_some_and(|cap| self.total(remedy.kind) + remedy.payout() > cap) {
            return Err(Error::ExceedsOriginal);
        }
        if let Some(a) = remedy.award() {
            let used: u128 = self
                .remedies
                .iter()
                .filter(|r| r.award() == Some(a))
                .map(Remedy::payout)
                .sum();
            if used + remedy.payout() > a.award.amount as u128 {
                return Err(Error::ExceedsAward);
            }
        }
        Ok(Self {
            remedies: [self.remedies.as_slice(), std::slice::from_ref(remedy)].concat(),
            ..self.clone()
        })
    }

    fn check_basis(&self, remedy: &Remedy) -> Result<()> {
        let o = self.original.terms();
        let consent = |signer: &IdentityId, signature: &[u8; 64]| {
            verify(
                signer,
                &consent_bytes(remedy.kind, &self.original, &remedy.terms),
                signature,
            )
        };
        match (&remedy.basis, remedy.kind) {
            (Basis::PayeeConsent(s), Kind::Refund) => consent(&o.beneficiary, s),
            (Basis::SenderConsent(s), Kind::Replacement) => consent(&o.sender, s),
            (Basis::Award(a), _) => self.check_award(a, remedy),
            _ => Err(Error::BasisNotPermitted),
        }
    }

    fn check_award(&self, signed: &SignedAward, remedy: &Remedy) -> Result<()> {
        let a = &signed.award;
        if a.mandate_digest != self.adjudicator.mandate_digest
            || verify(&self.adjudicator.key, &a.signing_bytes(), &signed.signature).is_err()
        {
            return Err(Error::WrongAdjudicator);
        }
        if a.original_terms_digest != self.original.terms().digest()
            || a.kind != remedy.kind
            || a.currency != remedy.terms.terms().payout_currency
        {
            return Err(Error::AwardMismatch);
        }
        let conflicting = self
            .remedies
            .iter()
            .filter_map(Remedy::award)
            .any(|prior| prior.award.case_id == a.case_id && prior != signed);
        if conflicting {
            return Err(Error::AwardConflict);
        }
        Ok(())
    }
}
