mod support;
use hawala_optical_media_test::*;
use hawala_optical_test::*;
use std::path::Path;
use support::*;
fn worker() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_optical-qr-decode"))
}

#[test]
fn three_frame_bidirectional_exchange_journal_replay_and_conflicts_remain_held() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("terminal");
    // Actually import/challenge/respond using image-recovered messages.
    let c = scanned(worker(), &commitment());
    let mut lab = TerminalLab::create(&path, policy()).unwrap();
    lab.import(&c, 140).unwrap();
    let h = lab
        .challenge(&c, &key(2), [20; 32], [21; 32], 140, 180)
        .unwrap();
    let h = scanned_at(worker(), &h, 140);
    let r = respond(&key(1), id(2), id(3), &c, &h, 145).unwrap();
    let r = scanned_at(worker(), &r, 145);
    assert_eq!(
        lab.accept(&r, id(1), 150).unwrap(),
        Evidence::AuthenticatedHeld
    );
    drop(lab);
    let mut lab = TerminalLab::open(&path, policy()).unwrap();
    assert_eq!(
        lab.accept(&scanned_at(worker(), &r, 145), id(1), 151)
            .unwrap(),
        Evidence::IdenticalReplayHeld
    );
    for (report, expected) in [(1, Evidence::ObservationHeld), (2, Evidence::ConflictHeld)] {
        let o = scanned(worker(), &observation(&key(2), &r, report).unwrap());
        assert_eq!(lab.observe(&o, 152).unwrap(), expected);
    }
    assert_eq!(lab.observations(scope().transfer).unwrap().len(), 2);
    assert_eq!(lab.state(scope().transfer).unwrap(), "consumed_held");
}
#[test]
fn scaling_orientation_display_recover_exact_text_and_blur_stays_unreadable() {
    let texts = transmit(&commitment());
    assert_eq!(
        texts.iter().map(String::len).collect::<Vec<_>>(),
        vec![503, 503, 417]
    );
    for scale in [2, 4, 8] {
        for turns in 0..4 {
            for text in &texts {
                let image = rotate(&Raster::render(text, scale).unwrap(), turns);
                let (decoded, measurement) = decode_isolated(worker(), &image).unwrap();
                assert_eq!(
                    decoded.as_deref(),
                    Some(text.as_str()),
                    "scale={scale} turns={turns}"
                );
                assert!(measurement.peak_rss_kib < ADDRESS_LIMIT / 1024);
            }
        }
    }
    for text in &texts {
        for (condition, image) in [
            ("display", display(&Raster::render(text, 4).unwrap())),
            ("blur", blur(&Raster::render(text, 4).unwrap())),
        ] {
            let decoded = decode_isolated(worker(), &image).unwrap().0;
            if condition == "display" {
                assert_eq!(decoded, Some(text.clone()));
            } else {
                assert_eq!(decoded, None);
            }
        }
    }
}
#[test]
fn reordered_repeated_dropped_partial_and_delayed_video() {
    let c = commitment();
    let images: Vec<_> = transmit(&c)
        .iter()
        .map(|t| Raster::render(t, 4).unwrap())
        .collect();
    let mut r = ImageReceiver::new(130);
    for (index, now) in [(2, 131), (2, 132), (0, 133)] {
        assert!(r
            .receive(worker(), &images[index], now)
            .unwrap()
            .0
            .is_none());
    }
    assert_eq!(r.receive(worker(), &images[1], 139).unwrap().0, Some(c));
    assert!(r.receive(worker(), &images[1], 139).is_err());
    let mut r = ImageReceiver::new(130);
    assert!(r.receive(worker(), &images[0], 131).unwrap().0.is_none());
    assert!(r.receive(worker(), &blank(), 132).unwrap().0.is_none());
    assert!(r
        .receive(worker(), &obscure(&images[1]), 133)
        .unwrap()
        .0
        .is_none());
    assert!(r.receive(worker(), &images[2], 134).unwrap().0.is_none());
    // Missing image never emits; blanks/delayed final image cannot renew deadline.
    assert!(r.receive(worker(), &blank(), 140).is_err());
    assert!(r.receive(worker(), &images[1], 139).is_err());
    let mut r = ImageReceiver::new(130);
    for _ in 0..32 {
        assert!(r.receive(worker(), &blank(), 131).unwrap().0.is_none());
    }
    assert!(r.receive(worker(), &images[0], 131).is_err());
    let mut r = ImageReceiver::new(130);
    r.receive(worker(), &blank(), 132).unwrap();
    assert!(r.receive(worker(), &images[0], 131).is_err());
}
#[test]
fn corrupt_and_conflicting_images_cannot_complete_or_change_pinned_scope() {
    let c = commitment();
    let frames = transmit(&c);
    // QR ECC faithfully recovers the attacker's corrupted FRAME text. The
    // canonical frame digest, rather than QR correction, rejects tampering.
    let mut corrupt = frames[0].clone().into_bytes();
    let n = corrupt.len() - 1;
    corrupt[n] = if corrupt[n] == b'0' { b'1' } else { b'0' };
    let corrupt = String::from_utf8(corrupt).unwrap();
    let mut r = ImageReceiver::new(130);
    for text in [&corrupt, &frames[1]] {
        assert!(r
            .receive(worker(), &Raster::render(text, 4).unwrap(), 131)
            .unwrap()
            .0
            .is_none());
    }
    assert!(r
        .receive(worker(), &Raster::render(&frames[2], 4).unwrap(), 131)
        .is_err());
    let mut other_scope = scope();
    other_scope.amount_minor += 1;
    let other = Envelope::precommit(&key(3), other_scope, 100, 200).unwrap();
    let mut r = ImageReceiver::new(130);
    r.receive(worker(), &Raster::render(&frames[0], 4).unwrap(), 131)
        .unwrap();
    assert!(r
        .receive(
            worker(),
            &Raster::render(&transmit(&other)[1], 4).unwrap(),
            132
        )
        .is_err());
    // Valid signatures/digests at a wrong transaction scope still reject at DB.
    let d = tempfile::tempdir().unwrap();
    let (mut lab, _, _, response) = setup(&d.path().join("terminal"));
    let d2 = tempfile::tempdir().unwrap();
    let mut foreign = TerminalLab::create(d2.path().join("other"), policy()).unwrap();
    foreign.import(&other, 110).unwrap();
    let h = foreign
        .challenge(&other, &key(2), [30; 32], [31; 32], 120, 180)
        .unwrap();
    let wrong = respond(&key(1), id(2), id(3), &other, &h, 130).unwrap();
    assert!(lab.accept(&scanned(worker(), &wrong), id(1), 140).is_err());
    assert_eq!(lab.state(scope().transfer).unwrap(), "challenged");
    assert_eq!(
        lab.accept(&scanned(worker(), &response), id(1), 141)
            .unwrap(),
        Evidence::AuthenticatedHeld
    );
    // Re-framed envelope with a bad signature gets a correct transport digest,
    // then is rejected by Envelope::decode, not trusted because it was QR-read.
    let mut forged = response.clone();
    forged.scope.amount_minor += 1;
    let mut r = ImageReceiver::new(130);
    let f = transmit(&forged);
    for text in &f[..2] {
        r.receive(worker(), &Raster::render(text, 4).unwrap(), 131)
            .unwrap();
    }
    assert!(r
        .receive(worker(), &Raster::render(&f[2], 4).unwrap(), 132)
        .is_err());
}
#[test]
fn ambiguity_and_malformed_text_fail_closed() {
    let f = transmit(&commitment());
    let a = Raster::render(&f[0], 4).unwrap();
    let b = Raster::render(&f[1], 4).unwrap();
    for image in [pair(&a, &b), pair(&a, &a)] {
        assert!(decode_isolated(worker(), &image).is_err());
        let mut r = ImageReceiver::new(130);
        assert!(r.receive(worker(), &image, 131).is_err());
        assert!(r.receive(worker(), &a, 132).is_err());
    }
    for bad in ["not Hawala".to_owned(), f[0].to_uppercase()] {
        let mut r = ImageReceiver::new(130);
        assert!(r
            .receive(worker(), &Raster::render(&bad, 4).unwrap(), 131)
            .is_err());
        assert!(r.receive(worker(), &a, 132).is_err());
    }
}
#[test]
fn bounded_raster_parser_rejects_header_bombs_truncation_and_trailing_bytes() {
    for (w, h) in [(0u32, 1u32), (u32::MAX, u32::MAX), (1025, 1), (1, 1025)] {
        let mut bytes = b"HWRAST01".to_vec();
        bytes.extend(w.to_be_bytes());
        bytes.extend(h.to_be_bytes());
        assert!(Raster::read_wire(&mut bytes.as_slice()).is_err());
    }
    let mut bytes = Vec::new();
    blank().write_wire(&mut bytes).unwrap();
    assert!(Raster::read_wire(&mut &bytes[..bytes.len() - 1]).is_err());
    bytes.push(0);
    assert!(Raster::read_wire(&mut bytes.as_slice()).is_err());
    assert!(Raster::render("x", 0).is_err());
    assert!(Raster::render(&"x".repeat(504), 4).is_err());
}
#[test]
fn unreadable_observations_use_existing_receiver_budget_and_permanent_deadline() {
    let f = transmit(&commitment());
    let mut r = CameraReceiver::new(130);
    r.receive(&f[0], 131).unwrap();
    for _ in 0..30 {
        r.observe_no_text(132).unwrap();
    }
    r.receive(&f[1], 133).unwrap();
    assert!(r.receive(&f[2], 134).is_err());
    let mut r = CameraReceiver::new(130);
    r.observe_no_text(139).unwrap();
    assert!(r.observe_no_text(140).is_err());
    assert!(r.receive(&f[0], 139).is_err());
    let mut r = CameraReceiver::new(130);
    r.observe_no_text(132).unwrap();
    assert!(r.observe_no_text(131).is_err());
    assert!(r.receive(&f[0], 133).is_err());
}
#[test]
fn resource_limits_crashes_and_oversized_worker_replies_poison_adapter() {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    let fake = d.path().join("worker");
    for script in [
        "#!/bin/sh\nexit 2\n",
        "#!/bin/sh\nprintf '\\001'; head -c 600 /dev/zero\n",
        "#!/bin/sh\nwhile :; do :; done\n",
    ] {
        std::fs::write(&fake, script).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut r = ImageReceiver::new(130);
        assert!(r.receive(&fake, &blank(), 131).is_err());
        assert!(r.receive(worker(), &blank(), 132).is_err());
    }
}
