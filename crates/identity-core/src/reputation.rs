use std::collections::BTreeSet;

use ed25519_dalek::SigningKey;

use crate::identity::IdentityId;
use crate::{check_signature, encode, signature, valid_label, Error, Result};

/// Event ID is opaque and must not disclose customer names, amounts, or ledger
/// references. A mutually signed receipt attests that both participants agree
/// on an interaction; it does NOT establish legal settlement or prevent collusion.
#[derive(Clone, Debug)]
pub struct Interaction {
    pub id: [u8; 32],
    pub domain: String,
    pub party_a: IdentityId,
    pub party_b: IdentityId,
    pub completed_at: u64,
}

#[derive(Clone, Debug)]
pub struct SignedInteraction {
    pub interaction: Interaction,
    pub signature_a: [u8; 64],
    pub signature_b: [u8; 64],
}

#[derive(Clone, Debug)]
pub struct Rating {
    pub interaction_id: [u8; 32],
    pub domain: String,
    pub rater: IdentityId,
    pub subject: IdentityId,
    /// An explicit domain-local 1..=5 scale, NOT financial trust or credit.
    pub stars: u8,
    pub created_at: u64,
}

#[derive(Clone, Debug)]
pub struct SignedRating {
    pub rating: Rating,
    pub signature: [u8; 64],
}

/// No global score; the evidence set and domain determine the result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReputationSummary {
    pub rating_count: u64,
    pub star_sum: u64,
}

fn receipt_message(c: &Interaction) -> Vec<u8> {
    encode(
        b"completed-interaction",
        &[
            &c.id,
            c.domain.as_bytes(),
            c.party_a.as_bytes(),
            c.party_b.as_bytes(),
            &c.completed_at.to_be_bytes(),
        ],
    )
}

fn rating_message(r: &Rating) -> Vec<u8> {
    encode(
        b"interaction-rating",
        &[
            &r.interaction_id,
            r.domain.as_bytes(),
            r.rater.as_bytes(),
            r.subject.as_bytes(),
            &[r.stars],
            &r.created_at.to_be_bytes(),
        ],
    )
}

fn validate_interaction(i: &Interaction) -> Result<()> {
    if i.id == [0; 32] || !valid_label(&i.domain) || i.party_a == i.party_b {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

fn validate_rating(r: &Rating) -> Result<()> {
    if r.interaction_id == [0; 32]
        || !valid_label(&r.domain)
        || !(1..=5).contains(&r.stars)
        || r.rater == r.subject
    {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

/// Each counterparty signs the exact SAME interaction object independently.
/// The caller assembles the two signatures into SignedInteraction.
pub fn sign_receipt(key: &SigningKey, interaction: &Interaction) -> Result<[u8; 64]> {
    validate_interaction(interaction)?;
    let signer = IdentityId::from_signing_key(key);
    if signer != interaction.party_a && signer != interaction.party_b {
        return Err(Error::PermissionDenied);
    }
    Ok(signature(key, &receipt_message(interaction)))
}

pub fn sign_rating(key: &SigningKey, rating: Rating) -> Result<SignedRating> {
    validate_rating(&rating)?;
    if IdentityId::from_signing_key(key) != rating.rater {
        return Err(Error::PermissionDenied);
    }
    Ok(SignedRating {
        signature: signature(key, &rating_message(&rating)),
        rating,
    })
}

fn verify_interaction(i: &SignedInteraction) -> Result<()> {
    validate_interaction(&i.interaction)?;
    let message = receipt_message(&i.interaction);
    check_signature(&i.interaction.party_a, &message, &i.signature_a)?;
    check_signature(&i.interaction.party_b, &message, &i.signature_b)
}

/// Strictly evaluates presented evidence; malformed, forged or duplicated
/// ratings cause failure, never silent inclusion. No weighted global ranking.
pub fn evaluate_reputation(
    subject: IdentityId,
    domain: &str,
    evidence: &[(SignedInteraction, SignedRating)],
    now: u64,
) -> Result<ReputationSummary> {
    if !valid_label(domain) {
        return Err(Error::InvalidInput);
    }
    let mut seen = BTreeSet::new();
    let mut result = ReputationSummary::default();

    for (receipt, signed_rating) in evidence {
        verify_interaction(receipt)?;
        let i = &receipt.interaction;
        let r = &signed_rating.rating;
        validate_rating(r)?;
        if i.domain != domain || r.domain != domain || r.subject != subject
            || i.id != r.interaction_id
            || !((i.party_a == r.rater && i.party_b == r.subject)
                || (i.party_b == r.rater && i.party_a == r.subject))
        {
            return Err(Error::PermissionDenied);
        }
        if r.created_at < i.completed_at || now < r.created_at {
            return Err(Error::NotYetValid);
        }
        check_signature(&r.rater, &rating_message(r), &signed_rating.signature)?;

        if !seen.insert((i.id, r.rater, r.subject)) {
            return Err(Error::DuplicateRating);
        }
        result.rating_count += 1;
        result.star_sum += u64::from(r.stars);
    }
    Ok(result)
}
