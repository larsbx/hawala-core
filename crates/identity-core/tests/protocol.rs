use std::collections::HashSet;
use std::sync::Mutex;

use ed25519_dalek::SigningKey;
use hawala_identity_core::{
    evaluate_reputation, issue_credential, sign_challenge, sign_rating, sign_receipt,
    verify_and_consume_challenge, verify_credential, Challenge, CredentialClaim, Error,
    IdentityId, Interaction, Rating, ReplayGuard, SignedInteraction, VerificationKind,
    VerificationPolicy,
};

fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}

fn id(n: u8) -> IdentityId {
    IdentityId::from_signing_key(&key(n))
}

#[derive(Default)]
struct DurableGuardForTest(Mutex<HashSet<(IdentityId, IdentityId, [u8; 32])>>);

impl ReplayGuard for DurableGuardForTest {
    fn consume_once(&self, verifier: IdentityId, claimant: IdentityId, nonce: [u8; 32]) -> hawala_identity_core::Result<bool> {
        Ok(self.0.lock().unwrap().insert((verifier, claimant, nonce)))
    }
}

#[test]
fn qr_is_an_identifier_not_a_bearer_token() {
    let qr = id(1).qr_payload();
    assert_eq!(IdentityId::parse_qr(&qr), Ok(id(1)));
    assert_eq!(IdentityId::parse_qr("hawala:id:v2:0000"), Err(Error::InvalidQr));
    assert_eq!(IdentityId::parse_qr("https://example.com?id=1"), Err(Error::InvalidQr));
}

#[test]
fn qr_is_always_presented_with_a_legible_identity_code_and_optional_name() {
    let person = id(1);
    let card = person.presentation(Some("Cedar Market")).unwrap();
    assert_eq!(card.display_name.as_deref(), Some("Cedar Market"));
    assert!(card.identity_code.starts_with("HW-"));
    assert_eq!(card.identity_code.len(), 17);
    assert_eq!(IdentityId::parse_qr(&card.qr_payload), Ok(person));
    assert_eq!(
        IdentityId::parse_qr(&card.qr_payload).unwrap().human_code(),
        card.identity_code
    );
    assert_ne!(id(2).human_code(), card.identity_code);

    let anonymous = person.presentation(None).unwrap();
    assert_eq!(anonymous.display_name, None);
    assert_eq!(anonymous.identity_code, card.identity_code);
    assert_eq!(person.presentation(Some("")), Err(Error::InvalidInput));
    assert_eq!(person.presentation(Some(" Padded name ")), Err(Error::InvalidInput));
}

#[test]
fn possession_authentication_is_scoped_and_replay_protected() {
    let scanned = id(1);
    let c = Challenge {
        verifier: id(2),
        nonce: [3; 32],
        purpose: "cash-withdrawal:TX-123".into(),
        expires_at: 120,
    };
    let response = sign_challenge(&key(1), &c).unwrap();
    let guard = DurableGuardForTest::default();
    assert_eq!(
        verify_and_consume_challenge(scanned, &c, &response, 110, &guard),
        Ok(())
    );
    assert_eq!(
        verify_and_consume_challenge(scanned, &c, &response, 110, &guard),
        Err(Error::Replay)
    );
    assert_eq!(
        verify_and_consume_challenge(id(4), &c, &response, 110, &guard),
        Err(Error::PermissionDenied)
    );
    let changed_purpose = Challenge { purpose: "gift-card".into(), ..c.clone() };
    assert_eq!(
        verify_and_consume_challenge(scanned, &changed_purpose, &response, 110, &DurableGuardForTest::default()),
        Err(Error::BadSignature)
    );
    assert_eq!(
        verify_and_consume_challenge(scanned, &c, &response, 120, &DurableGuardForTest::default()),
        Err(Error::Expired)
    );
}

fn issued_credential() -> hawala_identity_core::SignedCredential {
    issue_credential(
        &key(4),
        CredentialClaim {
            credential_id: [9; 16],
            issuer: id(4),
            subject: id(1),
            kind: VerificationKind::Merchant,
            scope: "US-PA".into(),
            issued_at: 100,
            expires_at: 200,
        },
    )
    .unwrap()
}

#[test]
fn credentials_require_exact_authority_scope_validity_and_signature() {
    let c = issued_credential();
    let mut policy = VerificationPolicy::new();
    policy.authorize_issuer(VerificationKind::Merchant, "US-PA", id(4)).unwrap();
    assert_eq!(verify_credential(&c, id(1), VerificationKind::Merchant, "US-PA", &policy, 110), Ok(()));
    assert_eq!(verify_credential(&c, id(1), VerificationKind::Merchant, "US-NY", &policy, 110), Err(Error::PermissionDenied));
    assert_eq!(verify_credential(&c, id(1), VerificationKind::Hawaladar, "US-PA", &policy, 110), Err(Error::PermissionDenied));
    assert_eq!(verify_credential(&c, id(2), VerificationKind::Merchant, "US-PA", &policy, 110), Err(Error::PermissionDenied));
    assert_eq!(verify_credential(&c, id(1), VerificationKind::Merchant, "US-PA", &policy, 99), Err(Error::NotYetValid));
    assert_eq!(verify_credential(&c, id(1), VerificationKind::Merchant, "US-PA", &policy, 200), Err(Error::Expired));
    assert_eq!(verify_credential(&c, id(1), VerificationKind::Merchant, "US-PA", &VerificationPolicy::new(), 110), Err(Error::UntrustedIssuer));
    let mut tampered = issued_credential();
    tampered.claim.expires_at = 201;
    assert_eq!(verify_credential(&tampered, id(1), VerificationKind::Merchant, "US-PA", &policy, 110), Err(Error::BadSignature));
}

#[test]
fn ratings_need_mutually_signed_completed_interaction() {
    let interaction = Interaction {
        id: [8; 32],
        domain: "merchant".into(),
        party_a: id(1),
        party_b: id(2),
        completed_at: 100,
    };
    let signed_receipt = SignedInteraction {
        signature_a: sign_receipt(&key(1), &interaction).unwrap(),
        signature_b: sign_receipt(&key(2), &interaction).unwrap(),
        interaction,
    };
    let rating = sign_rating(
        &key(1),
        Rating {
            interaction_id: [8; 32],
            domain: "merchant".into(),
            rater: id(1),
            subject: id(2),
            stars: 5,
            created_at: 110,
        },
    )
    .unwrap();
    let evidence = vec![(signed_receipt.clone(), rating.clone())];
    let summary = evaluate_reputation(id(2), "merchant", &evidence, 120).unwrap();
    assert_eq!(summary.rating_count, 1);
    assert_eq!(summary.star_sum, 5);
    assert_eq!(evaluate_reputation(id(2), "hawala", &evidence, 120), Err(Error::PermissionDenied));
    assert_eq!(evaluate_reputation(id(2), "merchant", &[evidence[0].clone(), evidence[0].clone()], 120), Err(Error::DuplicateRating));

    let mut tampered = signed_receipt.clone();
    tampered.interaction.completed_at = 99;
    assert_eq!(evaluate_reputation(id(2), "merchant", &[(tampered, rating.clone())], 120), Err(Error::BadSignature));

    let mut forged = rating;
    forged.rating.stars = 1;
    assert_eq!(evaluate_reputation(id(2), "merchant", &[(signed_receipt, forged)], 120), Err(Error::BadSignature));
}
