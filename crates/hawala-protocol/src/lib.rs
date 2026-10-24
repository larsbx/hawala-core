//! Bilateral Hawala obligation evidence — milestone 3.
//! WARNING: This does NOT control money, guarantee cash fulfillment, implement
//! accounting, confer licenses, or establish external settlement finality.
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hawala_identity_core::IdentityId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidTerms, InvalidClaim, InvalidSignature, WrongSigner,
    ExpiredQuote, ConflictingEvidence, DifferentObligation,
}
pub type Result<T> = std::result::Result<T, Error>;

fn encode(domain: &[u8], fields: &[&[u8]]) -> Vec<u8> {
    let mut out = b"hawala-obligation/v1\0".to_vec();
    out.extend_from_slice(&(domain.len() as u32).to_be_bytes());
    out.extend_from_slice(domain);
    for part in fields {
        out.extend_from_slice(&(part.len() as u32).to_be_bytes());
        out.extend_from_slice(part);
    }
    out
}
fn sign(key: &SigningKey, bytes: &[u8]) -> [u8; 64] {
    key.sign(bytes).to_bytes()
}
fn check(identity: IdentityId, bytes: &[u8], signature: &[u8; 64]) -> Result<()> {
    let key = VerifyingKey::from_bytes(identity.as_bytes())
        .map_err(|_| Error::InvalidSignature)?;
    key.verify_strict(bytes, &Signature::from_bytes(signature))
        .map_err(|_| Error::InvalidSignature)
}
fn money(currency: [u8; 3], amount: u64) -> bool {
    amount > 0 && currency.iter().all(u8::is_ascii_uppercase)
}

/// One signed obligation quotes both currencies explicitly. A transfer ID must
/// be generated uniquely by a secure RNG and durably deduplicated by each peer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Terms {
    pub transfer_id: [u8; 32],
    pub sender: IdentityId,
    pub beneficiary: IdentityId,
    pub origin: IdentityId,
    pub disburser: IdentityId,
    pub funding_minor: u64,
    pub funding_currency: [u8; 3],
    pub payout_minor: u64,
    pub payout_currency: [u8; 3],
    pub proposed_at: u64,
    pub accepted_at: u64,
    pub quote_expires_at: u64,
}
impl Terms {
    fn validate(&self) -> Result<()> {
        if self.transfer_id == [0; 32]
            || self.origin == self.disburser
            || self.disburser == self.beneficiary
            || !money(self.funding_currency, self.funding_minor)
            || !money(self.payout_currency, self.payout_minor)
            || self.proposed_at == 0
            || self.accepted_at < self.proposed_at
            || self.accepted_at > self.quote_expires_at
            || self.quote_expires_at - self.proposed_at > 86_400
        {
            return Err(Error::InvalidTerms);
        }
        Ok(())
    }
    fn message(&self) -> Vec<u8> {
        encode(b"dual-agent-accepted-terms", &[
            &self.transfer_id, self.sender.as_bytes(), self.beneficiary.as_bytes(),
            self.origin.as_bytes(), self.disburser.as_bytes(),
            &self.funding_minor.to_be_bytes(), &self.funding_currency,
            &self.payout_minor.to_be_bytes(), &self.payout_currency,
            &self.proposed_at.to_be_bytes(), &self.accepted_at.to_be_bytes(),
            &self.quote_expires_at.to_be_bytes(),
        ])
    }
    pub fn digest(&self) -> [u8; 32] { *blake3::hash(&self.message()).as_bytes() }
}

/// Signer must be either the origin or the disburser. Both parties sign the
/// identical complete terms, including FX amounts and acceptance timestamp.
pub fn sign_terms(key: &SigningKey, terms: &Terms) -> Result<[u8; 64]> {
    terms.validate()?;
    let signer = IdentityId::from_signing_key(key);
    if signer != terms.origin && signer != terms.disburser {
        return Err(Error::WrongSigner);
    }
    Ok(sign(key, &terms.message()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedTerms {
    terms: Terms,
    origin_signature: [u8; 64],
    disburser_signature: [u8; 64],
}
impl SignedTerms {
    pub fn terms(&self) -> &Terms { &self.terms }
    fn verify(&self) -> Result<()> {
        self.terms.validate()?;
        check(self.terms.origin, &self.terms.message(), &self.origin_signature)?;
        check(self.terms.disburser, &self.terms.message(), &self.disburser_signature)?;
        Ok(())
    }
}
pub fn accept_terms(terms: Terms, origin_signature: [u8; 64], disburser_signature: [u8; 64])
    -> Result<SignedTerms>
{
    let accepted = SignedTerms {terms, origin_signature, disburser_signature};
    accepted.verify()?;
    Ok(accepted)
}

/// Evidence that the disburser and beneficiary both attest to full payout.
/// This is *not* an ATM dispenser hardware attestation, nor partial payout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayoutClaim {
    pub transfer_id: [u8; 32],
    pub terms_digest: [u8; 32],
    pub receipt_id: [u8; 32],
    pub payout_minor: u64,
    pub payout_currency: [u8; 3],
    pub paid_at: u64,
}
impl PayoutClaim {
    fn validate(&self, terms: &Terms) -> Result<()> {
        if self.transfer_id != terms.transfer_id
            || self.terms_digest != terms.digest()
            || self.receipt_id == [0; 32]
            || self.payout_minor != terms.payout_minor
            || self.payout_currency != terms.payout_currency
            || self.paid_at < terms.accepted_at
        { return Err(Error::InvalidClaim); }
        Ok(())
    }
    fn message(&self) -> Vec<u8> {
        encode(b"dual-party-payout-evidence", &[
            &self.transfer_id, &self.terms_digest, &self.receipt_id,
            &self.payout_minor.to_be_bytes(), &self.payout_currency,
            &self.paid_at.to_be_bytes(),
        ])
    }
}
pub fn sign_payout(key: &SigningKey, accepted: &SignedTerms, claim: &PayoutClaim)
    -> Result<[u8; 64]>
{
    accepted.verify()?;
    claim.validate(accepted.terms())?;
    let signer = IdentityId::from_signing_key(key);
    if signer != accepted.terms.disburser && signer != accepted.terms.beneficiary {
        return Err(Error::WrongSigner);
    }
    Ok(sign(key, &claim.message()))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedPayout {
    claim: PayoutClaim,
    disburser_signature: [u8; 64],
    beneficiary_signature: [u8; 64],
}
impl SignedPayout {
    pub fn claim(&self) -> &PayoutClaim { &self.claim }
    fn verify(&self, terms: &Terms) -> Result<()> {
        self.claim.validate(terms)?;
        check(terms.disburser, &self.claim.message(), &self.disburser_signature)?;
        check(terms.beneficiary, &self.claim.message(), &self.beneficiary_signature)
    }
}
pub fn attest_payout(
    accepted: &SignedTerms, claim: PayoutClaim,
    disburser_signature: [u8; 64], beneficiary_signature: [u8; 64],
) -> Result<SignedPayout> {
    accepted.verify()?;
    let receipt = SignedPayout {claim, disburser_signature, beneficiary_signature};
    receipt.verify(accepted.terms())?;
    Ok(receipt)
}

/// Bilateral acknowledgment of discharged inter-agent obligations. The
/// receipt's external evidence digest is only a reference to independently
/// verified settlement documentation, never finality by itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementClaim {
    pub transfer_id: [u8; 32],
    pub terms_digest: [u8; 32],
    pub receipt_id: [u8; 32],
    pub evidence_digest: [u8; 32],
    pub acknowledged_at: u64,
}
impl SettlementClaim {
    fn validate(&self, terms: &Terms) -> Result<()> {
        if self.transfer_id != terms.transfer_id
            || self.terms_digest != terms.digest()
            || self.receipt_id == [0; 32]
            || self.evidence_digest == [0; 32]
            || self.acknowledged_at < terms.accepted_at
        { return Err(Error::InvalidClaim); }
        Ok(())
    }
    fn message(&self) -> Vec<u8> {
        encode(b"dual-agent-settlement-evidence", &[
            &self.transfer_id, &self.terms_digest, &self.receipt_id,
            &self.evidence_digest, &self.acknowledged_at.to_be_bytes(),
        ])
    }
}
pub fn sign_settlement(key: &SigningKey, accepted: &SignedTerms, claim: &SettlementClaim)
    -> Result<[u8; 64]>
{
    accepted.verify()?;
    claim.validate(accepted.terms())?;
    let signer = IdentityId::from_signing_key(key);
    if signer != accepted.terms.origin && signer != accepted.terms.disburser {
        return Err(Error::WrongSigner);
    }
    Ok(sign(key, &claim.message()))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedSettlement {
    claim: SettlementClaim,
    origin_signature: [u8; 64],
    disburser_signature: [u8; 64],
}
impl SignedSettlement {
    pub fn claim(&self) -> &SettlementClaim { &self.claim }
    fn verify(&self, terms: &Terms) -> Result<()> {
        self.claim.validate(terms)?;
        check(terms.origin, &self.claim.message(), &self.origin_signature)?;
        check(terms.disburser, &self.claim.message(), &self.disburser_signature)
    }
}
pub fn attest_settlement(
    accepted: &SignedTerms, claim: SettlementClaim,
    origin_signature: [u8; 64], disburser_signature: [u8; 64],
) -> Result<SignedSettlement> {
    accepted.verify()?;
    let receipt = SignedSettlement {claim, origin_signature, disburser_signature};
    receipt.verify(accepted.terms())?;
    Ok(receipt)
}

/// Verified, order-independent evidence; not a journal persisted to durable
/// storage. Payout and inter-agent settlement remain INDEPENDENT states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObligationEvidence {
    accepted: SignedTerms,
    payout: Option<SignedPayout>,
    settlement: Option<SignedSettlement>,
}
impl ObligationEvidence {
    pub fn new(accepted: SignedTerms) -> Result<Self> {
        accepted.verify()?;
        Ok(Self {accepted, payout: None, settlement: None})
    }
    pub fn accepted(&self) -> &SignedTerms { &self.accepted }
    pub fn payout_attested(&self) -> bool { self.payout.is_some() }
    pub fn settlement_attested(&self) -> bool { self.settlement.is_some() }

    pub fn add_payout(&mut self, payout: SignedPayout) -> Result<bool> {
        payout.verify(self.accepted.terms())?;
        match &self.payout {
            Some(old) if old == &payout => Ok(false),
            Some(_) => Err(Error::ConflictingEvidence),
            None => { self.payout = Some(payout); Ok(true) }
        }
    }
    pub fn add_settlement(&mut self, settlement: SignedSettlement) -> Result<bool> {
        settlement.verify(self.accepted.terms())?;
        match &self.settlement {
            Some(old) if old == &settlement => Ok(false),
            Some(_) => Err(Error::ConflictingEvidence),
            None => { self.settlement = Some(settlement); Ok(true) }
        }
    }
    /// Atomic, deterministic merge: neither a conflict nor a bad signature
    /// can leave half of a remote peer's evidence applied.
    pub fn merge(&mut self, other: &Self) -> Result<usize> {
        if self.accepted != other.accepted {
            return Err(Error::DifferentObligation);
        }
        let mut next = self.clone();
        let mut added = 0;
        if let Some(receipt) = &other.payout {
            if next.add_payout(receipt.clone())? { added += 1; }
        }
        if let Some(receipt) = &other.settlement {
            if next.add_settlement(receipt.clone())? { added += 1; }
        }
        *self = next;
        Ok(added)
    }
}
