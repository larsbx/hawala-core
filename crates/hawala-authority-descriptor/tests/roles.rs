//! ADR 0005 gap G5, remainder: execution observers and settlement attestors
//! are pinned with the same descriptor chain, under a role-specific scope, so
//! one role's pin can never vouch for another role's evidence.
use ed25519_dalek::SigningKey;
use hawala_authority_descriptor::*;
use hawala_identity_core::IdentityId;
use sha2::{Digest, Sha256};

const NETWORK: [u8; 32] = [1; 32];
const LEG_SCOPE: [u8; 32] = [2; 32];

fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}

fn id(n: u8) -> IdentityId {
    IdentityId::from_signing_key(&key(n))
}

fn envelope(k: u8, scope: [u8; 32], epoch: u64, predecessor: [u8; 32], next: u8) -> Vec<u8> {
    let d = Descriptor {
        network_id: NETWORK,
        scope_id: scope,
        authority_key: id(k),
        authority_epoch: epoch,
        predecessor_digest: predecessor,
        next_key_digest: key_commitment(&id(next)),
        custody_profile_digest: [3; 32],
        fencing_policy_digest: [4; 32],
        mandate_digest: [5; 32],
    };
    sign_descriptor(&key(k), d).unwrap().encode()
}

fn pin_for(role: Role, k: u8) -> (Pin, Vec<u8>) {
    let e0 = envelope(k, role_scope_id(role, LEG_SCOPE), 0, [0; 32], k + 1);
    let digest = SignedDescriptor::decode(&e0).unwrap().digest();
    (Pin::anchor(&e0, digest).unwrap(), e0)
}

#[test]
fn role_scopes_are_distinct_and_authorities_keep_the_manifest_scope() {
    assert_eq!(role_scope_id(Role::Authority, LEG_SCOPE), LEG_SCOPE);
    let observer = role_scope_id(Role::ExecutionObserver, LEG_SCOPE);
    let attestor = role_scope_id(Role::SettlementAttestor, LEG_SCOPE);
    assert!(observer != attestor && observer != LEG_SCOPE && attestor != LEG_SCOPE);
    let expected: [u8; 32] =
        Sha256::digest([b"hawala-role-scope/v1\0".as_slice(), &[1], &LEG_SCOPE].concat()).into();
    assert_eq!(observer, expected);
    assert_ne!(
        role_scope_id(Role::ExecutionObserver, [9; 32]),
        observer,
        "scopes are per leg"
    );
}

#[test]
fn an_observer_is_admitted_for_its_role_leg_key_and_current_epoch() {
    let (pin, e0) = pin_for(Role::ExecutionObserver, 20);
    assert_eq!(
        pin.admit_issuer(Role::ExecutionObserver, LEG_SCOPE, &id(20), 0),
        Ok(())
    );
    let e1 = envelope(
        21,
        role_scope_id(Role::ExecutionObserver, LEG_SCOPE),
        1,
        SignedDescriptor::decode(&e0).unwrap().digest(),
        22,
    );
    let pin1 = pin.advance(&e1).unwrap();
    assert_eq!(
        pin1.admit_issuer(Role::ExecutionObserver, LEG_SCOPE, &id(21), 1),
        Ok(())
    );
    assert_eq!(
        pin1.admit_issuer(Role::ExecutionObserver, LEG_SCOPE, &id(20), 0),
        Err(Error::WrongEpoch)
    );
    assert_eq!(
        pin1.admit_issuer(Role::ExecutionObserver, LEG_SCOPE, &id(20), 1),
        Err(Error::WrongIssuer)
    );
}

#[test]
fn no_pin_vouches_for_another_role_or_leg() {
    let (observer, _) = pin_for(Role::ExecutionObserver, 20);
    let (authority, _) = pin_for(Role::Authority, 30);
    assert_eq!(
        observer.admit_issuer(Role::SettlementAttestor, LEG_SCOPE, &id(20), 0),
        Err(Error::WrongScope)
    );
    assert_eq!(
        observer.admit_issuer(Role::ExecutionObserver, [9; 32], &id(20), 0),
        Err(Error::WrongScope)
    );
    assert_eq!(
        authority.admit_issuer(Role::ExecutionObserver, LEG_SCOPE, &id(30), 0),
        Err(Error::WrongScope)
    );
    assert_eq!(
        authority.admit_issuer(Role::Authority, LEG_SCOPE, &id(30), 0),
        Ok(())
    );
    let leg = Leg {
        network_id: NETWORK,
        authority_key: id(20),
        scope_id: LEG_SCOPE,
        authority_epoch: 0,
        custody_profile_digest: [3; 32],
    };
    assert_eq!(
        observer.admit_leg(&leg),
        Err(Error::WrongScope),
        "an observer pin is never a leg authority"
    );
}
