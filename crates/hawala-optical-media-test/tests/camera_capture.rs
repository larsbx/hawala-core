mod support;
use hawala_optical_media_test::{
    CaptureFrame, CaptureLayout, Raster, MAX_ROW_PADDING, MAX_SIDE,
};
use hawala_optical_test::{respond, transmit, Envelope, Evidence, TerminalLab};
use std::path::Path;
use support::{commitment, id, key, policy, scope, ImageReceiver};

fn worker() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_optical-qr-decode"))
}

/// Synthetic raw camera buffers. No camera device or video decoder exists.
fn encode_capture(image: &Raster, layout: CaptureLayout, padding: usize) -> (usize, Vec<u8>) {
    let channels = match layout {
        CaptureLayout::Gray8 => 1,
        CaptureLayout::Rgb888 => 3,
        CaptureLayout::Yuyv422 => 2,
    };
    assert!(padding <= MAX_ROW_PADDING);
    assert!(layout != CaptureLayout::Yuyv422 || image.width() % 2 == 0);
    let stride = image.width() * channels + padding;
    let mut bytes = vec![0x6du8; stride * image.height()];
    for y in 0..image.height() {
        for x in 0..image.width() {
            let v = image.pixels()[y * image.width() + x];
            let o = y * stride + x * channels;
            match layout {
                CaptureLayout::Gray8 => bytes[o] = v,
                CaptureLayout::Rgb888 => bytes[o..o + 3].copy_from_slice(&[v, v, v]),
                CaptureLayout::Yuyv422 => {
                    bytes[o] = v;
                    bytes[o + 1] = 128; // U/V chroma are ignored.
                }
            }
        }
    }
    (stride, bytes)
}
fn decoded_capture(
    message: &Envelope,
    start: u64,
    layout: CaptureLayout,
    padding: usize,
) -> Envelope {
    let mut receiver = ImageReceiver::new(start);
    let mut complete = None;
    for (i, text) in transmit(message).iter().enumerate() {
        let image = Raster::render(text, 4).unwrap();
        let (stride, bytes) = encode_capture(&image, layout, padding);
        let frame = CaptureFrame {
            width: image.width(),
            height: image.height(),
            row_stride: stride,
            layout,
            bytes: &bytes,
        };
        let (found, _) = receiver
            .receive_capture(worker(), &frame, start + 1 + i as u64)
            .unwrap();
        complete = found;
    }
    complete.expect("all image fragments recovered")
}

#[test]
fn camera_like_buffers_roundtrip_all_pixels_and_ignore_padding() {
    let image = Raster::new(4, 2, vec![0, 20, 128, 255, 255, 128, 20, 0]).unwrap();
    for layout in [CaptureLayout::Gray8, CaptureLayout::Rgb888, CaptureLayout::Yuyv422] {
        for padding in [0, 7, MAX_ROW_PADDING] {
            let (stride, bytes) = encode_capture(&image, layout, padding);
            let capture = CaptureFrame {
                width: image.width(),
                height: image.height(),
                row_stride: stride,
                layout,
                bytes: &bytes,
            };
            assert_eq!(Raster::from_capture(&capture).unwrap(), image);
        }
    }
}

#[test]
fn full_held_optical_exchange_from_bounded_gray_rgb_and_yuyv_captures() {
    for layout in [CaptureLayout::Gray8, CaptureLayout::Rgb888, CaptureLayout::Yuyv422] {
        let d = tempfile::tempdir().unwrap();
        let c = decoded_capture(&commitment(), 130, layout, 12);
        assert_eq!(c, commitment());
        let mut lab = TerminalLab::create(d.path().join("terminal"), policy()).unwrap();
        lab.import(&c, 140).unwrap();
        let h = lab
            .challenge(&c, &key(2), [20; 32], [21; 32], 140, 180)
            .unwrap();
        let h = decoded_capture(&h, 140, layout, 4);
        let response = respond(&key(1), id(2), id(3), &c, &h, 145).unwrap();
        let response = decoded_capture(&response, 145, layout, 0);
        assert_eq!(
            lab.accept(&response, id(1), 150).unwrap(),
            Evidence::AuthenticatedHeld
        );
        assert_eq!(lab.state(scope().transfer).unwrap(), "consumed_held");
        drop(lab);
        let mut lab = TerminalLab::open(d.path().join("terminal"), policy()).unwrap();
        assert_eq!(
            lab.accept(&response, id(1), 151).unwrap(),
            Evidence::IdenticalReplayHeld
        );
        assert_eq!(lab.state(scope().transfer).unwrap(), "consumed_held");
    }
}

#[test]
fn rejects_malformed_capture_shapes_before_allocating_decoding() {
    let data = vec![0u8; 32];
    for frame in [
        CaptureFrame {width: 0, height: 1, row_stride: 1, layout: CaptureLayout::Gray8, bytes: &data},
        CaptureFrame {width: usize::MAX, height: 1, row_stride: usize::MAX, layout: CaptureLayout::Rgb888, bytes: &data},
        CaptureFrame {width: MAX_SIDE + 1, height: 1, row_stride: 1025, layout: CaptureLayout::Gray8, bytes: &data},
        CaptureFrame {width: 3, height: 1, row_stride: 6, layout: CaptureLayout::Yuyv422, bytes: &data},
        CaptureFrame {width: 4, height: 1, row_stride: 3, layout: CaptureLayout::Gray8, bytes: &data},
        CaptureFrame {width: 4, height: 1, row_stride: 4 + MAX_ROW_PADDING + 1, layout: CaptureLayout::Gray8, bytes: &data},
        CaptureFrame {width: 4, height: 2, row_stride: 8, layout: CaptureLayout::Yuyv422, bytes: &data},
    ] {
        assert!(Raster::from_capture(&frame).is_err());
    }
    let short = vec![3u8; 7];
    assert!(Raster::from_capture(&CaptureFrame {
        width: 4, height: 2, row_stride: 4, layout: CaptureLayout::Gray8, bytes: &short,
    }).is_err());
    let trailing = vec![3u8; 9];
    assert!(Raster::from_capture(&CaptureFrame {
        width: 4, height: 2, row_stride: 4, layout: CaptureLayout::Gray8, bytes: &trailing,
    }).is_err());
    let odd_yuyv = vec![10u8; 18];
    assert!(Raster::from_capture(&CaptureFrame {
        width: 3, height: 3, row_stride: 6, layout: CaptureLayout::Yuyv422, bytes: &odd_yuyv,
    }).is_err());
}

#[test]
fn malformed_capture_poison_is_permanent_within_bounded_assembly() {
    let frames = transmit(&commitment());
    let good = Raster::render(&frames[0], 4).unwrap();
    let (stride, bytes) = encode_capture(&good, CaptureLayout::Rgb888, 8);
    let mut recv = ImageReceiver::new(130);
    let bad = CaptureFrame {
        width: good.width(),
        height: good.height(),
        row_stride: stride,
        layout: CaptureLayout::Rgb888,
        bytes: &bytes[..bytes.len() - 1],
    };
    assert!(recv.receive_capture(worker(), &bad, 131).is_err());
    let valid = CaptureFrame { bytes: &bytes, ..bad };
    assert!(recv.receive_capture(worker(), &valid, 132).is_err());
}

#[test]
fn unreadable_camera_scene_spends_budget_and_does_not_create_held_evidence() {
    let blank = Raster::new(372, 372, vec![255; 372 * 372]).unwrap();
    let (stride, bytes) = encode_capture(&blank, CaptureLayout::Yuyv422, 0);
    let capture = CaptureFrame {
        width: blank.width(), height: blank.height(), row_stride: stride,
        layout: CaptureLayout::Yuyv422, bytes: &bytes,
    };
    let mut receiver = ImageReceiver::new(130);
    for _ in 0..32 {
        assert!(receiver.receive_capture(worker(), &capture, 131).unwrap().0.is_none());
    }
    assert!(receiver.receive_capture(worker(), &capture, 132).is_err());
}
