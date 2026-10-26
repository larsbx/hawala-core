//! Disposable TEST decoder; only raw bounded grayscale input and untrusted text.
use hawala_optical_media_test::{decode_raster, Raster};
use std::io::Write;
fn run() -> Result<(), String> {
    let raster = Raster::read_wire(&mut std::io::stdin().lock())?;
    let text = decode_raster(&raster)?;
    let mut out = std::io::stdout().lock();
    out.write_all(&[u8::from(text.is_some())])
        .map_err(|e| e.to_string())?;
    if let Some(text) = text {
        out.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn main() {
    if run().is_err() {
        std::process::exit(2);
    }
}
