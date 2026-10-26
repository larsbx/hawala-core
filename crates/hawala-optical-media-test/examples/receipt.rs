//! Reproducible synthetic TEST images and measured optical observations.
#[path = "../tests/support/mod.rs"]
mod support;
use hawala_optical_media_test::*;
use hawala_optical_test::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use support::*;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 3, "receipt WORKER OUTPUT_DIRECTORY");
    let worker = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&output).unwrap();
    let c = commitment();
    let texts = transmit(&c);
    let mut rows = Vec::new();
    let mut fixtures = Vec::new();
    for scale in [1, 2, 4, 8] {
        for turns in 0..4 {
            for (i, text) in texts.iter().enumerate() {
                let image = rotate(&Raster::render(text, scale).unwrap(), turns);
                let (decoded, m) = decode_isolated(&worker, &image).unwrap();
                assert!(decoded.is_none() || decoded.as_ref() == Some(text));
                if scale >= 2 {
                    assert_eq!(decoded.as_ref(), Some(text));
                }
                rows.push(json!({"condition":"clean","scale":scale,"clockwise_quarter_turns":turns,
            "fragment":i,"width":image.width(),"height":image.height(),"decoded_exact":decoded.as_ref()==Some(text),
            "wall_micros":m.wall_micros,"cpu_micros":m.cpu_micros,"peak_rss_kib":m.peak_rss_kib}));
            }
        }
    }
    for (i, text) in texts.iter().enumerate() {
        let clean = Raster::render(text, 4).unwrap();
        for (condition, image) in [
            ("display_32_224_on_gray_160", display(&clean)),
            ("box_blur_3x3", blur(&clean)),
            ("right_half_occluded", obscure(&clean)),
        ] {
            let (decoded, m) = decode_isolated(&worker, &image).unwrap();
            assert!(decoded.is_none() || decoded.as_ref() == Some(text));
            if condition.starts_with("display") {
                assert_eq!(decoded.as_ref(), Some(text));
            }
            // Lossy optical conditions are measured, not silently retried or upgraded.
            rows.push(json!({"condition":condition,"scale":4,"fragment":i,
                "width":image.width(),"height":image.height(),"decoded_exact":decoded.as_ref()==Some(text),
                "wall_micros":m.wall_micros,"cpu_micros":m.cpu_micros,"peak_rss_kib":m.peak_rss_kib}));
        }
        let name = format!("precommit-{i}.pgm");
        let bytes = clean.pgm();
        std::fs::write(output.join(&name), &bytes).unwrap();
        fixtures.push(json!({"path":name,"blake3":blake3::hash(&bytes).to_hex().to_string(),"qr_chars":text.len()}));
    }
    // Simulated video is a schedule of independently bounded raw frames, no
    // file-container timestamps, compressed codec or recording is trusted.
    let images: Vec<_> = texts
        .iter()
        .map(|t| Raster::render(t, 4).unwrap())
        .collect();
    type Schedule = (&'static str, Vec<(usize, u64)>, &'static str);
    let schedules: [Schedule; 5] = [
        (
            "ordered",
            vec![(0, 131), (1, 132), (2, 133)],
            "complete_signed_evidence",
        ),
        (
            "reordered_repeated",
            vec![(2, 131), (2, 132), (0, 133), (1, 139)],
            "complete_signed_evidence",
        ),
        ("dropped", vec![(0, 131), (2, 133)], "incomplete"),
        ("delayed", vec![(0, 131), (1, 132), (2, 140)], "rejected"),
        (
            "partial_visibility",
            vec![(0, 131), (3, 132), (2, 133)],
            "incomplete",
        ),
    ];
    let partial = obscure(&images[1]);
    let mut sequences = Vec::new();
    for (name, schedule, expected) in schedules {
        let mut r = ImageReceiver::new(130);
        let mut actual = "incomplete";
        for (index, time) in &schedule {
            let image = if *index == 3 {
                &partial
            } else {
                &images[*index]
            };
            match r.receive(&worker, image, *time) {
                Ok((Some(found), _)) => {
                    assert_eq!(found, c);
                    actual = "complete_signed_evidence";
                }
                Ok((None, _)) => (),
                Err(_) => {
                    actual = "rejected";
                    break;
                }
            }
        }
        assert_eq!(actual, expected);
        sequences.push(json!({"name":name,"started_at":130,"schedule":schedule,"outcome":actual}));
    }
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("terminal");
    let c = scanned(&worker, &c);
    let mut lab = TerminalLab::create(&path, policy()).unwrap();
    lab.import(&c, 140).unwrap();
    let h = lab
        .challenge(&c, &key(2), [20; 32], [21; 32], 140, 180)
        .unwrap();
    let h = scanned_at(&worker, &h, 140);
    let r = respond(&key(1), id(2), id(3), &c, &h, 145).unwrap();
    assert_eq!(
        lab.accept(&scanned_at(&worker, &r, 145), id(1), 150)
            .unwrap(),
        Evidence::AuthenticatedHeld
    );
    drop(lab);
    let mut lab = TerminalLab::open(&path, policy()).unwrap();
    assert_eq!(
        lab.accept(&scanned_at(&worker, &r, 145), id(1), 151)
            .unwrap(),
        Evidence::IdenticalReplayHeld
    );
    for report in [1, 2] {
        lab.observe(
            &scanned(&worker, &observation(&key(2), &r, report).unwrap()),
            152,
        )
        .unwrap();
    }
    assert_eq!(lab.state(scope().transfer).unwrap(), "consumed_held");
    assert_eq!(lab.observations(scope().transfer).unwrap().len(), 2);
    let report: Value = json!({"schema":"hawala-optical-media-TEST-v1","test_only":true,
        "conditions":{"qr_version":17,"ecc":"M","quiet_zone_modules":4,
            "pixel_model":"8-bit grayscale; integer nearest-neighbor scaling; no perspective, glare, sensor noise or compressed codec",
            "worker_limits":{"max_side":MAX_SIDE,"max_pixels":MAX_PIXELS,"address_bytes":ADDRESS_LIMIT,
                "wall_ms":WALL_LIMIT.as_millis(),"cpu_seconds":2,"output_file_bytes":1024,"open_files":32},
            "assembly_seconds":ASSEMBLY_SECS,"max_image_observations":32},
        "fixtures":fixtures,"decoding":rows,"sequences":sequences,
        "journal":{"after_exchange":"AuthenticatedHeld","after_reopen_replay":"IdenticalReplayHeld",
            "after_conflicting_reports":"consumed_held","retained_observations":2},
        "authority":"held TEST evidence only; no cash, custody, settlement, offline spending or Iroh identity capability"});
    std::fs::write(
        output.join("measurements.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
