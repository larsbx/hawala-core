use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

use ed25519_dalek::SigningKey;
use hawala_identity_core::{IdentityId, ReplayGuard};
use hawala_iroh_bridge::{handshake, IDENTITY_ALPN};
use iroh::{endpoint::presets, Endpoint};

fn key(b: u8) -> SigningKey { SigningKey::from_bytes(&[b; 32]) }
fn id(b: u8) -> IdentityId { IdentityId::from_signing_key(&key(b)) }

#[derive(Default)]
struct NonceGuard(Mutex<HashSet<(IdentityId, IdentityId, [u8; 32])>>);
impl ReplayGuard for NonceGuard {
    fn consume_once(
        &self, verifier: IdentityId, claimant: IdentityId, nonce: [u8; 32],
    ) -> hawala_identity_core::Result<bool> {
        Ok(self.0.lock().unwrap().insert((verifier, claimant, nonce)))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_two_peer_iroh_handshake_binds_qr_identity_to_transport() {
    let alice = Endpoint::builder(presets::Minimal)
        .alpns(vec![IDENTITY_ALPN.to_vec()]).bind().await.unwrap();
    let bob = Endpoint::builder(presets::Minimal)
        .alpns(vec![IDENTITY_ALPN.to_vec()]).bind().await.unwrap();
    let bob_addr = bob.addr();
    let nonce_alice = NonceGuard::default();
    let nonce_bob = NonceGuard::default();
    let output = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(
            async {
                let conn = alice.connect(bob_addr, IDENTITY_ALPN)
                    .await.map_err(|e| format!("connect Alice: {e}"))?;
                handshake::initiate(&alice, &conn, &key(1), id(2), &nonce_alice)
                    .await.map_err(|e| format!("Alice handshake: {e:?}"))
            },
            async {
                let incoming = bob.accept().await.ok_or("Bob endpoint closed".to_string())?;
                let conn = incoming.await.map_err(|e| format!("Bob connect: {e}"))?;
                handshake::accept(&bob, &conn, &key(2), id(1), &nonce_bob)
                    .await.map_err(|e| format!("Bob handshake: {e:?}"))
            }
        )
    }).await.expect("local two-peer handshake timed out");
    let (verified_alice, verified_bob) = output;
    let va = verified_alice.expect("initiator must verify scanned Bob identity");
    let vb = verified_bob.expect("responder must verify scanned Alice identity");
    assert_eq!(va.identity(), id(2));
    assert_eq!(vb.identity(), id(1));
    assert_eq!(va.endpoint_key(), *bob.id().as_bytes());
    assert_eq!(vb.endpoint_key(), *alice.id().as_bytes());
    alice.close().await;
    bob.close().await;
}

/// Recognition checks must never rely only on the short code/display name.
#[test]
fn visible_identity_codes_are_not_cryptographic_session_authority() {
    assert_ne!(id(1).human_code(), id(2).human_code());
    assert_ne!(id(1).qr_payload(), id(2).qr_payload());
    let display = id(1).presentation(Some("Alice Agent")).unwrap();
    assert_eq!(IdentityId::parse_qr(&display.qr_payload), Ok(id(1)));
}
