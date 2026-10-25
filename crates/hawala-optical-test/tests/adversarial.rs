use ed25519_dalek::{Signer, SigningKey};
use hawala_identity_core::IdentityId;
use hawala_optical_test::*;
use std::path::Path;
fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
fn id(n: u8) -> IdentityId {
    IdentityId::from_signing_key(&key(n))
}
fn policy() -> TestPolicy {
    TestPolicy {
        network: [10; 32],
        terminal: id(2),
        authority: id(3),
        epoch: 1,
    }
}
fn scope() -> Scope {
    Scope {
        network: [10; 32],
        transfer: [11; 32],
        terms: [12; 32],
        attempt: [13; 32],
        holder: id(1),
        terminal: id(2),
        origin: id(4),
        disburser: id(5),
        coordinated_binding: [14; 32],
        amount_minor: 900,
        fee_minor: 100,
        epoch: 1,
        currency: *b"EUR",
    }
}
// Independent attacker encoder follows the published wire contract. This
// creates valid signatures on misbound messages, rather than testing only
// accidental corruption and invalid signatures.
fn resign(message: &Envelope, signer: &SigningKey) -> Envelope {
    let mut bytes = message.encode();
    let mut statement = b"hawala/optical-TEST-evidence/v1\0".to_vec();
    statement.extend_from_slice(&bytes[..ENVELOPE_LEN - 64]);
    bytes[ENVELOPE_LEN - 64..].copy_from_slice(&signer.sign(&statement).to_bytes());
    Envelope::decode(&bytes).unwrap()
}
fn precommit() -> Envelope {
    Envelope::precommit(&key(3), scope(), 100, 200).unwrap()
}
fn setup(path: &Path) -> (TerminalLab, Envelope, Envelope, Envelope) {
    let mut lab = TerminalLab::create(path, policy()).unwrap();
    let c = precommit();
    lab.import(&c, 110).unwrap();
    let h = lab
        .challenge(&c, &key(2), [20; 32], [21; 32], 120, 180)
        .unwrap();
    let r = respond(&key(1), id(2), id(3), &c, &h, 130).unwrap();
    (lab, c, h, r)
}
fn scanned(e: &Envelope) -> Envelope {
    let mut receiver = CameraReceiver::new(130);
    let mut result = None;
    for f in transmit(e).into_iter().rev() {
        result = receiver.receive(&f, 131).unwrap();
    }
    result.unwrap()
}
#[test]
fn bidirectional_multiframe_exchange_and_signed_conflicting_reports_stay_held() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("terminal");
    let (mut lab, c, h, r) = setup(&p);
    assert_eq!(scanned(&c), c);
    assert_eq!(scanned(&h), h);
    assert_eq!(scanned(&r), r);
    assert_eq!(
        lab.accept(&scanned(&r), id(1), 140).unwrap(),
        Evidence::AuthenticatedHeld
    );
    let yes = observation(&key(2), &r, 1).unwrap();
    let no = observation(&key(2), &r, 2).unwrap();
    assert_eq!(
        lab.observe(&scanned(&yes), 141).unwrap(),
        Evidence::ObservationHeld
    );
    assert_eq!(
        lab.observe(&scanned(&no), 142).unwrap(),
        Evidence::ConflictHeld
    );
    assert_eq!(lab.observe(&no, 143).unwrap(), Evidence::ConflictHeld);
    assert_eq!(lab.observations(scope().transfer).unwrap().len(), 2);
    assert_eq!(lab.state(scope().transfer).unwrap(), "consumed_held");
    drop(lab);
    let lab = TerminalLab::open(p, policy()).unwrap();
    assert_eq!(lab.observations(scope().transfer).unwrap().len(), 2);
    assert_eq!(lab.state(scope().transfer).unwrap(), "consumed_held");
}
#[test]
fn video_replay_lost_ack_and_restart_never_yield_second_authentication() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("terminal");
    let (mut lab, c, _, r) = setup(&p);
    assert_eq!(
        lab.accept(&r, id(1), 140).unwrap(),
        Evidence::AuthenticatedHeld
    );
    drop(lab);
    let mut lab = TerminalLab::open(p, policy()).unwrap();
    assert_eq!(
        lab.accept(&scanned(&r), id(1), 141).unwrap(),
        Evidence::IdenticalReplayHeld
    );
    lab.import(&c, 142).unwrap();
    assert!(lab
        .challenge(&c, &key(2), [22; 32], [23; 32], 143, 180)
        .is_err());
    assert!(lab.accept(&r, id(1), 180).is_err());
    assert_eq!(lab.state(scope().transfer).unwrap(), "consumed_held");
}
#[test]
fn interrupted_exchange_resumes_same_challenge_but_cannot_issue_replacement() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("terminal");
    let (lab, c, _, r) = setup(&p);
    drop(lab);
    let mut lab = TerminalLab::open(p, policy()).unwrap();
    assert!(lab
        .challenge(&c, &key(2), [22; 32], [23; 32], 130, 180)
        .is_err());
    assert_eq!(
        lab.accept(&r, id(1), 140).unwrap(),
        Evidence::AuthenticatedHeld
    );
}
#[test]
fn expired_interruption_fails_closed_and_clock_rollback_is_rejected() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("terminal");
    let (mut lab, c, _, r) = setup(&p);
    assert!(lab.accept(&r, id(1), 119).is_err());
    assert!(lab.accept(&r, id(1), 180).is_err());
    assert!(lab
        .challenge(&c, &key(2), [22; 32], [23; 32], 181, 190)
        .is_err());
    assert!(lab.accept(&r, id(1), 140).is_err());
    drop(lab);
    let mut lab = TerminalLab::open(p, policy()).unwrap();
    assert!(lab.accept(&r, id(1), 139).is_err());
    assert_eq!(lab.state(scope().transfer).unwrap(), "challenged");
}
#[test]
fn camera_cannot_forge_holder_terminal_or_cash_observation() {
    let d = tempfile::tempdir().unwrap();
    let (mut lab, c, h, r) = setup(&d.path().join("terminal"));
    assert!(respond(&key(9), id(2), id(3), &c, &h, 130).is_err());
    assert!(respond(&key(1), id(9), id(3), &c, &h, 130).is_err());
    assert!(respond(&key(1), id(2), id(9), &c, &h, 130).is_err());
    assert!(lab.accept(&r, id(9), 140).is_err());
    let mut forged = r.clone();
    forged.scope.amount_minor += 1;
    assert!(lab.accept(&forged, id(1), 140).is_err());
    assert!(observation(&key(9), &r, 1).is_err());
    assert_eq!(lab.state(scope().transfer).unwrap(), "challenged");
}
#[test]
fn altered_scope_and_cross_terminal_relay_reject_but_unchanged_live_relay_is_possible() {
    let d = tempfile::tempdir().unwrap();
    let (mut lab, c, h, r) = setup(&d.path().join("one"));
    let other = TestPolicy {
        terminal: id(8),
        ..policy()
    };
    let mut other_lab = TerminalLab::create(d.path().join("two"), other).unwrap();
    assert!(other_lab.import(&c, 130).is_err());
    assert!(other_lab.accept(&r, id(1), 140).is_err());
    for f in 0..7 {
        let mut changed = h.clone();
        match f {
            0 => changed.scope.terminal = id(8),
            1 => changed.scope.attempt = [31; 32],
            2 => changed.scope.terms = [31; 32],
            3 => changed.session = [31; 32],
            4 => changed.nonce = [31; 32],
            5 => changed.parent = [31; 32],
            _ => changed.expires_at += 1,
        };
        // A valid terminal signer may choose a fresh nonce/window; the holder
        // cannot know whether it was durably issued. The terminal must still
        // reject any response not matching its actually stored challenge.
        if let Ok(response) = respond(&key(1), id(2), id(3), &c, &resign(&changed, &key(2)), 130) {
            assert!(lab.accept(&response, id(1), 140).is_err());
        }
    }
    // Fundamental limit: transporting the unchanged live exchange across a
    // distance is indistinguishable. The only result is HELD evidence.
    let relayed = respond(&key(1), id(2), id(3), &scanned(&c), &scanned(&h), 130).unwrap();
    assert_eq!(
        lab.accept(&relayed, id(1), 140).unwrap(),
        Evidence::AuthenticatedHeld
    );
}
#[test]
fn concurrent_connections_consume_one_durable_authorization() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("terminal");
    let (lab, _, _, r) = setup(&p);
    drop(lab);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let p = p.clone();
            let r = r.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut lab = TerminalLab::open(p, policy()).unwrap();
                barrier.wait();
                lab.accept(&r, id(1), 140).unwrap()
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(
        results
            .iter()
            .filter(|v| **v == Evidence::AuthenticatedHeld)
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|v| **v == Evidence::IdenticalReplayHeld)
            .count(),
        1
    );
}
#[test]
fn consumption_rolls_back_on_storage_failure_or_ignored_update() {
    for action in ["RAISE(ABORT,'fault')", "RAISE(IGNORE)"] {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("terminal");
        let (mut lab, _, _, r) = setup(&p);
        let db = rusqlite::Connection::open(&p).unwrap();
        db.execute_batch(&format!("CREATE TRIGGER fail BEFORE UPDATE OF response ON optical_intents BEGIN SELECT {action}; END;")).unwrap();
        assert!(lab.accept(&r, id(1), 140).is_err());
        assert_eq!(lab.state(scope().transfer).unwrap(), "challenged");
        db.execute_batch("DROP TRIGGER fail").unwrap();
        assert_eq!(
            lab.accept(&r, id(1), 140).unwrap(),
            Evidence::AuthenticatedHeld
        );
    }
}
#[test]
fn no_duplicate_transfer_attempt_or_challenge_and_no_policy_reconfiguration() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("terminal");
    let (mut lab, _, _, _) = setup(&p);
    let mut s = scope();
    s.transfer = [30; 32]; // same reserved attempt under new transfer
    assert!(lab
        .import(
            &Envelope::precommit(&key(3), s.clone(), 100, 200).unwrap(),
            130
        )
        .is_err());
    s.attempt = [31; 32];
    let c = Envelope::precommit(&key(3), s, 100, 200).unwrap();
    lab.import(&c, 130).unwrap();
    assert!(lab
        .challenge(&c, &key(2), [20; 32], [21; 32], 130, 180)
        .is_err());
    assert!(TerminalLab::create(&p, policy()).is_err());
    drop(lab);
    assert!(TerminalLab::open(
        &p,
        TestPolicy {
            epoch: 2,
            ..policy()
        }
    )
    .is_err());
    let mut lab = TerminalLab::open(&p, policy()).unwrap();
    for change in 0..3 {
        let mut s = scope();
        match change {
            0 => s.network = [33; 32],
            1 => s.epoch = 2,
            _ => s.terminal = id(9),
        };
        assert!(lab
            .import(&Envelope::precommit(&key(3), s, 100, 200).unwrap(), 140)
            .is_err());
    }
    assert!(lab
        .import(
            &Envelope::precommit(&key(9), scope(), 100, 200).unwrap(),
            140
        )
        .is_err());
}
#[test]
fn frame_reordering_duplicates_and_missing_chunks_are_bounded() {
    let c = precommit();
    let frames = transmit(&c);
    assert!(frames.len() <= MAX_FRAMES);
    assert!(frames.iter().all(|f| f.len() <= MAX_QR_CHARS));
    let mut r = CameraReceiver::new(100);
    assert!(r.receive(&frames[0], 101).unwrap().is_none());
    assert!(r.receive(&frames[0], 102).unwrap().is_none());
    for f in frames[1..frames.len() - 1].iter().rev() {
        assert!(r.receive(f, 103).unwrap().is_none());
    }
    assert_eq!(r.receive(frames.last().unwrap(), 104).unwrap().unwrap(), c);
    assert!(r.receive(&frames[0], 105).is_err()); // complete receiver never emits twice
    let mut r = CameraReceiver::new(100);
    for _ in 0..32 {
        assert!(r.receive(&frames[0], 101).unwrap().is_none());
    }
    assert!(r.receive(&frames[0], 101).is_err());
}
#[test]
fn delayed_or_backwards_frames_and_mixed_videos_poison_the_assembly() {
    let frames = transmit(&precommit());
    for time in [99, 110, u64::MAX] {
        let mut r = CameraReceiver::new(100);
        assert!(r.receive(&frames[0], time).is_err());
    }
    let mut other = scope();
    other.attempt = [32; 32];
    let mixed = transmit(&Envelope::precommit(&key(3), other, 100, 200).unwrap());
    let mut r = CameraReceiver::new(100);
    r.receive(&frames[0], 101).unwrap();
    assert!(r.receive(&mixed[1], 102).is_err());
    assert!(r.receive(&frames[1], 103).is_err());
    let mut r = CameraReceiver::new(100);
    r.receive(&frames[0], 102).unwrap();
    assert!(r.receive(&frames[1], 101).is_err());
}
#[test]
fn malformed_fragment_indices_sizes_headers_and_bit_flips_reject_without_panics() {
    let frames = transmit(&precommit());
    let raw = hex::decode(frames[0].strip_prefix(QR_PREFIX).unwrap()).unwrap();
    for index in [0, 8, 40, 41, 42, 43, 44, 45, 46, 47, 48] {
        let mut b = raw.clone();
        b[index] ^= 255;
        let mut r = CameraReceiver::new(100);
        let bad = format!("{QR_PREFIX}{}", hex::encode(b));
        let first = r.receive(&bad, 101);
        if first.is_ok() {
            assert!(frames[1..].iter().any(|f| r.receive(f, 102).is_err()));
        }
    }
    for bad in [
        "hawala:auth:v1:00".to_owned(),
        "hawala:id:v1:00".to_owned(),
        frames[0].to_uppercase(),
        format!("{}00", frames[0]),
        "z".repeat(MAX_QR_CHARS + 1),
    ] {
        assert!(CameraReceiver::new(100).receive(&bad, 101).is_err());
    }
}
#[test]
fn fixed_canonical_envelopes_reject_every_bit_flip_truncation_and_suffix() {
    let wire = precommit().encode();
    assert_eq!(wire.len(), ENVELOPE_LEN);
    assert_eq!(Envelope::decode(&wire).unwrap(), precommit());
    for i in 0..wire.len() {
        let mut b = wire.clone();
        b[i] ^= 1;
        assert!(Envelope::decode(&b).is_err(), "byte {i}");
    }
    for end in 0..wire.len() {
        assert!(Envelope::decode(&wire[..end]).is_err());
    }
    let mut b = wire;
    b.push(0);
    assert!(Envelope::decode(&b).is_err());
}
#[test]
fn persistent_storage_and_missing_schema_fail_closed() {
    assert!(TerminalLab::create(":memory:", policy()).is_err());
    assert!(TerminalLab::create("file:test?mode=memory", policy()).is_err());
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("terminal");
    assert!(TerminalLab::open(&p, policy()).is_err());
    let (lab, _, _, _) = setup(&p);
    drop(lab);
    let db = rusqlite::Connection::open(&p).unwrap();
    db.execute_batch("DROP TABLE optical_intents").unwrap();
    assert!(TerminalLab::open(&p, policy()).is_err());
    assert!(TerminalLab::create(&p, policy()).is_err());
}
#[test]
fn abrupt_exit_child() {
    let Some(path) = std::env::var_os("HAWALA_OPTICAL_CRASH_TEST") else {
        return;
    };
    let (mut lab, _, _, r) = setup(Path::new(&path));
    lab.accept(&r, id(1), 140).unwrap();
    std::process::exit(0); // bypass destructors and graceful connection close
}
#[test]
fn abrupt_process_exit_preserves_single_use_before_ack() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("terminal");
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "abrupt_exit_child"])
        .env("HAWALA_OPTICAL_CRASH_TEST", &p)
        .status()
        .unwrap();
    assert!(result.success());
    let mut lab = TerminalLab::open(&p, policy()).unwrap();
    let h = lab.state(scope().transfer).unwrap();
    assert_eq!(h, "consumed_held");
    // Deterministic same signed response as child; no second authorization.
    let c = precommit();
    let tmp = tempfile::tempdir().unwrap();
    let (_, _, _, r) = setup(&tmp.path().join("other"));
    assert_eq!(
        lab.accept(&r, id(1), 141).unwrap(),
        Evidence::IdenticalReplayHeld
    );
    assert!(lab
        .challenge(&c, &key(2), [30; 32], [31; 32], 142, 180)
        .is_err());
}

#[test]
fn valid_holder_signature_on_wrong_session_challenge_or_transaction_rejects() {
    let d = tempfile::tempdir().unwrap();
    let (mut lab, _, _, r) = setup(&d.path().join("terminal"));
    for index in 0..8 {
        let mut v = r.clone();
        match index {
            0 => v.session = [90; 32],
            1 => v.nonce = [90; 32],
            2 => v.parent = [90; 32],
            3 => v.scope.amount_minor += 1,
            4 => v.scope.attempt = [90; 32],
            5 => v.scope.coordinated_binding = [90; 32],
            6 => v.scope.terminal = id(9),
            _ => v.expires_at -= 1,
        }
        assert!(lab.accept(&resign(&v, &key(1)), id(1), 140).is_err());
        assert_eq!(lab.state(scope().transfer).unwrap(), "challenged");
    }
    assert_eq!(
        lab.accept(&r, id(1), 140).unwrap(),
        Evidence::AuthenticatedHeld
    );
}
#[test]
fn conflicting_duplicate_fragment_and_untrusted_video_timestamps_cannot_extend_deadline() {
    let frames = transmit(&precommit());
    let mut receiver = CameraReceiver::new(100);
    receiver.receive(&frames[0], 101).unwrap();
    let mut raw = hex::decode(frames[0].strip_prefix(QR_PREFIX).unwrap()).unwrap();
    raw[48] ^= 1;
    assert!(receiver
        .receive(&format!("{QR_PREFIX}{}", hex::encode(raw)), 102)
        .is_err());
    let mut receiver = CameraReceiver::new(100);
    receiver.receive(&frames[0], 101).unwrap();
    receiver.receive(&frames[0], 109).unwrap();
    assert!(receiver.receive(&frames[1], 110).is_err());
}

#[test]
fn cloned_same_identity_store_is_an_explicit_limit_not_global_offline_spend_protection() {
    let d = tempfile::tempdir().unwrap();
    let first = d.path().join("one");
    let second = d.path().join("cloned");
    let (lab, _, _, r) = setup(&first);
    drop(lab); // checkpoint before hostile backup clone
    std::fs::copy(&first, &second).unwrap();
    let mut one = TerminalLab::open(&first, policy()).unwrap();
    let mut cloned = TerminalLab::open(&second, policy()).unwrap();
    assert_eq!(
        one.accept(&r, id(1), 140).unwrap(),
        Evidence::AuthenticatedHeld
    );
    assert_eq!(
        cloned.accept(&r, id(1), 140).unwrap(),
        Evidence::AuthenticatedHeld
    );
    // A clone cannot discover the other offline store's state. Neither result
    // is cash, a hardware ticket, custody proof or settlement authority.
    assert_eq!(one.state(scope().transfer).unwrap(), "consumed_held");
    assert_eq!(cloned.state(scope().transfer).unwrap(), "consumed_held");
}
