//! TEST ONLY: one bounded camera-acquisition IPC packet, not a driver.
//!
//! This record is untrusted even when it arrives from a local process.
//! It includes NO timestamp, key, signer, session identity, custody claim,
//! financial instruction or authorization bit. Trusted receipt time and
//! scan quotas are provided by the parent, not camera/IPC metadata.
use crate::{CaptureFrame, CaptureLayout, MAX_CAPTURE_BYTES, MAX_ROW_PADDING, MAX_SIDE};

const MAGIC: &[u8; 8] = b"HWCAM001";
pub const HEADER_LEN: usize = 28;
pub const MAX_PACKET_BYTES: usize = HEADER_LEN + MAX_CAPTURE_BYTES;

fn value(input: &[u8], at: usize) -> Result<usize, String> {
    let raw: [u8; 4] = input[at..at + 4]
        .try_into().map_err(|_| "capture integer")?;
    Ok(u32::from_be_bytes(raw) as usize)
}
fn format_code(layout: CaptureLayout) -> u8 {
    match layout {
        CaptureLayout::Gray8 => 1,
        CaptureLayout::Rgb888 => 2,
        CaptureLayout::Yuyv422 => 3,
    }
}
fn interpret(v: u8) -> Result<CaptureLayout, String> {
    match v {
        1 => Ok(CaptureLayout::Gray8),
        2 => Ok(CaptureLayout::Rgb888),
        3 => Ok(CaptureLayout::Yuyv422),
        _ => Err("unsupported capture layout".into()),
    }
}
fn check_shape(
    width: usize,
    height: usize,
    stride: usize,
    layout: CaptureLayout,
    len: usize,
) -> Result<(), String> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err("invalid capture dimensions".into());
    }
    if layout == CaptureLayout::Yuyv422 && !width.is_multiple_of(2) {
        return Err("uneven YUYV422 width".into());
    }
    let bytes_per_pixel = match layout {
        CaptureLayout::Gray8 => 1,
        CaptureLayout::Rgb888 => 3,
        CaptureLayout::Yuyv422 => 2,
    };
    let row = width.checked_mul(bytes_per_pixel).ok_or("row overflow")?;
    if stride < row
        || stride > row.checked_add(MAX_ROW_PADDING).ok_or("padding overflow")?
    {
        return Err("invalid capture stride".into());
    }
    let required = stride.checked_mul(height).ok_or("capture product overflow")?;
    if required != len || required > MAX_CAPTURE_BYTES {
        return Err("invalid capture payload length".into());
    }
    Ok(())
}

/// Encode one deterministic **test capture**. This is not a financial message
/// format, nor a privacy-sensitive raw-camera recording mechanism.
pub fn encode_camera_packet(frame: &CaptureFrame<'_>) -> Result<Vec<u8>, String> {
    check_shape(
        frame.width, frame.height, frame.row_stride, frame.layout, frame.bytes.len(),
    )?;
    let mut out = Vec::with_capacity(HEADER_LEN + frame.bytes.len());
    out.extend_from_slice(MAGIC);
    out.push(format_code(frame.layout));
    out.extend_from_slice(&[0; 3]); // flags and reserved fields must be zero
    for n in [frame.width, frame.height, frame.row_stride, frame.bytes.len()] {
        let n = u32::try_from(n).map_err(|_| "packet field overflow")?;
        out.extend_from_slice(&n.to_be_bytes());
    }
    out.extend_from_slice(frame.bytes);
    Ok(out)
}

/// Parse an exact, complete single-frame packet without allocating pixels.
/// Declared lengths are checked BEFORE slicing the raw payload. The caller
/// still uses Raster::from_capture followed by the isolated decoder and
/// signed optical-envelope verifier; no packet metadata is trusted.
pub fn parse_camera_packet(packet: &[u8]) -> Result<CaptureFrame<'_>, String> {
    if packet.len() < HEADER_LEN || packet.len() > MAX_PACKET_BYTES {
        return Err("capture IPC packet bounds".into());
    }
    if &packet[..8] != MAGIC || packet[9..12] != [0; 3] {
        return Err("invalid capture magic/flags".into());
    }
    let layout = interpret(packet[8])?;
    let width = value(packet, 12)?;
    let height = value(packet, 16)?;
    let row_stride = value(packet, 20)?;
    let length = value(packet, 24)?;
    if length != packet.len() - HEADER_LEN {
        return Err("truncated or trailing capture payload".into());
    }
    check_shape(width, height, row_stride, layout, length)?;
    Ok(CaptureFrame {
        width,
        height,
        row_stride,
        layout,
        bytes: &packet[HEADER_LEN..],
    })
}
