use crate::{Envelope, Error, Result, ENVELOPE_LEN};

pub const QR_PREFIX: &str = "hawala:optical-test:v1:";
pub const CHUNK_BYTES: usize = 192;
pub const MAX_FRAMES: usize = 8;
pub const ASSEMBLY_SECS: u64 = 10;
const HEADER: usize = 8 + 32 + 2 + 2 + 4;
pub const MAX_QR_CHARS: usize = QR_PREFIX.len() + 2 * (HEADER + CHUNK_BYTES);
fn digest(b: &[u8]) -> [u8; 32] {
    *blake3::Hasher::new_derive_key("hawala/optical-TEST-frames/v1")
        .update(b)
        .finalize()
        .as_bytes()
}
/// QR text adapter. No image/video decoder, network, or camera credentials.
pub fn transmit(message: &Envelope) -> Vec<String> {
    let bytes = message.encode();
    let id = digest(&bytes);
    let count = bytes.len().div_ceil(CHUNK_BYTES);
    bytes
        .chunks(CHUNK_BYTES)
        .enumerate()
        .map(|(i, chunk)| {
            let mut b = b"HWQRT001".to_vec();
            b.extend_from_slice(&id);
            b.extend_from_slice(&(i as u16).to_be_bytes());
            b.extend_from_slice(&(count as u16).to_be_bytes());
            b.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            b.extend_from_slice(chunk);
            format!("{QR_PREFIX}{}", hex::encode(b))
        })
        .collect()
}
/// One bounded assembly per receiver, with a non-renewable trusted deadline and
/// 32 total observations (including duplicates/errors). Dropping it loses only
/// transport progress; durable challenge/consumption records live in TerminalLab.
pub struct CameraReceiver {
    started: u64,
    last: u64,
    observations: u8,
    failed: bool,
    id: Option<[u8; 32]>,
    chunks: Vec<Option<Vec<u8>>>,
}
impl CameraReceiver {
    pub fn new(now: u64) -> Self {
        Self {
            started: now,
            last: now,
            observations: 0,
            failed: false,
            id: None,
            chunks: Vec::new(),
        }
    }
    pub fn receive(&mut self, qr: &str, now: u64) -> Result<Option<Envelope>> {
        if self.failed {
            return Err(Error::Invalid);
        }
        self.observations = self.observations.saturating_add(1);
        let result = self.inner(qr, now);
        if result.is_err() || matches!(result, Ok(Some(_))) {
            self.failed = true;
        }
        result
    }
    fn inner(&mut self, qr: &str, now: u64) -> Result<Option<Envelope>> {
        if now < self.last
            || now.saturating_sub(self.started) >= ASSEMBLY_SECS
            || self.observations > 32
        {
            return Err(Error::Time);
        }
        self.last = now;
        if qr.len() > MAX_QR_CHARS {
            return Err(Error::Invalid);
        }
        let text = qr.strip_prefix(QR_PREFIX).ok_or(Error::Invalid)?;
        // Single textual representation: lowercase hex, no whitespace/padding.
        if !text
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(Error::Invalid);
        }
        let b = hex::decode(text).map_err(|_| Error::Invalid)?;
        if b.len() < HEADER || &b[..8] != b"HWQRT001" {
            return Err(Error::Invalid);
        }
        let id: [u8; 32] = b[8..40].try_into().map_err(|_| Error::Invalid)?;
        let index = u16::from_be_bytes([b[40], b[41]]) as usize;
        let count = u16::from_be_bytes([b[42], b[43]]) as usize;
        let length = u32::from_be_bytes(b[44..48].try_into().map_err(|_| Error::Invalid)?) as usize;
        if length != ENVELOPE_LEN
            || count == 0
            || count > MAX_FRAMES
            || count != length.div_ceil(CHUNK_BYTES)
            || index >= count
            || b.len() != HEADER + (length - index * CHUNK_BYTES).min(CHUNK_BYTES)
        {
            return Err(Error::Invalid);
        }
        match self.id {
            Some(old) if old != id => return Err(Error::Binding),
            None => {
                self.id = Some(id);
                self.chunks = vec![None; count];
            }
            _ => (),
        }
        let chunk = &b[HEADER..];
        if let Some(old) = &self.chunks[index] {
            if old != chunk {
                return Err(Error::Binding);
            }
        } else {
            self.chunks[index] = Some(chunk.to_vec());
        }
        if self.chunks.iter().any(Option::is_none) {
            return Ok(None);
        }
        let mut bytes = Vec::with_capacity(length);
        for c in &self.chunks {
            bytes.extend_from_slice(c.as_ref().ok_or(Error::Invalid)?);
        }
        if digest(&bytes) != id {
            return Err(Error::Binding);
        }
        // Authorship only. Independent pinned-policy/scope checks still required.
        Ok(Some(Envelope::decode(&bytes)?))
    }
}
