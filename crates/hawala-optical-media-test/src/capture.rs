//! TEST ONLY: adapter boundary for untrusted, uncompressed camera buffers.
//! No camera driver, video container, network, clock, keys or financial API.
//!
//! An external camera/driver provides raw pixels and an independently
//! authenticated, monotonic receipt time. Device-supplied timestamps and
//! camera metadata are NEVER evidence of transaction freshness.
use crate::{Raster, MAX_PIXELS, MAX_SIDE};

/// Supported uncompressed, packed camera-buffer layouts. YUYV422 stores
/// Y0 U Y1 V for two adjacent pixels; only the Y bytes are retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureLayout {
    Gray8,
    Rgb888,
    Yuyv422,
}

/// Borrowed untrusted camera buffer. Padding at the end of each image row
/// is allowed, but truncated rows or trailing data reject. The whole buffer
/// is consumed in one call; no frame queues or streaming allocations.
#[derive(Clone, Copy, Debug)]
pub struct CaptureFrame<'a> {
    pub width: usize,
    pub height: usize,
    pub row_stride: usize,
    pub layout: CaptureLayout,
    pub bytes: &'a [u8],
}

/// Limit padded row footprint before copying anything into the decoder.
/// This is an independent camera-input budget, not the worker's 1 MiB
/// grayscale raster budget.
pub const MAX_ROW_PADDING: usize = 256;
pub const MAX_CAPTURE_BYTES: usize = 4 * MAX_PIXELS;

impl Raster {
    /// Pure bounded conversion from an untrusted camera capture. The caller
    /// must apply the same finite observation/time budget to this conversion
    /// as to the separate QR decoder. On error the receiver must fail closed.
    pub fn from_capture(frame: &CaptureFrame<'_>) -> Result<Self, String> {
        let (width, height) = (frame.width, frame.height);
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return Err("capture dimensions".into());
        }
        if frame.layout == CaptureLayout::Yuyv422 && !width.is_multiple_of(2) {
            return Err("YUYV422 requires an even pixel width".into());
        }
        let bytes_per_pixel = match frame.layout {
            CaptureLayout::Gray8 => 1,
            CaptureLayout::Rgb888 => 3,
            CaptureLayout::Yuyv422 => 2,
        };
        let row_bytes = width.checked_mul(bytes_per_pixel)
            .ok_or("capture row overflow")?;
        let max_stride = row_bytes.checked_add(MAX_ROW_PADDING)
            .ok_or("capture stride overflow")?;
        if frame.row_stride < row_bytes || frame.row_stride > max_stride {
            return Err("capture row stride".into());
        }
        let total = frame.row_stride.checked_mul(height)
            .ok_or("capture length overflow")?;
        if total > MAX_CAPTURE_BYTES || frame.bytes.len() != total {
            return Err("capture bounded buffer length".into());
        }
        let pixels_len = width.checked_mul(height).ok_or("capture pixels overflow")?;
        if pixels_len > MAX_PIXELS {
            return Err("capture pixel budget".into());
        }
        let mut pixels = vec![0u8; pixels_len];
        for y in 0..height {
            let row = &frame.bytes[y * frame.row_stride..y * frame.row_stride + row_bytes];
            let dst = &mut pixels[y * width..(y + 1) * width];
            match frame.layout {
                CaptureLayout::Gray8 => dst.copy_from_slice(row),
                CaptureLayout::Yuyv422 => {
                    for (x, lum) in dst.iter_mut().enumerate() {
                        *lum = row[x * 2];
                    }
                }
                CaptureLayout::Rgb888 => {
                    for (x, lum) in dst.iter_mut().enumerate() {
                        let off = x * 3;
                        // BT.601 integer approximation, with integer
                        // rounding; no floating point in this path.
                        let r = u32::from(row[off]);
                        let g = u32::from(row[off + 1]);
                        let b = u32::from(row[off + 2]);
                        *lum = ((77 * r + 150 * g + 29 * b + 128) >> 8) as u8;
                    }
                }
            }
        }
        Raster::new(width, height, pixels)
    }
}
