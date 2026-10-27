//! TEST ONLY: one bounded HWCAM001 packet on stdin -> untrusted QR text.
//! This is a PIXEL DECODER, never identity verification, ATM authority,
//! financial settlement or custody. No camera, account or network APIs.
//!
//! On Linux, all QR decoding still happens in the existing resource-limited
//! child process; this program holds no signing keys or financial records.
#![cfg(target_os = "linux")]

use hawala_optical_media_test::{
    decode_isolated, parse_camera_packet, Raster, MAX_PACKET_BYTES,
};
use std::io::{Read, Write};
use std::path::Path;

fn decode_packet(packet: &[u8], worker: &Path) -> Result<Option<String>, String> {
    let capture = parse_camera_packet(packet)?;
    let image = Raster::from_capture(&capture)?;
    let (untrusted_text, _) = decode_isolated(worker, &image)?;
    Ok(untrusted_text)
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 2 {
        return Err("usage: optical-camera-scan-test /absolute/path/optical-qr-decode".into());
    }
    let path = Path::new(&args[1]);
    if !path.is_absolute() {
        return Err("an absolute operator-selected QR decoder path is required".into());
    }
    let kind = std::fs::symlink_metadata(path).map_err(|_| "decoder unavailable")?.file_type();
    if !kind.is_file() || kind.is_symlink() {
        return Err("decoder must be a non-symlink regular executable file".into());
    }
    // Enforce upper bound before interpreting untrusted headers or pixels.
    // A read of MAX+1 bytes detects oversized packets without unbounded input.
    let mut packet = Vec::new();
    std::io::stdin().lock()
        .take((MAX_PACKET_BYTES + 1) as u64)
        .read_to_end(&mut packet)
        .map_err(|_| "input capture packet read failed")?;
    if packet.len() > MAX_PACKET_BYTES {
        return Err("capture input exceeds bounded maximum".into());
    }
    let text = decode_packet(&packet, path)?
        .ok_or("camera image has no readable QR text")?;
    // Output is untrusted QR TEXT, not an authenticated envelope or financial
    // decision. The caller MUST do independent signature, scope, time and
    // durable journal validation. No QR data is printed to stderr.
    std::io::stdout().lock().write_all(text.as_bytes())
        .map_err(|_| "unable to write bounded QR text".into())
}
fn main() {
    if let Err(message) = run() {
        eprintln!("TEST camera QR decoder rejected input: {message}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bad_packet_never_calls_cryptographic_authority() {
        assert!(parse_camera_packet(b"hawala:auth:v1:opaque").is_err());
        assert!(parse_camera_packet(b"hawala:id:v1:public").is_err());
    }
}
