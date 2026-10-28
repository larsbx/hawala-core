use ed25519_dalek::SigningKey;
use hawala_cross_authority_test::{endpoints::*, TestHost};
use std::path::PathBuf;
fn fixture() -> (PathBuf, Service, SigningKey, SigningKey) {
    let root = std::env::temp_dir().join(format!(
        "hawala-endpoint-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let host = SigningKey::from_bytes(&[1; 32]);
    let key = SigningKey::from_bytes(&[2; 32]);
    let service = Service::open(
        &root,
        Policy {
            host: host.verifying_key().to_bytes(),
            grant: [3; 32],
            epoch: 1,
            allowed: 15,
        },
        key.clone(),
        true,
    )
    .unwrap();
    (root, service, host, key)
}
fn call(op: u8, id: u8, host: &SigningKey, key: &SigningKey) -> Call {
    Call {
        operation: op,
        service: key.verifying_key().to_bytes(),
        host: host.verifying_key().to_bytes(),
        grant: [3; 32],
        epoch: 1,
        request: [id; 32],
        attempt: [4; 32],
        binding: [5; 32],
        issued: 100,
        expires: 150,
    }
}
#[test]
fn lifecycle_replay_scope_and_no_financial_claims() {
    let (root, s, h, k) = fixture();
    for op in 1..=3 {
        let q = call(op, op, &h, &k).sign(&h);
        let a = s.handle(&q, 101).unwrap();
        assert_eq!(
            verify_response(&a, &q, k.verifying_key().to_bytes()).unwrap(),
            View {
                stage: op,
                held: false
            }
        );
        assert_eq!(s.handle(&q, 102).unwrap(), a);
    }
    let mut c = call(4, 4, &h, &k);
    c.binding = [9; 32];
    let q = c.sign(&h);
    assert!(
        verify_response(
            &s.handle(&q, 101).unwrap(),
            &q,
            k.verifying_key().to_bytes()
        )
        .unwrap()
        .held
    );
    let mut changed = call(4, 1, &h, &k);
    let q = changed.sign(&h);
    assert!(
        verify_response(
            &s.handle(&q, 101).unwrap(),
            &q,
            k.verifying_key().to_bytes()
        )
        .unwrap()
        .held
    );
    changed.request = [8; 32];
    let q = changed.sign(&h);
    assert_eq!(
        verify_response(
            &s.handle(&q, 101).unwrap(),
            &q,
            k.verifying_key().to_bytes()
        )
        .unwrap()
        .stage,
        3
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn signatures_grants_epochs_expiry_and_response_domains_are_pinned() {
    let (root, s, h, k) = fixture();
    let c = call(1, 1, &h, &k);
    let q = c.sign(&h);
    let a = s.handle(&q, 101).unwrap();
    for mut bad in [q.clone(), q.clone()] {
        bad[5] ^= 1;
        assert!(s.handle(&bad, 101).is_err());
    }
    assert!(s.handle(&q, 150).is_err());
    for field in [0, 1, 2] {
        let mut c = c.clone();
        if field == 0 {
            c.grant = [8; 32];
        } else if field == 1 {
            c.epoch = 2;
        } else {
            c.host = [6; 32];
        }
        assert!(s.handle(&c.sign(&h), 101).is_err());
    }
    let other = call(4, 2, &h, &k).sign(&h);
    assert!(verify_response(&a, &other, k.verifying_key().to_bytes()).is_err());
    let mut bad = a.clone();
    bad[0] ^= 1;
    assert!(verify_response(&bad, &q, k.verifying_key().to_bytes()).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn interrupted_finish_recovers_by_observation_without_reexecuting() {
    let (root, s, h, k) = fixture();
    for op in 1..=2 {
        s.handle(&call(op, op, &h, &k).sign(&h), 101).unwrap();
    }
    let db = rusqlite::Connection::open(root.join("endpoint.db")).unwrap();
    db.execute_batch("CREATE TRIGGER stop_finish BEFORE UPDATE OF response ON requests BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    let q = call(3, 3, &h, &k).sign(&h);
    assert!(s.handle(&q, 101).is_err());
    let p = TestHost::open_existing(root.join(hex(&[4; 32])))
        .unwrap()
        .projection()
        .unwrap();
    assert_eq!(p.0[6], 1);
    db.execute_batch("DROP TRIGGER stop_finish;").unwrap();
    drop(s);
    let s = Service::open(
        &root,
        Policy {
            host: h.verifying_key().to_bytes(),
            grant: [3; 32],
            epoch: 1,
            allowed: 15,
        },
        k.clone(),
        false,
    )
    .unwrap();
    let q = call(4, 4, &h, &k).sign(&h);
    assert_eq!(
        verify_response(
            &s.handle(&q, 101).unwrap(),
            &q,
            k.verifying_key().to_bytes()
        )
        .unwrap(),
        View {
            stage: 3,
            held: false
        }
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn incomplete_effects_hold_and_never_start_a_replacement() {
    let (root, s, h, k) = fixture();
    s.handle(&call(1, 1, &h, &k).sign(&h), 101).unwrap();
    let actor = root.join(hex(&[4; 32]));
    let db = rusqlite::Connection::open(actor.join("right.db")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_right BEFORE INSERT ON receipts BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    let q = call(2, 2, &h, &k).sign(&h);
    assert!(
        verify_response(
            &s.handle(&q, 101).unwrap(),
            &q,
            k.verifying_key().to_bytes()
        )
        .unwrap()
        .held
    );
    db.execute_batch("DROP TRIGGER fail_right;").unwrap();
    for (op, id) in [(2, 8), (3, 9), (4, 10)] {
        let q = call(op, id, &h, &k).sign(&h);
        assert!(
            verify_response(
                &s.handle(&q, 101).unwrap(),
                &q,
                k.verifying_key().to_bytes()
            )
            .unwrap()
            .held
        );
    }
    assert_eq!(
        TestHost::open_existing(actor)
            .unwrap()
            .projection()
            .unwrap()
            .0[6],
        0
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn current_status_holds_if_an_independent_actor_store_is_missing() {
    let (root, s, h, k) = fixture();
    for op in 1..=3 {
        s.handle(&call(op, op, &h, &k).sign(&h), 101).unwrap();
    }
    std::fs::remove_file(root.join(hex(&[4; 32])).join("right.db")).unwrap();
    let q = call(4, 4, &h, &k).sign(&h);
    assert!(
        verify_response(
            &s.handle(&q, 101).unwrap(),
            &q,
            k.verifying_key().to_bytes()
        )
        .unwrap()
        .held
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn independent_node_fixture_pins_the_exact_signed_rpc_bytes() {
    let (root, s, h, k) = fixture();
    let cols: Vec<_> = include_str!("../../../formal/evidence/testEndpoints.tsv")
        .lines()
        .nth(1)
        .unwrap()
        .split('\t')
        .collect();
    let decode = |v: &str| -> Vec<u8> {
        (0..v.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&v[i..i + 2], 16).unwrap())
            .collect()
    };
    let request = decode(cols[0]);
    let response = decode(cols[1]);
    let mut c = call(1, 1, &h, &k);
    let mut id = b"hawala/TEST-outbox/id/v1\0".to_vec();
    id.extend(c.attempt);
    id.extend(c.binding);
    id.extend(1u64.to_be_bytes());
    c.request = digest(&id);
    c.expires = 160;
    assert_eq!(c.sign(&h), request);
    assert_eq!(s.handle(&request, 101).unwrap(), response);
    assert_eq!(
        verify_response(&response, &request, k.verifying_key().to_bytes()).unwrap(),
        View {
            stage: 1,
            held: false
        }
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lost_reply_at_every_completed_phase_is_resolved_by_fresh_status() {
    let (root, s, h, k) = fixture();
    let q = call(4, 10, &h, &k).sign(&h);
    assert!(
        verify_response(
            &s.handle(&q, 101).unwrap(),
            &q,
            k.verifying_key().to_bytes()
        )
        .unwrap()
        .held
    );
    for op in 1..=3 {
        // Discard each mutation reply, as if the connection failed after commit.
        s.handle(&call(op, op, &h, &k).sign(&h), 101).unwrap();
        let q = call(4, 10 + op, &h, &k).sign(&h);
        assert_eq!(
            verify_response(
                &s.handle(&q, 102).unwrap(),
                &q,
                k.verifying_key().to_bytes()
            )
            .unwrap(),
            View {
                stage: op,
                held: false
            }
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
