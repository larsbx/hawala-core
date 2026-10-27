mod support;

use hawala_optical_media_test::{
    encode_camera_packet, CaptureFrame, CaptureLayout, Raster, MAX_PACKET_BYTES,
};
use hawala_optical_test::{transmit, Evidence, TerminalLab};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use support::{commitment, id, key, policy, scope};

fn worker() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_optical-qr-decode"))
}
fn scanner() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_optical-camera-scan-test"))
}
fn packet(text: &str) -> Vec<u8> {
    let raster = Raster::render(text, 4).unwrap();
    let mut pixels = vec![0u8; raster.pixels().len() * 2];
    for (x, byte) in raster.pixels().iter().enumerate() {
        pixels[x * 2] = *byte;
        pixels[x * 2 + 1] = 128; // fake UV chroma
    }
    encode_camera_packet(&CaptureFrame {
        width: raster.width(),
        height: raster.height(),
        row_stride: raster.width() * 2,
        layout: CaptureLayout::Yuyv422,
        bytes: &pixels,
    }).unwrap()
}
fn scan(frame: &[u8]) -> std::process::Output {
    let mut child = Command::new(scanner())
        .arg(worker())
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(frame).unwrap();
    }
    child.wait_with_output().unwrap()
}
fn scanned(message: &hawala_optical_test::Envelope) -> hawala_optical_test::Envelope {
    use hawala_optical_test::CameraReceiver;
    let mut r = CameraReceiver::new(130);
    let mut found = None;
    for (n,frame) in transmit(message).iter().enumerate() {
        let out = scan(&packet(frame));
        assert!(out.status.success(), "{:?}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(out.stdout, frame.as_bytes());
        let txt = std::str::from_utf8(&out.stdout).unwrap();
        found = r.receive(txt, 131 + n as u64).unwrap();
    }
    found.unwrap()
}
#[test]
fn raw_camera_packet_through_two_executables_then_signed_held_journal() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("terminal");
    let c = scanned(&commitment());
    let mut lab = TerminalLab::create(&path, policy()).unwrap();
    lab.import(&c, 140).unwrap();
    let h = lab.challenge(&c, &key(2), [20;32], [21;32], 140, 180).unwrap();
    let h = scanned(&h);
    let r = hawala_optical_test::respond(&key(1), id(2), id(3), &c, &h, 145).unwrap();
    let r = scanned(&r);
    assert_eq!(lab.accept(&r, id(1), 150).unwrap(), Evidence::AuthenticatedHeld);
    assert_eq!(lab.state(scope().transfer).unwrap(), "consumed_held");
    drop(lab);
    let mut lab = TerminalLab::open(&path, policy()).unwrap();
    assert_eq!(lab.accept(&r, id(1), 151).unwrap(), Evidence::IdenticalReplayHeld);
    assert_eq!(lab.state(scope().transfer).unwrap(), "consumed_held");
}

#[test]
fn malformed_or_missing_camera_pixels_cannot_mint_qr_text() {
    for input in [
        b"hawala:auth:v1:invalid".as_slice(),
        b"hawala:id:v1:invalid".as_slice(),
        b"HWCAM001".as_slice(),
    ] {
        let out = scan(input);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
    }
    let images = transmit(&commitment());
    let mut changed = packet(&images[0]);
    changed.truncate(changed.len()-1);
    let out=scan(&changed);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let mut changed = packet(&images[0]);
    changed.push(0);
    let out=scan(&changed);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
}

#[test]
fn bounded_input_never_grows_beyond_one_raw_camera_packet() {
    // The scanner's capped stdin read rejects oversize before interpreting
    // any camera metadata or running the QR worker.
    let over = vec![0xff; MAX_PACKET_BYTES + 1];
    let out = scan(&over);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
}
