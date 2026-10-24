use ed25519_dalek::SigningKey;
use hawala_identity_core::IdentityId;
use hawala_protocol::{
    accept_terms, attest_payout, attest_settlement, sign_payout, sign_settlement,
    sign_terms, Error, ObligationEvidence, PayoutClaim, SettlementClaim,
    SignedPayout, SignedSettlement, SignedTerms, Terms,
};

fn key(n: u8) -> SigningKey { SigningKey::from_bytes(&[n; 32]) }
fn id(n: u8) -> IdentityId { IdentityId::from_signing_key(&key(n)) }

fn terms() -> Terms {
    Terms {
        transfer_id: [42; 32],
        sender: id(1),
        beneficiary: id(2),
        origin: id(3),
        disburser: id(4),
        funding_minor: 10_000,
        funding_currency: *b"USD",
        payout_minor: 8_600,
        payout_currency: *b"EUR",
        proposed_at: 100,
        accepted_at: 110,
        quote_expires_at: 120,
    }
}
fn accepted() -> SignedTerms {
    let t = terms();
    let o = sign_terms(&key(3), &t).unwrap();
    let d = sign_terms(&key(4), &t).unwrap();
    accept_terms(t, o, d).unwrap()
}
fn payout_for(a: &SignedTerms, receipt: u8) -> SignedPayout {
    let t = a.terms();
    let claim = PayoutClaim {
        transfer_id: t.transfer_id,
        terms_digest: t.digest(),
        receipt_id: [receipt; 32],
        payout_minor: t.payout_minor,
        payout_currency: t.payout_currency,
        paid_at: 200,
    };
    let d = sign_payout(&key(4), a, &claim).unwrap();
    let b = sign_payout(&key(2), a, &claim).unwrap();
    attest_payout(a, claim, d, b).unwrap()
}
fn settlement_for(a: &SignedTerms, receipt: u8) -> SignedSettlement {
    let t = a.terms();
    let claim = SettlementClaim {
        transfer_id: t.transfer_id,
        terms_digest: t.digest(),
        receipt_id: [receipt; 32],
        evidence_digest: [9; 32], // fixture: not evidence of actual asset transfer
        acknowledged_at: 170,
    };
    let o = sign_settlement(&key(3), a, &claim).unwrap();
    let d = sign_settlement(&key(4), a, &claim).unwrap();
    attest_settlement(a, claim, o, d).unwrap()
}

#[test]
fn accept_requires_both_agents_signing_exact_terms_and_fx() {
    let t = terms();
    let a = accepted();
    assert_eq!(a.terms(), &t);
    assert_eq!(a.terms().digest(), terms().digest());
    assert_eq!(sign_terms(&key(1), &t), Err(Error::WrongSigner));
    let mut tampered = t.clone();
    tampered.payout_minor += 1;
    assert!(matches!(
        accept_terms(tampered, sign_terms(&key(3), &t).unwrap(), sign_terms(&key(4), &t).unwrap()),
        Err(Error::InvalidSignature)
    ));
    let mut expired = t.clone();
    expired.accepted_at = 121;
    assert_eq!(sign_terms(&key(3), &expired), Err(Error::InvalidTerms));
    let mut bad_currency = t.clone();
    bad_currency.payout_currency = *b"eUR";
    assert_eq!(sign_terms(&key(3), &bad_currency), Err(Error::InvalidTerms));
    let mut same_party = t;
    same_party.beneficiary = same_party.disburser;
    assert_eq!(sign_terms(&key(3), &same_party), Err(Error::InvalidTerms));
}

#[test]
fn payout_requires_disburser_and_beneficiary_attestations() {
    let a = accepted();
    let claim = PayoutClaim {
        transfer_id: a.terms().transfer_id,
        terms_digest: a.terms().digest(),
        receipt_id: [8; 32],
        payout_minor: 8_600,
        payout_currency: *b"EUR",
        paid_at: 200,
    };
    assert_eq!(sign_payout(&key(1), &a, &claim), Err(Error::WrongSigner));
    let disburser = sign_payout(&key(4), &a, &claim).unwrap();
    let beneficiary = sign_payout(&key(2), &a, &claim).unwrap();
    let receipt = attest_payout(&a, claim.clone(), disburser, beneficiary).unwrap();
    assert_eq!(receipt.claim(), &claim);
    assert!(matches!(
        attest_payout(&a, claim.clone(), disburser, disburser),
        Err(Error::InvalidSignature)
    ));
    let mut wrong = claim;
    wrong.payout_minor = 8_599;
    assert_eq!(sign_payout(&key(4), &a, &wrong), Err(Error::InvalidClaim));
}

#[test]
fn payout_and_settlement_remain_independent_and_reconcile_offline() {
    let a = accepted();
    let payout = payout_for(&a, 5);
    let settlement = settlement_for(&a, 7);
    let mut peer_a = ObligationEvidence::new(a.clone()).unwrap();
    let mut peer_b = ObligationEvidence::new(a.clone()).unwrap();
    assert_eq!(peer_a.add_payout(payout.clone()), Ok(true));
    assert!(!peer_a.settlement_attested());
    assert!(peer_a.payout_attested());
    assert_eq!(peer_b.add_settlement(settlement.clone()), Ok(true));
    assert!(!peer_b.payout_attested());
    assert!(peer_b.settlement_attested());

    assert_eq!(peer_a.merge(&peer_b), Ok(1));
    assert_eq!(peer_b.merge(&peer_a), Ok(1));
    assert_eq!(peer_a, peer_b);
    assert_eq!(peer_a.merge(&peer_b), Ok(0));
    assert_eq!(peer_a.add_payout(payout), Ok(false));
    assert_eq!(peer_a.add_settlement(settlement), Ok(false));
}

#[test]
fn rejects_conflicting_payouts_and_does_not_partially_apply_merge() {
    let a = accepted();
    let mut left = ObligationEvidence::new(a.clone()).unwrap();
    let mut right = ObligationEvidence::new(a.clone()).unwrap();
    left.add_payout(payout_for(&a, 5)).unwrap();
    right.add_payout(payout_for(&a, 6)).unwrap();
    right.add_settlement(settlement_for(&a, 7)).unwrap();
    let before = left.clone();
    assert_eq!(left.merge(&right), Err(Error::ConflictingEvidence));
    assert_eq!(left, before);
    assert_eq!(left.add_payout(payout_for(&a, 6)), Err(Error::ConflictingEvidence));

    let mut other_terms = terms();
    other_terms.transfer_id = [41; 32];
    let so = sign_terms(&key(3), &other_terms).unwrap();
    let sd = sign_terms(&key(4), &other_terms).unwrap();
    let unrelated = ObligationEvidence::new(accept_terms(other_terms, so, sd).unwrap()).unwrap();
    assert_eq!(left.merge(&unrelated), Err(Error::DifferentObligation));
}

#[test]
fn settlement_requires_bilateral_signatures_and_specific_accepted_terms() {
    let a = accepted();
    let claim = SettlementClaim {
        transfer_id: a.terms().transfer_id,
        terms_digest: a.terms().digest(),
        receipt_id: [3; 32],
        evidence_digest: [2; 32],
        acknowledged_at: 300,
    };
    assert_eq!(sign_settlement(&key(2), &a, &claim), Err(Error::WrongSigner));
    let o = sign_settlement(&key(3), &a, &claim).unwrap();
    let d = sign_settlement(&key(4), &a, &claim).unwrap();
    assert!(attest_settlement(&a, claim.clone(), o, d).is_ok());
    assert!(matches!(attest_settlement(&a, claim.clone(), o, o), Err(Error::InvalidSignature)));
    let mut no_proof = claim;
    no_proof.evidence_digest = [0; 32];
    assert_eq!(sign_settlement(&key(3), &a, &no_proof), Err(Error::InvalidClaim));
}
