//! TEST ONLY raster transport. No Hawala identities, signing keys, journals,
//! controller commands, Iroh credentials, file image codecs or video codecs.
//! The worker returns untrusted text. Its parent must independently verify it.
mod capture;
pub use capture::{CaptureFrame, CaptureLayout, MAX_CAPTURE_BYTES, MAX_ROW_PADDING};

use qrcode::{Color, EcLevel, QrCode, Version};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const MAX_SIDE: usize = 1024;
pub const MAX_PIXELS: usize = MAX_SIDE * MAX_SIDE;
pub const MAX_TEXT: usize = 503;
pub const WALL_LIMIT: Duration = Duration::from_secs(2);
pub const ADDRESS_LIMIT: u64 = 256 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"HWRAST01";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raster {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}
impl Raster {
    pub fn new(width: usize, height: usize, pixels: Vec<u8>) -> Result<Self, String> {
        if width == 0
            || height == 0
            || width > MAX_SIDE
            || height > MAX_SIDE
            || pixels.len() != width * height
        {
            return Err("invalid raster bounds".into());
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    /// Fixed QR version 17, M correction, four-module white quiet zone.
    /// Integer nearest-neighbor module rendering; deterministic with Cargo.lock.
    pub fn render(text: &str, scale: usize) -> Result<Self, String> {
        if text.len() > MAX_TEXT || !(1..=8).contains(&scale) {
            return Err("render bounds".into());
        }
        let code = QrCode::with_version(text.as_bytes(), Version::Normal(17), EcLevel::M)
            .map_err(|e| e.to_string())?;
        let side = (code.width() + 8) * scale;
        let mut pixels = vec![255; side * side];
        for y in 0..code.width() {
            for x in 0..code.width() {
                if code[(x, y)] == Color::Dark {
                    for dy in 0..scale {
                        let row = ((y + 4) * scale + dy) * side;
                        pixels[row + (x + 4) * scale..row + (x + 5) * scale].fill(0);
                    }
                }
            }
        }
        Self::new(side, side, pixels)
    }
    pub fn write_wire(&self, out: &mut impl Write) -> std::io::Result<()> {
        out.write_all(MAGIC)?;
        out.write_all(&(self.width as u32).to_be_bytes())?;
        out.write_all(&(self.height as u32).to_be_bytes())?;
        out.write_all(&self.pixels)
    }
    pub fn read_wire(input: &mut impl Read) -> Result<Self, String> {
        let mut header = [0u8; 16];
        input.read_exact(&mut header).map_err(|_| "short header")?;
        if &header[..8] != MAGIC {
            return Err("raster magic".into());
        }
        let width = u32::from_be_bytes(header[8..12].try_into().unwrap()) as usize;
        let height = u32::from_be_bytes(header[12..16].try_into().unwrap()) as usize;
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return Err("raster dimensions".into());
        }
        // Check dimensions BEFORE any pixel allocation; no compressed input.
        let mut pixels = vec![0; width * height];
        input.read_exact(&mut pixels).map_err(|_| "short raster")?;
        let mut extra = [0];
        if input.read(&mut extra).map_err(|_| "raster read")? != 0 {
            return Err("trailing raster".into());
        }
        Self::new(width, height, pixels)
    }
    pub fn pgm(&self) -> Vec<u8> {
        let mut out = format!("P5\n{} {}\n255\n", self.width, self.height).into_bytes();
        out.extend_from_slice(&self.pixels);
        out
    }
}

/// Runs ONLY inside the disposable worker. Multiple detected candidates reject
/// even when they decode to identical text; no first/best-candidate selection.
pub fn decode_raster(image: &Raster) -> Result<Option<String>, String> {
    let mut prepared =
        rqrr::PreparedImage::prepare_from_greyscale(image.width, image.height, |x, y| {
            image.pixels[y * image.width + x]
        });
    let grids = prepared.detect_grids();
    if grids.len() > 1 {
        return Err("ambiguous image".into());
    }
    let Some(grid) = grids.first() else {
        return Ok(None);
    };
    let Ok((_, text)) = grid.decode() else {
        return Ok(None);
    };
    if text.len() > MAX_TEXT || !text.is_ascii() {
        return Err("decoded text bound".into());
    }
    Ok(Some(text))
}

#[derive(Debug)]
pub struct Measurement {
    pub wall_micros: u128,
    pub cpu_micros: u128,
    pub peak_rss_kib: u64,
}
/// Linux resource-bounded child, NOT a hostile-code/security sandbox. No network
/// or filesystem namespace isolation is claimed. Input/output are anonymous files
/// to prevent pipe deadlock; no streaming decoder or unbounded frame queue.
#[cfg(target_os = "linux")]
pub fn decode_isolated(
    worker: &Path,
    image: &Raster,
) -> Result<(Option<String>, Measurement), String> {
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    let mut input = tempfile::tempfile().map_err(|e| e.to_string())?;
    image.write_wire(&mut input).map_err(|e| e.to_string())?;
    input.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut output = tempfile::tempfile().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(worker);
    cmd.env_clear()
        .stdin(Stdio::from(input))
        .stdout(Stdio::from(output.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::null());
    // Safety: this closure only calls async-signal-safe setrlimit before exec.
    unsafe {
        cmd.pre_exec(|| {
            for (resource, limit) in [
                (libc::RLIMIT_AS, ADDRESS_LIMIT),
                (libc::RLIMIT_CPU, 2),
                (libc::RLIMIT_FSIZE, 1024),
                (libc::RLIMIT_NOFILE, 32),
                (libc::RLIMIT_CORE, 0),
            ] {
                let value = libc::rlimit {
                    rlim_cur: limit,
                    rlim_max: limit,
                };
                if libc::setrlimit(resource, &value) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let start = Instant::now();
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut status = 0;
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    loop {
        let waited =
            unsafe { libc::wait4(child.id() as i32, &mut status, libc::WNOHANG, &mut usage) };
        if waited > 0 {
            break;
        }
        if waited < 0 || start.elapsed() >= WALL_LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            return Err("worker wait failure/deadline".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    // A child may have exited while the parent was descheduled. Reaping it is
    // necessary, but late completion must not turn into an accepted reply.
    if start.elapsed() >= WALL_LIMIT || !std::process::ExitStatus::from_raw(status).success() {
        return Err("decoder rejected image/deadline".into());
    }
    output.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    output
        .take((MAX_TEXT + 2) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let text = match bytes.first() {
        Some(0) if bytes.len() == 1 => None,
        Some(1) if bytes.len() > 1 && bytes.len() <= MAX_TEXT + 1 && bytes[1..].is_ascii() => {
            Some(String::from_utf8(bytes[1..].to_vec()).map_err(|_| "worker UTF8")?)
        }
        _ => return Err("invalid bounded worker reply".into()),
    };
    let micros = |t: libc::timeval| (t.tv_sec as u128) * 1_000_000 + t.tv_usec as u128;
    Ok((
        text,
        Measurement {
            wall_micros: start.elapsed().as_micros(),
            cpu_micros: micros(usage.ru_utime) + micros(usage.ru_stime),
            peak_rss_kib: usage.ru_maxrss as u64,
        },
    ))
}
