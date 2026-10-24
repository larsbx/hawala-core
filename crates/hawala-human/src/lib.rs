//! A signed cash-handoff observation between beneficiary and disbursing
//! hawaladar, sharing the same obligation+attempt binding as an ATM report.
//!
//! This records what two identified keys assert. It cannot prove that notes
//! changed hands, that signers were physically present, or that a key's owner
//! is currently licensed/eligible or even a human. An approved authenticator,
//! fresh QR identity, legal eligibility and cash-inventory policy are separate.
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hawala_identity_core::IdentityId;
use hawala_protocol::SignedTerms;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    WrongTerms,
    WrongSigner,
    InvalidSignature,
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandoffClaim {
    pub transfer_id: [u8; 32],
    pub terms_digest: [u8; 32],
    pub attempt_id: [u8; 32],
    pub receipt_id: [u8; 32],
    pub beneficiary: IdentityId,
    pub disburser: IdentityId,
    pub payout_minor: u64,
    pub currency: [u8; 3],
    pub asserted_handoff_at: u64,
}

impl HandoffClaim {
    pub fn validate(&self, accepted: &SignedTerms) -> Result<()> {
        let terms = accepted.terms();
        if self.transfer_id == [0; 32]
            || self.terms_digest == [0; 32]
            || self.attempt_id == [0; 32]
            || self.receipt_id == [0; 32]
            || self.asserted_handoff_at == 0
            || self.beneficiary != terms.beneficiary
            || self.disburser != terms.disburser
            || self.transfer_id != terms.transfer_id
            || self.terms_digest != terms.digest()
            || self.payout_minor != terms.payout_minor
            || self.currency != terms.payout_currency
            || self.asserted_handoff_at < terms.accepted_at
        {
            return Err(Error::WrongTerms);
        }
        Ok(())
    }

    fn statement(&self) -> Vec<u8> {
        let mut out = b"hawala/human-cash-handoff/v1\0".to_vec();
        let amount = self.payout_minor.to_be_bytes();
        let when = self.asserted_handoff_at.to_be_bytes();
        let fields: [&[u8]; 9] = [
            self.transfer_id.as_slice(),
            self.terms_digest.as_slice(),
            self.attempt_id.as_slice(),
            self.receipt_id.as_slice(),
            self.beneficiary.as_bytes().as_slice(),
            self.disburser.as_bytes().as_slice(),
            amount.as_slice(),
            self.currency.as_slice(),
            when.as_slice(),
        ];
        for part in fields {
            out.extend_from_slice(&(part.len() as u32).to_be_bytes());
            out.extend_from_slice(part);
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedHandoff {
    claim: HandoffClaim,
    beneficiary_signature: [u8; 64],
    disburser_signature: [u8; 64],
}
impl SignedHandoff {
    pub fn claim(&self) -> &HandoffClaim { &self.claim }

    pub fn verify(&self, accepted: &SignedTerms) -> Result<()> {
        self.claim.validate(accepted)?;
        let message = self.claim.statement();
        check(self.claim.beneficiary, &message, &self.beneficiary_signature)?;
        check(self.claim.disburser, &message, &self.disburser_signature)?;
        Ok(())
    }
    pub fn digest(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new_derive_key("hawala/human-handoff-evidence/v1");
        h.update(&self.claim.statement());
        h.update(&self.beneficiary_signature);
        h.update(&self.disburser_signature);
        *h.finalize().as_bytes()
    }
}

fn check(id: IdentityId, msg: &[u8], sig: &[u8; 64]) -> Result<()> {
    let key = VerifyingKey::from_bytes(id.as_bytes()).map_err(|_| Error::InvalidSignature)?;
    key.verify_strict(msg, &Signature::from_bytes(sig))
        .map_err(|_| Error::InvalidSignature)
}

pub fn sign_handoff(
    key: &SigningKey,
    accepted: &SignedTerms,
    claim: &HandoffClaim,
) -> Result<[u8; 64]> {
    claim.validate(accepted)?;
    let signer = IdentityId::from_signing_key(key);
    if signer != claim.beneficiary && signer != claim.disburser {
        return Err(Error::WrongSigner);
    }
    Ok(key.sign(&claim.statement()).to_bytes())
}

pub fn assemble_handoff(
    accepted: &SignedTerms,
    claim: HandoffClaim,
    beneficiary_signature: [u8; 64],
    disburser_signature: [u8; 64],
) -> Result<SignedHandoff> {
    let receipt = SignedHandoff {
        claim, beneficiary_signature, disburser_signature,
    };
    receipt.verify(accepted)?;
    Ok(receipt)
}
