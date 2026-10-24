use std::collections::BTreeSet;
use std::sync::Mutex;
use ed25519_dalek::SigningKey;
use hawala_identity_core::{
    sign_endpoint_proof, verify_endpoint_proof, EndpointChallenge, Error,
    IdentityId, ReplayGuard, MAX_SESSION_PROOF_SECS,
};

fn key(seed: u8) -> SigningKey { SigningKey::from_bytes(&[seed; 32]) }
fn identity(seed: u8) -> IdentityId { IdentityId::from_signing_key(&key(seed)) }

#[derive(Default)]
struct AtomicTestGuard(Mutex<BTreeSet<(IdentityId, IdentityId, [u8; 32])>>);
impl ReplayGuard for AtomicTestGuard {
    fn consume_once(&self, verifier: IdentityId, claimant: IdentityId, nonce: [u8; 32])
        -> hawala_identity_core::Result<bool>
    {
        Ok(self.0.lock().unwrap().insert((verifier, claimant, nonce)))
    }
}

fn challenge() -> EndpointChallenge {
    EndpointChallenge {
        verifier: identity(2),
        subject: identity(1),
        endpoint_key: [17; 32], // Test fixture: in production obtain from Iroh TLS
        nonce: [33; 32], // Test fixture: MUST be cryptographically random in production
        purpose: "peer-auth".into(),
        issued_at: 1_000,
        expires_at: 1_120,
    }
}

#[test]
fn signed_qr_identity_is_bound_to_specific_iroh_endpoint_and_nonce() {
    let c = challenge();
    let proof = sign_endpoint_proof(&key(1), [17; 32], &c).unwrap();
    let guard = AtomicTestGuard::default();
    let verified = verify_endpoint_proof(identity(1), [17; 32], &c, &proof, 1_050, &guard).unwrap();
    assert_eq!(verified.identity(), identity(1));
    assert_eq!(verified.endpoint_key(), [17; 32]);
    assert_eq!(verified.expires_at(), 1_120);
    assert_eq!(verify_endpoint_proof(identity(1), [17; 32], &c, &proof, 1_050, &guard), Err(Error::Replay));
}

#[test]
fn refuses_endpoint_swaps_wrong_scans_and_unowned_endpoint_signing() {
    let c = challenge();
    let proof = sign_endpoint_proof(&key(1), [17; 32], &c).unwrap();
    let fresh = || AtomicTestGuard::default();
    assert_eq!(sign_endpoint_proof(&key(1), [18; 32], &c).err(), Some(Error::PermissionDenied));
    assert_eq!(sign_endpoint_proof(&key(3), [17; 32], &c).err(), Some(Error::PermissionDenied));
    assert_eq!(
        verify_endpoint_proof(identity(1), [18; 32], &c, &proof, 1_050, &fresh()),
        Err(Error::PermissionDenied)
    );
    assert_eq!(
        verify_endpoint_proof(identity(3), [17; 32], &c, &proof, 1_050, &fresh()),
        Err(Error::PermissionDenied)
    );
    let altered = EndpointChallenge { endpoint_key: [18; 32], ..c.clone() };
    assert_eq!(
        verify_endpoint_proof(identity(1), [18; 32], &altered, &proof, 1_050, &fresh()),
        Err(Error::BadSignature)
    );
}

#[test]
fn rejects_modified_context_expiry_future_and_long_sessions() {
    let c = challenge();
    let proof = sign_endpoint_proof(&key(1), [17; 32], &c).unwrap();
    let fresh = || AtomicTestGuard::default();
    let changed = EndpointChallenge { purpose: "withdraw-cash".into(), ..c.clone() };
    assert_eq!(
        verify_endpoint_proof(identity(1), [17; 32], &changed, &proof, 1_050, &fresh()),
        Err(Error::BadSignature)
    );
    assert_eq!(
        verify_endpoint_proof(identity(1), [17; 32], &c, &proof, 999, &fresh()),
        Err(Error::NotYetValid)
    );
    assert_eq!(
        verify_endpoint_proof(identity(1), [17; 32], &c, &proof, 1_120, &fresh()),
        Err(Error::Expired)
    );
    let long = EndpointChallenge { expires_at: 1_000 + MAX_SESSION_PROOF_SECS + 1, ..c };
    assert_eq!(sign_endpoint_proof(&key(1), [17; 32], &long).err(), Some(Error::InvalidInput));
}
