//! Golden vectors consumed by biz-kit as independent Elixir conformance tests.
//! These prove byte-level interoperability, not identity grants or settlement authority.
use ed25519_dalek::{Signature, SigningKey};
use hawala_identity_core::{
    issue_credential, sign_challenge, Challenge, CredentialClaim, IdentityId, VerificationKind,
};
use serde_json::Value;

fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}

fn id(n: u8) -> IdentityId {
    IdentityId::from_signing_key(&key(n))
}

fn vector(name: &str) -> Value {
    let document: Value =
        serde_json::from_str(include_str!("../test-vectors/bizkit_identity_v1.json")).unwrap();
    assert_eq!(document["schema"], "hawala-identity-core-interoperability-v1");
    document["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == name)
        .expect("missing named golden vector")
        .clone()
}

fn raw(v: &Value, field: &str) -> Vec<u8> {
    hex::decode(v[field].as_str().expect("missing hex field")).unwrap()
}

fn signed_input(domain: &[u8], parts: &[&[u8]]) -> Vec<u8> {
    // Independent test-side construction of identity-core's length-prefixed wire format.
    let mut result = b"hawala-identity-core/v1\0".to_vec();
    result.extend_from_slice(&(domain.len() as u32).to_be_bytes());
    result.extend_from_slice(domain);
    for part in parts {
        result.extend_from_slice(&(part.len() as u32).to_be_bytes());
        result.extend_from_slice(part);
    }
    result
}

fn verify_vector_signature(v: &Value, message: &[u8]) {
    let pk: [u8; 32] = raw(v, "public_key_hex").try_into().unwrap();
    let sig: [u8; 64] = raw(v, "signature_hex").try_into().unwrap();
    let public = ed25519_dalek::VerifyingKey::from_bytes(&pk).unwrap();
    assert!(public.verify_strict(message, &Signature::from_bytes(&sig)).is_ok());
    let mut changed = message.to_vec();
    let last = changed.len() - 1;
    changed[last] ^= 1;
    assert!(public.verify_strict(&changed, &Signature::from_bytes(&sig)).is_err());
}

fn assert_disjoint_from_bizkit_map(message: &[u8]) {
    // Raw ASCII h (0x68), NOT a CBOR text-string header in Hawala's wire protocol.
    assert!(message.starts_with(b"hawala-identity-core/v1\0"));
    assert_eq!(message[0], 0x68);
    assert!(!(0xA0..=0xBB).contains(&message[0]));
}

#[test]
fn possession_challenge_matches_shared_golden_vector() {
    let v = vector("possession-challenge");
    assert_eq!(raw(&v, "seed_hex"), vec![1u8; 32]);
    assert_eq!(raw(&v, "public_key_hex").as_slice(), id(1).as_bytes());
    assert_eq!(raw(&v, "verifier_key_hex").as_slice(), id(2).as_bytes());

    let challenge = Challenge {
        verifier: id(2),
        nonce: [3; 32],
        purpose: v["purpose"].as_str().unwrap().to_owned(),
        expires_at: v["expires_at"].as_u64().unwrap(),
    };
    assert_eq!(raw(&v, "nonce_hex"), challenge.nonce.to_vec());
    let signed = sign_challenge(&key(1), &challenge).unwrap();
    assert_eq!(signed.claimant, id(1));
    assert_eq!(signed.signature.to_vec(), raw(&v, "signature_hex"));
    assert_eq!(id(1).qr_payload(), v["qr_payload"].as_str().unwrap());

    let expiry = challenge.expires_at.to_be_bytes();
    let message = signed_input(
        b"identity-possession-challenge",
        &[
            signed.claimant.as_bytes(),
            challenge.verifier.as_bytes(),
            &challenge.nonce,
            challenge.purpose.as_bytes(),
            &expiry,
        ],
    );
    assert_eq!(v["domain"], "identity-possession-challenge");
    assert_eq!(message, raw(&v, "message_hex"));
    assert_disjoint_from_bizkit_map(&message);
    verify_vector_signature(&v, &message);
}

#[test]
fn merchant_credential_matches_shared_golden_vector() {
    let v = vector("merchant-credential");
    assert_eq!(raw(&v, "seed_hex"), vec![4u8; 32]);
    assert_eq!(raw(&v, "public_key_hex").as_slice(), id(4).as_bytes());
    assert_eq!(raw(&v, "subject_key_hex").as_slice(), id(1).as_bytes());
    assert_eq!(raw(&v, "credential_id_hex"), vec![9u8; 16]);
    assert_eq!(v["kind"], "merchant");
    assert_eq!(v["scope"], "US-PA");
    let claim = CredentialClaim {
        credential_id: [9; 16],
        issuer: id(4),
        subject: id(1),
        kind: VerificationKind::Merchant,
        scope: v["scope"].as_str().unwrap().to_owned(),
        issued_at: v["issued_at"].as_u64().unwrap(),
        expires_at: v["expires_at"].as_u64().unwrap(),
    };
    let signed = issue_credential(&key(4), claim).unwrap();
    assert_eq!(signed.signature.to_vec(), raw(&v, "signature_hex"));
    let start = signed.claim.issued_at.to_be_bytes();
    let expiry = signed.claim.expires_at.to_be_bytes();
    let message = signed_input(
        b"verification-credential",
        &[
            &signed.claim.credential_id,
            signed.claim.issuer.as_bytes(),
            signed.claim.subject.as_bytes(),
            b"merchant",
            signed.claim.scope.as_bytes(),
            &start,
            &expiry,
        ],
    );
    assert_eq!(v["domain"], "verification-credential");
    assert_eq!(message, raw(&v, "message_hex"));
    assert_disjoint_from_bizkit_map(&message);
    verify_vector_signature(&v, &message);
}
