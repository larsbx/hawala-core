//! Frozen exact-byte vectors for other implementations (Elixir, JS) to match.
use ed25519_dalek::SigningKey;
use hawala_authority_descriptor::*;
use hawala_identity_core::IdentityId;
use serde_json::Value;

fn vectors() -> Vec<Value> {
    let doc: Value =
        serde_json::from_str(include_str!("../test-vectors/authority_descriptor_v1.json")).unwrap();
    assert_eq!(doc["schema"], "hawala-authority-descriptor-vectors-v1");
    doc["vectors"].as_array().unwrap().clone()
}

fn bytes(v: &Value, field: &str) -> Vec<u8> {
    hex::decode(v[field].as_str().unwrap()).unwrap()
}

fn seed(v: &Value, field: &str) -> SigningKey {
    SigningKey::from_bytes(&bytes(v, field).try_into().unwrap())
}

#[test]
fn envelopes_rederive_from_their_documented_inputs_and_chain() {
    let vs = vectors();
    let mut predecessor = [0; 32];
    for v in &vs {
        let key = seed(v, "seed_hex");
        let descriptor = Descriptor {
            network_id: [1; 32],
            scope_id: [2; 32],
            authority_key: IdentityId::from_signing_key(&key),
            authority_epoch: v["epoch"].as_u64().unwrap(),
            predecessor_digest: predecessor,
            next_key_digest: key_commitment(&IdentityId::from_signing_key(&seed(
                v,
                "next_seed_hex",
            ))),
            custody_profile_digest: [3; 32],
            fencing_policy_digest: [4; 32],
            mandate_digest: [5; 32],
        };
        let signed = sign_descriptor(&key, descriptor).unwrap();
        assert_eq!(signed.encode(), bytes(v, "envelope_hex"), "{}", v["name"]);
        assert_eq!(
            signed.digest().to_vec(),
            bytes(v, "digest_hex"),
            "{}",
            v["name"]
        );
        predecessor = signed.digest();
    }
    let genesis = bytes(&vs[0], "envelope_hex");
    let pin = Pin::anchor(&genesis, bytes(&vs[0], "digest_hex").try_into().unwrap()).unwrap();
    assert_eq!(
        pin.advance(&bytes(&vs[1], "envelope_hex"))
            .unwrap()
            .current()
            .authority_epoch,
        1
    );
}
