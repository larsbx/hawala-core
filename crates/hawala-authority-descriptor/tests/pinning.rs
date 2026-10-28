//! ADR 0005 gap G5: authority discovery and trust pinning. Discovery delivers
//! bytes; only a locally pinned anchor and the pre-committed succession chain
//! confer trust. Nothing here is revoked or erased (ADR 0001).
use ed25519_dalek::{Signer, SigningKey};
use hawala_authority_descriptor::*;
use hawala_identity_core::IdentityId;
use sha2::{Digest, Sha256};

const NETWORK: [u8; 32] = [1; 32];
const SCOPE: [u8; 32] = [2; 32];
const CUSTODY: [u8; 32] = [3; 32];

fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}

fn id(n: u8) -> IdentityId {
    IdentityId::from_signing_key(&key(n))
}

/// Descriptor for `epoch` held by key `k`, committing to successor key `next`.
fn descriptor(k: u8, epoch: u64, predecessor: [u8; 32], next: Option<u8>) -> Descriptor {
    Descriptor {
        network_id: NETWORK,
        scope_id: SCOPE,
        authority_key: id(k),
        authority_epoch: epoch,
        predecessor_digest: predecessor,
        next_key_digest: next.map_or(RETIRED, |n| key_commitment(&id(n))),
        custody_profile_digest: CUSTODY,
        fencing_policy_digest: [4; 32],
        mandate_digest: [5; 32],
    }
}

fn envelope(k: u8, d: Descriptor) -> Vec<u8> {
    sign_descriptor(&key(k), d).unwrap().encode()
}

/// Keys 10 -> 11 -> 12, each epoch pre-committing the next key.
fn chain() -> [Vec<u8>; 3] {
    let e0 = envelope(10, descriptor(10, 0, [0; 32], Some(11)));
    let e1 = envelope(11, descriptor(11, 1, digest(&e0), Some(12)));
    let e2 = envelope(12, descriptor(12, 2, digest(&e1), Some(13)));
    [e0, e1, e2]
}

fn digest(envelope: &[u8]) -> [u8; 32] {
    SignedDescriptor::decode(envelope).unwrap().digest()
}

fn anchored() -> (Pin, [Vec<u8>; 3]) {
    let c = chain();
    (Pin::anchor(&c[0], digest(&c[0])).unwrap(), c)
}

#[test]
fn envelope_is_fixed_length_hawala_domain_bytes_and_digest_is_sha256() {
    let e = &chain()[0];
    assert_eq!(e.len(), ENVELOPE_LEN);
    assert!(e.starts_with(b"hawala-authority-descriptor/v1\0"));
    assert_eq!(digest(e), <[u8; 32]>::from(Sha256::digest(e)));
}

#[test]
fn decoding_rejects_malformed_or_forged_bytes() {
    let e = chain()[0].clone();
    assert_eq!(
        SignedDescriptor::decode(&e[..e.len() - 1]).unwrap_err(),
        Error::Malformed
    );
    assert_eq!(
        SignedDescriptor::decode(&[e.as_slice(), &[0]].concat()).unwrap_err(),
        Error::Malformed
    );
    let mut wrong_domain = e.clone();
    wrong_domain[0] ^= 1;
    assert_eq!(
        SignedDescriptor::decode(&wrong_domain).unwrap_err(),
        Error::Malformed
    );
    let mut forged = e.clone();
    forged[ENVELOPE_LEN - 65] ^= 1; // last byte of the mandate digest
    assert_eq!(
        SignedDescriptor::decode(&forged).unwrap_err(),
        Error::InvalidSignature
    );
}

#[test]
fn descriptors_must_be_well_formed_to_be_signed() {
    let genesis_with_predecessor = descriptor(10, 0, [9; 32], Some(11));
    assert_eq!(
        sign_descriptor(&key(10), genesis_with_predecessor).unwrap_err(),
        Error::InvalidDescriptor
    );
    let orphan = descriptor(10, 1, [0; 32], Some(11));
    assert_eq!(
        sign_descriptor(&key(10), orphan).unwrap_err(),
        Error::InvalidDescriptor
    );
    let self_successor = descriptor(10, 0, [0; 32], Some(10));
    assert_eq!(
        sign_descriptor(&key(10), self_successor).unwrap_err(),
        Error::InvalidDescriptor
    );
    assert_eq!(
        sign_descriptor(&key(99), descriptor(10, 0, [0; 32], Some(11))).unwrap_err(),
        Error::WrongSigner
    );
}

#[test]
fn only_an_operator_pinned_digest_anchors_trust() {
    let [e0, ..] = chain();
    let discovered = envelope(66, descriptor(66, 0, [0; 32], Some(67)));
    assert_eq!(
        Pin::anchor(&discovered, digest(&e0)).unwrap_err(),
        Error::AnchorMismatch
    );
    let pin = Pin::anchor(&e0, digest(&e0)).unwrap();
    assert_eq!(pin.current().authority_key, id(10));
    assert_eq!(pin.current().authority_epoch, 0);
}

#[test]
fn follows_pre_committed_succession_epoch_by_epoch() {
    let (pin, [_, e1, e2]) = anchored();
    let pin1 = pin.advance(&e1).unwrap();
    let pin2 = pin1.advance(&e2).unwrap();
    assert_eq!(pin2.current().authority_key, id(12));
    assert_eq!(pin2.current().authority_epoch, 2);
    assert_eq!(
        pin.current().authority_epoch,
        0,
        "advance never mutates the prior pin"
    );
}

#[test]
fn a_stolen_current_key_cannot_redirect_succession() {
    let (pin, [e0, ..]) = anchored();
    // An attacker holding key 10 names its own key 66 as the epoch-1 authority.
    let hijack = envelope(66, descriptor(66, 1, digest(&e0), Some(67)));
    assert_eq!(
        pin.advance(&hijack).unwrap_err(),
        Error::UncommittedSuccessor
    );
    // Key 10 cannot sign the epoch-1 descriptor in the name of committed key 11.
    let named_11 = descriptor(11, 1, digest(&e0), Some(12));
    let signed_by_10 = key(10).sign(&named_11.signing_input()).to_bytes();
    let by_old_key = [named_11.signing_input(), signed_by_10.to_vec()].concat();
    assert_eq!(
        pin.advance(&by_old_key).unwrap_err(),
        Error::InvalidSignature
    );
}

#[test]
fn successors_must_link_to_the_pinned_predecessor_in_order() {
    let (pin, [_, e1, e2]) = anchored();
    let unlinked = envelope(11, descriptor(11, 1, [7; 32], Some(12)));
    assert_eq!(pin.advance(&unlinked).unwrap_err(), Error::BrokenChain);
    assert_eq!(pin.advance(&e2).unwrap_err(), Error::Gap);
    let mut elsewhere = descriptor(11, 1, digest(&chain()[0]), Some(12));
    elsewhere.scope_id = [8; 32];
    assert_eq!(
        pin.advance(&envelope(11, elsewhere)).unwrap_err(),
        Error::WrongScope
    );
    let mut other_network = descriptor(11, 1, digest(&chain()[0]), Some(12));
    other_network.network_id = [8; 32];
    assert_eq!(
        pin.advance(&envelope(11, other_network)).unwrap_err(),
        Error::WrongNetwork
    );
    assert!(pin.advance(&e1).is_ok());
}

#[test]
fn replaying_known_descriptors_is_idempotent() {
    let (pin, [e0, e1, _]) = anchored();
    let pin1 = pin.advance(&e1).unwrap();
    assert_eq!(pin1.advance(&e1).unwrap(), pin1);
    assert_eq!(
        pin1.advance(&e0).unwrap(),
        pin1,
        "an accepted historical epoch stays evidence"
    );
}

#[test]
fn equivocation_at_any_known_epoch_is_a_conflict_that_retains_both_digests() {
    let (pin, [e0, e1, _]) = anchored();
    let pin1 = pin.advance(&e1).unwrap();
    let mut rival = descriptor(11, 1, digest(&e0), Some(12));
    rival.custody_profile_digest = [0xCC; 32];
    let rival = envelope(11, rival);
    assert_eq!(
        pin1.advance(&rival).unwrap_err(),
        Error::Conflict {
            epoch: 1,
            pinned: digest(&e1),
            presented: digest(&rival)
        }
    );
    let rival_genesis = envelope(10, descriptor(10, 0, [0; 32], Some(14)));
    assert_eq!(
        pin1.advance(&rival_genesis).unwrap_err(),
        Error::Conflict {
            epoch: 0,
            pinned: digest(&e0),
            presented: digest(&rival_genesis)
        }
    );
}

#[test]
fn a_retired_scope_accepts_no_successor() {
    let e0 = envelope(10, descriptor(10, 0, [0; 32], None));
    let pin = Pin::anchor(&e0, digest(&e0)).unwrap();
    let e1 = envelope(11, descriptor(11, 1, digest(&e0), Some(12)));
    assert_eq!(pin.advance(&e1).unwrap_err(), Error::Retired);
}

#[test]
fn manifest_legs_must_match_the_current_pinned_descriptor_exactly() {
    let (pin, [_, e1, _]) = anchored();
    let pin1 = pin.advance(&e1).unwrap();
    let leg = |network, authority, scope, epoch, custody| {
        pin1.admit_leg(&Leg {
            network_id: network,
            authority_key: authority,
            scope_id: scope,
            authority_epoch: epoch,
            custody_profile_digest: custody,
        })
    };
    assert_eq!(leg(NETWORK, id(11), SCOPE, 1, CUSTODY), Ok(()));
    assert_eq!(
        leg(NETWORK, id(10), SCOPE, 0, CUSTODY),
        Err(Error::WrongEpoch)
    );
    assert_eq!(
        leg(NETWORK, id(10), SCOPE, 1, CUSTODY),
        Err(Error::WrongAuthority)
    );
    assert_eq!(
        leg(NETWORK, id(11), [8; 32], 1, CUSTODY),
        Err(Error::WrongScope)
    );
    assert_eq!(
        leg([8; 32], id(11), SCOPE, 1, CUSTODY),
        Err(Error::WrongNetwork)
    );
    assert_eq!(
        leg(NETWORK, id(11), SCOPE, 1, [8; 32]),
        Err(Error::CustodyProfileMismatch)
    );
}
