mod support;
use hawala_optical_media_test::{
    encode_camera_packet, parse_camera_packet, CaptureFrame, CaptureLayout,
    Raster, HEADER_LEN, MAX_CAPTURE_BYTES, MAX_PACKET_BYTES,
};
use hawala_optical_test::{respond, transmit, Envelope, Evidence, TerminalLab};
use std::path::Path;
use support::{commitment, id, key, policy, scope, ImageReceiver};

fn worker() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_optical-qr-decode"))
}
fn pack(image: &Raster, layout: CaptureLayout, padding: usize) -> Vec<u8> {
    let count = match layout {
        CaptureLayout::Gray8 => 1,
        CaptureLayout::Rgb888 => 3,
        CaptureLayout::Yuyv422 => 2,
    };
    let stride = image.width() * count + padding;
    let mut bytes = vec![0x4du8; stride * image.height()];
    for y in 0..image.height() {
        for x in 0..image.width() {
            let value = image.pixels()[y * image.width() + x];
            let index = y * stride + x * count;
            match layout {
                CaptureLayout::Gray8 => bytes[index] = value,
                CaptureLayout::Rgb888 => bytes[index..index + 3].fill(value),
                CaptureLayout::Yuyv422 => {
                    bytes[index] = value;
                    bytes[index + 1] = 128;
                }
            }
        }
    }
    encode_camera_packet(&CaptureFrame {
        width: image.width(), height: image.height(),
        row_stride: stride, layout, bytes: &bytes,
    }).unwrap()
}
fn roundtrip(e: &Envelope, start: u64, layout: CaptureLayout) -> Envelope {
    let mut receiver = ImageReceiver::new(start);
    let mut complete = None;
    for (i, text) in transmit(e).iter().enumerate() {
        let rendered = Raster::render(text, 4).unwrap();
        let packet = pack(&rendered, layout, 9);
        let frame = parse_camera_packet(&packet).unwrap();
        assert_eq!(Raster::from_capture(&frame).unwrap(), rendered);
        complete = receiver.receive_packet(worker(), &packet, start + 1 + i as u64)
            .unwrap().0;
    }
    complete.expect("three bounded camera frames must assemble")
}

#[test]
fn all_three_camera_ipc_layouts_reach_only_durable_held_evidence() {
    for layout in [CaptureLayout::Gray8, CaptureLayout::Rgb888, CaptureLayout::Yuyv422] {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("test-terminal");
        let mut lab = TerminalLab::create(&path, policy()).unwrap();
        let commit = roundtrip(&commitment(), 130, layout);
        lab.import(&commit, 140).unwrap();
        let challenge = lab.challenge(
            &commit, &key(2), [20;32], [21;32], 140, 180,
        ).unwrap();
        let challenge = roundtrip(&challenge, 140, layout);
        let response = respond(
            &key(1), id(2), id(3), &commit, &challenge, 145,
        ).unwrap();
        let response = roundtrip(&response, 145, layout);
        assert_eq!(lab.accept(&response, id(1), 150).unwrap(), Evidence::AuthenticatedHeld);
        assert_eq!(lab.state(scope().transfer).unwrap(), "consumed_held");
        drop(lab);
        let mut lab = TerminalLab::open(&path, policy()).unwrap();
        assert_eq!(lab.accept(&response, id(1), 151).unwrap(), Evidence::IdenticalReplayHeld);
        assert_eq!(lab.state(scope().transfer).unwrap(), "consumed_held");
    }
}

#[test]
fn camera_ipc_packets_are_exact_framing_without_timestamp_or_permission_fields() {
    let image = Raster::new(4, 2, vec![0, 40, 90, 255, 255, 90, 40, 0]).unwrap();
    for layout in [CaptureLayout::Gray8, CaptureLayout::Rgb888, CaptureLayout::Yuyv422] {
        let packet = pack(&image, layout, 17);
        assert_eq!(&packet[..8], b"HWCAM001");
        assert_eq!(packet[9..12], [0;3]);
        assert_eq!(packet.len(), HEADER_LEN + (image.width() * match layout {
            CaptureLayout::Gray8 => 1,
            CaptureLayout::Rgb888 => 3,
            CaptureLayout::Yuyv422 => 2,
        } + 17) * image.height());
        let frame = parse_camera_packet(&packet).unwrap();
        assert_eq!(Raster::from_capture(&frame).unwrap(), image);
        // There is no time/identity/operator in the transport header.
        assert_eq!(HEADER_LEN, 28);
    }
}

#[test]
fn header_bombs_trailing_short_unknown_format_and_padding_fail_closed() {
    let image = Raster::new(4, 2, vec![0u8; 8]).unwrap();
    let packet = pack(&image, CaptureLayout::Gray8, 0);
    for changed_at in [0usize, 8, 9, 10, 11, 12, 16, 20, 24] {
        let mut bad = packet.clone();
        bad[changed_at] = 0xff;
        assert!(parse_camera_packet(&bad).is_err(), "changed offset {changed_at}");
    }
    let mut bad = packet.clone();
    bad.push(0);
    assert!(parse_camera_packet(&bad).is_err());
    assert!(parse_camera_packet(&packet[..packet.len() - 1]).is_err());
    assert!(parse_camera_packet(&packet[..HEADER_LEN-1]).is_err());
    assert!(parse_camera_packet(&vec![0u8; MAX_PACKET_BYTES + 1]).is_err());
    // Header claims 4 MiB of pixels although only eight bytes follow.
    let mut bad = packet.clone();
    bad[24..28].copy_from_slice(&(MAX_CAPTURE_BYTES as u32).to_be_bytes());
    assert!(parse_camera_packet(&bad).is_err());
    // Legacy identity QR and financial policy bytes have no valid frame code.
    assert!(parse_camera_packet(b"hawala:id:v1:").is_err());
    assert!(parse_camera_packet(b"hawala:auth:v1:").is_err());
}
#[test]
fn invalid_ipc_packet_poisons_session_and_preserves_nonrenewable_quota() {
    let images: Vec<_> = transmit(&commitment())
        .iter().map(|s| Raster::render(s, 4).unwrap()).collect();
    let good = pack(&images[0], CaptureLayout::Gray8, 0);
    let mut corrupted = good.clone();
    corrupted[8] = 100;
    let mut recv = ImageReceiver::new(130);
    assert!(recv.receive_packet(worker(), &corrupted, 131).is_err());
    assert!(recv.receive_packet(worker(), &good, 132).is_err());
    let mut recv = ImageReceiver::new(130);
    assert!(recv.receive_packet(worker(), &good, 131).unwrap().0.is_none());
    assert!(recv.receive_packet(worker(), &good, 140).is_err());
    assert!(recv.receive_packet(worker(), &good, 131).is_err());
    // A missing second/third QR fragment can never be filled by padding
    // the transport or extending the deadline.
}
#[test]
fn bounded_raw_frame_file_is_data_not_a_camera_authority() {
    use std::io::{Read, Seek, Write};
    let image = Raster::render(&transmit(&commitment())[0], 4).unwrap();
    let bytes = pack(&image, CaptureLayout::Yuyv422, 5);
    let mut spool = tempfile::tempfile().unwrap();
    spool.write_all(&bytes).unwrap();
    spool.rewind().unwrap();
    // A prospective acquisition process may write one bounded packet to a
    // preallocated file/IPC channel; the caller caps bytes, not timestamps.
    let mut limited = Vec::new();
    spool.take((MAX_PACKET_BYTES + 1) as u64).read_to_end(&mut limited).unwrap();
    assert_eq!(parse_camera_packet(&limited).unwrap().width, image.width());
    let mut recv = ImageReceiver::new(130);
    assert!(recv.receive_packet(worker(), &limited, 131).unwrap().0.is_none());
}
