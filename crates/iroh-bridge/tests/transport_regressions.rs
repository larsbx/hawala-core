use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

use ed25519_dalek::SigningKey;
use hawala_identity_core::{
    sign_endpoint_proof, EndpointChallenge, Error, IdentityId, ReplayGuard,
};
use hawala_iroh_bridge::{handshake, verify_connected_peer, IDENTITY_ALPN};
use iroh::{
    endpoint::{presets, Connection, QuicTransportConfig},
    Endpoint, EndpointAddr,
};

const DEADLINE: Duration = Duration::from_secs(15);

fn key(value: u8) -> SigningKey {
    SigningKey::from_bytes(&[value; 32])
}

fn id(value: u8) -> IdentityId {
    IdentityId::from_signing_key(&key(value))
}

#[derive(Default)]
struct Nonces(Mutex<HashSet<(IdentityId, IdentityId, [u8; 32])>>);

impl ReplayGuard for Nonces {
    fn consume_once(
        &self,
        verifier: IdentityId,
        claimant: IdentityId,
        nonce: [u8; 32],
    ) -> hawala_identity_core::Result<bool> {
        Ok(self.0.lock().unwrap().insert((verifier, claimant, nonce)))
    }
}

async fn endpoint(alpn: &[u8]) -> Endpoint {
    Endpoint::builder(presets::Minimal)
        .alpns(vec![alpn.to_vec()])
        .bind_addr("127.0.0.1:0")
        .unwrap()
        .bind()
        .await
        .unwrap()
}

async fn connect(alice: &Endpoint, bob: &Endpoint, alpn: &[u8]) -> (Connection, Connection) {
    let target = EndpointAddr::new(bob.id()).with_ip_addr(bob.bound_sockets()[0]);
    tokio::time::timeout(DEADLINE, async {
        tokio::join!(
            async { alice.connect(target, alpn).await.unwrap() },
            async { bob.accept().await.unwrap().await.unwrap() }
        )
    })
    .await
    .expect("local QUIC setup exceeded its deadline")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tls_rejects_a_different_endpoint_key_at_the_correct_socket() {
    let alice = endpoint(IDENTITY_ALPN).await;
    let bob = endpoint(IDENTITY_ALPN).await;
    let wrong_id = iroh::SecretKey::from_bytes(&[93; 32]).public();
    assert_ne!(wrong_id, bob.id());
    let target = EndpointAddr::new(wrong_id).with_ip_addr(bob.bound_sockets()[0]);
    let (dial, _) = tokio::time::timeout(DEADLINE, async {
        tokio::join!(alice.connect(target, IDENTITY_ALPN), async {
            let incoming = bob.accept().await.expect("incoming QUIC attempt");
            incoming.await
        })
    })
    .await
    .expect("TLS rejection must complete before the test deadline");
    assert!(
        dial.is_err(),
        "a reachable socket must not override the pinned key"
    );
    alice.close().await;
    bob.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_successful_tls_connection_on_another_alpn_cannot_run_the_identity_handshake() {
    let other_alpn = b"hawala/test-other-protocol/1";
    let alice = endpoint(other_alpn).await;
    let bob = endpoint(other_alpn).await;
    let (left, right) = connect(&alice, &bob, other_alpn).await;
    let nonces = Nonces::default();
    assert!(matches!(
        handshake::initiate(&alice, &left, &key(1), id(2), &nonces).await,
        Err(handshake::HandshakeError::WrongAlpn)
    ));
    assert!(matches!(
        handshake::accept(&bob, &right, &key(2), id(1), &nonces).await,
        Err(handshake::HandshakeError::WrongAlpn)
    ));
    assert!(nonces.0.lock().unwrap().is_empty());
    alice.close().await;
    bob.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scanned_identity_mismatch_rejects_before_consuming_a_nonce() {
    // Exercise both the expected initiator and the initiator's expected responder.
    for (expected_alice, expected_bob) in [(id(3), id(2)), (id(1), id(3))] {
        let alice = endpoint(IDENTITY_ALPN).await;
        let bob = endpoint(IDENTITY_ALPN).await;
        let (left, right) = connect(&alice, &bob, IDENTITY_ALPN).await;
        let left_nonces = Nonces::default();
        let right_nonces = Nonces::default();
        let alice_key = key(1);
        let (initiator, acceptor) = tokio::time::timeout(DEADLINE, async {
            tokio::join!(
                handshake::initiate(&alice, &left, &alice_key, expected_bob, &left_nonces),
                async {
                    let result =
                        handshake::accept(&bob, &right, &key(2), expected_alice, &right_nonces)
                            .await;
                    right.close(0u32.into(), b"identity refused");
                    result
                }
            )
        })
        .await
        .expect("identity refusal must terminate the peer's pending handshake");
        assert!(matches!(
            acceptor,
            Err(handshake::HandshakeError::UnexpectedIdentity)
        ));
        assert!(initiator.is_err());
        assert!(left_nonces.0.lock().unwrap().is_empty());
        assert!(right_nonces.0.lock().unwrap().is_empty());
        alice.close().await;
        bob.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnect_requires_a_proof_for_the_new_authenticated_endpoint() {
    let alice = endpoint(IDENTITY_ALPN).await;
    let old_bob = endpoint(IDENTITY_ALPN).await;
    let (old_connection, _) = connect(&alice, &old_bob, IDENTITY_ALPN).await;
    let old_endpoint_key = *old_connection.remote_id().as_bytes();
    old_bob.close().await;

    let bob = endpoint(IDENTITY_ALPN).await;
    let (connection, _) = connect(&alice, &bob, IDENTITY_ALPN).await;
    assert_ne!(old_endpoint_key, *connection.remote_id().as_bytes());
    let mut challenge = EndpointChallenge {
        verifier: id(1),
        subject: id(2),
        endpoint_key: old_endpoint_key,
        nonce: [51; 32],
        purpose: "peer-auth".into(),
        issued_at: 100,
        expires_at: 200,
    };
    let old_proof = sign_endpoint_proof(&key(2), old_endpoint_key, &challenge).unwrap();
    let nonces = Nonces::default();
    assert_eq!(
        verify_connected_peer(&connection, id(2), &challenge, &old_proof, 150, &nonces),
        Err(Error::PermissionDenied)
    );
    assert!(nonces.0.lock().unwrap().is_empty());

    challenge.endpoint_key = *bob.id().as_bytes();
    let proof = sign_endpoint_proof(&key(2), challenge.endpoint_key, &challenge).unwrap();
    let verified = verify_connected_peer(&connection, id(2), &challenge, &proof, 150, &nonces)
        .expect("fresh proof must bind the participant to the new TLS endpoint");
    assert_eq!(verified.endpoint_key(), *bob.id().as_bytes());
    assert_eq!(
        verify_connected_peer(&connection, id(2), &challenge, &proof, 150, &nonces),
        Err(Error::Replay)
    );
    alice.close().await;
    bob.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn direct_bulk_transfer_preserves_bytes_with_segmentation_offload_enabled_and_disabled() {
    for offload in [true, false] {
        let config = QuicTransportConfig::builder()
            .enable_segmentation_offload(offload)
            .build();
        let alice = Endpoint::builder(presets::Minimal)
            .transport_config(config)
            .alpns(vec![IDENTITY_ALPN.to_vec()])
            .bind_addr("127.0.0.1:0")
            .unwrap()
            .bind()
            .await
            .unwrap();
        let bob = endpoint(IDENTITY_ALPN).await;
        let (left, right) = connect(&alice, &bob, IDENTITY_ALPN).await;
        let alice_nonces = Nonces::default();
        let bob_nonces = Nonces::default();
        let bytes = vec![0x5a; 8 * 1024 * 1024];
        let digest = *blake3::hash(&bytes).as_bytes();
        tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(
                async {
                    handshake::initiate(&alice, &left, &key(1), id(2), &alice_nonces)
                        .await
                        .unwrap();
                    let (mut tx, mut rx) = left.open_bi().await.unwrap();
                    tx.write_all(&bytes).await.unwrap();
                    tx.finish().unwrap();
                    assert_eq!(rx.read_to_end(32).await.unwrap(), digest);
                    assert!(left.paths().iter().all(|path| path.is_ip()));
                },
                async {
                    handshake::accept(&bob, &right, &key(2), id(1), &bob_nonces)
                        .await
                        .unwrap();
                    let (mut tx, mut rx) = right.accept_bi().await.unwrap();
                    let received = rx.read_to_end(bytes.len()).await.unwrap();
                    assert_eq!(received.len(), bytes.len());
                    assert_eq!(*blake3::hash(&received).as_bytes(), digest);
                    tx.write_all(&digest).await.unwrap();
                    tx.finish().unwrap();
                    tx.stopped().await.unwrap();
                }
            );
        })
        .await
        .expect("bulk transfer must finish with either segmentation setting");
        alice.close().await;
        bob.close().await;
    }
}
