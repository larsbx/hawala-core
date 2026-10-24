use ed25519_dalek::SigningKey;
use hawala_identity_core::{continuity::*, Error, IdentityId};

fn key(n: u8) -> SigningKey { SigningKey::from_bytes(&[n;32]) }
fn id(n: u8) -> IdentityId { IdentityId::from_signing_key(&key(n)) }
fn claim() -> ContinuityClaim {
    ContinuityClaim { predecessor: id(1), successor: id(2), scope: "test-peer".into(),
        context: [3;32], nonce: [4;32] }
}

#[test]
fn canonical_encoding_and_successor_acceptance() {
    let c = claim();
    let predecessor = id(1);
    let successor = id(2);
    let mut expected = b"hawala-identity-core/v1\0".to_vec();
    let parts: &[&[u8]] = &[b"continuity-claim-v1", predecessor.as_bytes(),
        successor.as_bytes(), b"test-peer", &[3;32], &[4;32]];
    for part in parts {
        expected.extend_from_slice(&(part.len() as u32).to_be_bytes());
        expected.extend_from_slice(part);
    }
    assert_eq!(c.canonical_bytes().unwrap(), expected);
    let old = endorse(&key(1), &c).unwrap();
    assert_eq!(verified_signers(&c, &[old.clone()]), Err(Error::PermissionDenied));
    let new = endorse(&key(2), &c).unwrap();
    assert_eq!(new.signature, endorse(&key(2), &c).unwrap().signature);
    assert_eq!(verified_signers(&c, &[old, new.clone(), new]).unwrap().len(), 2);
}

#[test]
fn lost_key_evidence_does_not_establish_witness_trust() {
    let c = claim();
    let e = [endorse(&key(2), &c).unwrap(), endorse(&key(5), &c).unwrap()];
    let signers = verified_signers(&c, &e).unwrap();
    assert!(!signers.contains(&id(1)));
    assert!(signers.contains(&id(5))); // Policy trust in key 5 is NOT established.
    let mut forged = e[1].clone(); forged.signer = id(6);
    assert_eq!(verify_endorsement(&c, &forged), Err(Error::BadSignature));
    for field in 0..4 {
        let mut changed = c.clone();
        match field {
            0 => changed.scope = "other-peer".into(),
            1 => changed.context = [8;32],
            2 => changed.successor = id(7),
            _ => changed.nonce = [9;32],
        }
        assert_eq!(verify_endorsement(&changed, &e[0]), Err(Error::BadSignature));
    }
}

#[test]
fn thief_with_old_key_can_sign_competing_successors_in_either_order() {
    let a = claim();
    let mut b = a.clone(); b.successor = id(7);
    assert!(verify_endorsement(&a, &endorse(&key(1), &a).unwrap()).is_ok());
    assert!(verify_endorsement(&b, &endorse(&key(1), &b).unwrap()).is_ok());
    assert_eq!(compare(&a, &b), Ok(ClaimRelation::Competing));
    assert_eq!(compare(&b, &a), Ok(ClaimRelation::Competing));
    assert_eq!(compare(&a, &a), Ok(ClaimRelation::Identical));
    b = a.clone(); b.context = [9;32];
    assert_eq!(compare(&a, &b), Ok(ClaimRelation::Competing));
    b = a.clone(); b.nonce = [9;32];
    assert_eq!(compare(&a, &b), Ok(ClaimRelation::Competing));
    b.scope = "other-peer".into();
    assert_eq!(compare(&a, &b), Ok(ClaimRelation::Distinct));
}

#[test]
fn malformed_claims_cannot_be_signed() {
    let mut c = claim(); c.successor = c.predecessor;
    assert_eq!(c.canonical_bytes(), Err(Error::InvalidInput));
    c = claim(); c.context = [0;32];
    assert_eq!(c.canonical_bytes(), Err(Error::InvalidInput));
    c = claim(); c.nonce = [0;32];
    assert_eq!(c.canonical_bytes(), Err(Error::InvalidInput));
    c = claim(); c.scope = "".into();
    assert!(matches!(endorse(&key(1), &c), Err(Error::InvalidInput)));
}
