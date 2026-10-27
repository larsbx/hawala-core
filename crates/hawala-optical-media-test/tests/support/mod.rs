#![allow(dead_code)]
use ed25519_dalek::SigningKey;
use hawala_identity_core::IdentityId;
use hawala_optical_media_test::{
    decode_isolated, parse_camera_packet, CaptureFrame, Measurement, Raster,
};
use hawala_optical_test::*;
use std::path::Path;

pub fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
pub fn id(n: u8) -> IdentityId {
    IdentityId::from_signing_key(&key(n))
}
pub fn policy() -> TestPolicy {
    TestPolicy {
        network: [10; 32],
        terminal: id(2),
        authority: id(3),
        epoch: 1,
    }
}
pub fn scope() -> Scope {
    Scope {
        network: [10; 32],
        transfer: [11; 32],
        terms: [12; 32],
        attempt: [13; 32],
        holder: id(1),
        terminal: id(2),
        origin: id(4),
        disburser: id(5),
        coordinated_binding: [14; 32],
        amount_minor: 900,
        fee_minor: 100,
        epoch: 1,
        currency: *b"EUR",
    }
}
pub fn commitment() -> Envelope {
    Envelope::precommit(&key(3), scope(), 100, 200).unwrap()
}
pub fn setup(path: &Path) -> (TerminalLab, Envelope, Envelope, Envelope) {
    let mut lab = TerminalLab::create(path, policy()).unwrap();
    let c = commitment();
    lab.import(&c, 110).unwrap();
    let h = lab
        .challenge(&c, &key(2), [20; 32], [21; 32], 120, 180)
        .unwrap();
    let r = respond(&key(1), id(2), id(3), &c, &h, 130).unwrap();
    (lab, c, h, r)
}
/// The media/controller boundary is in this TEST harness only. The worker has
/// no optical-protocol dependency, keys, scope, trusted time, DB or capability.
pub struct ImageReceiver {
    receiver: CameraReceiver,
    last: u64,
    start_time: u64,
    start_clock: std::time::Instant,
    frames: u8,
    failed: bool,
}
impl ImageReceiver {
    pub fn new(now: u64) -> Self {
        Self {
            receiver: CameraReceiver::new(now),
            last: now,
            start_time: now,
            start_clock: std::time::Instant::now(),
            frames: 0,
            failed: false,
        }
    }
    pub fn receive(
        &mut self,
        worker: &Path,
        image: &Raster,
        now: u64,
    ) -> std::result::Result<(Option<Envelope>, Measurement), String> {
        // Check before spending decoder resources, and poison all adapter errors.
        let result = self.inner(worker, image, now);
        if result.is_err() || matches!(result, Ok((Some(_), _))) {
            self.failed = true;
        }
        result
    }
    /// A bounded raw-camera IPC packet represents ONE received frame,
    /// not an authenticated camera identity or financial instruction.
    /// Rejecting an invalid packet poisons the entire optical assembly.
    pub fn receive_packet(
        &mut self,
        worker: &Path,
        packet: &[u8],
        now: u64,
    ) -> std::result::Result<(Option<Envelope>, Measurement), String> {
        if self.failed || self.frames >= 32
            || now < self.last
            || now.saturating_sub(self.started()) >= ASSEMBLY_SECS
            || self.start_clock.elapsed().as_secs() >= ASSEMBLY_SECS
        {
            self.failed = true;
            return Err("capture IPC receiver poisoned/time/budget".into());
        }
        match parse_camera_packet(packet) {
            Ok(frame) => self.receive_capture(worker, &frame, now),
            Err(e) => {
                self.failed = true;
                Err(e)
            }
        }
    }

    /// Untrusted camera plane -> bounded grayscale -> isolated QR decoder.
    /// A malformed capture permanently poisons this receiver. Even clean
    /// pixels do not establish camera identity or financial authority.
    pub fn receive_capture(
        &mut self,
        worker: &Path,
        capture: &CaptureFrame<'_>,
        now: u64,
    ) -> std::result::Result<(Option<Envelope>, Measurement), String> {
        // Reject known-poisoned, expired and exhausted sessions before even
        // converting an untrusted raw camera plane. The finite raw-buffer
        // budget is per observation; the existing receiver is the time/quota
        // authority, not camera metadata.
        if self.failed
            || now < self.last
            || self.frames >= 32
            || now.saturating_sub(self.started()) >= ASSEMBLY_SECS
            || self.start_clock.elapsed().as_secs() >= ASSEMBLY_SECS
        {
            self.failed = true;
            return Err("capture receiver poisoned/time/budget".into());
        }
        match Raster::from_capture(capture) {
            Ok(image) => self.receive(worker, &image, now),
            Err(e) => {
                self.failed = true;
                Err(e)
            }
        }
    }

    fn inner(
        &mut self,
        worker: &Path,
        image: &Raster,
        now: u64,
    ) -> std::result::Result<(Option<Envelope>, Measurement), String> {
        if self.failed || now < self.last || self.frames >= 32 {
            return Err("adapter poisoned/time/budget".into());
        }
        // The supplied trusted simulation clock AND real elapsed monotonic time
        // are bounded. Decoder latency and blank images cannot renew assembly.
        if now.saturating_sub(self.started()) >= ASSEMBLY_SECS
            || self.start_clock.elapsed().as_secs() >= ASSEMBLY_SECS
        {
            return Err("assembly deadline".into());
        }
        self.frames += 1;
        let (text, metrics) = decode_isolated(worker, image)?;
        // Trusted time includes actual decoding latency; video timestamps are not
        // used as authority. A near-deadline decode cannot extend the window.
        let finished = now
            .checked_add((metrics.wall_micros / 1_000_000) as u64)
            .ok_or("time overflow")?
            .max(
                self.start_time
                    .checked_add(self.start_clock.elapsed().as_secs())
                    .ok_or("time overflow")?,
            );
        self.last = finished;
        let message = if let Some(text) = text {
            self.receiver.receive(&text, finished)
        } else {
            self.receiver.observe_no_text(finished).map(|()| None)
        }
        .map_err(|e| format!("receiver: {e:?}"))?;
        Ok((message, metrics))
    }
    fn started(&self) -> u64 {
        self.start_time
    }
}
pub fn rotate(image: &Raster, turns: usize) -> Raster {
    let mut img = image.clone();
    for _ in 0..turns {
        let (w, h) = (img.width(), img.height());
        let mut p = vec![255; w * h];
        for y in 0..h {
            for x in 0..w {
                p[x * h + (h - 1 - y)] = img.pixels()[y * w + x];
            }
        }
        img = Raster::new(h, w, p).unwrap();
    }
    img
}
pub fn blank() -> Raster {
    Raster::new(372, 372, vec![255; 372 * 372]).unwrap()
}
pub fn obscure(image: &Raster) -> Raster {
    let mut pixels = image.pixels().to_vec();
    // Right half including alignment/finder patterns hidden behind white mask.
    for y in 0..image.height() {
        pixels[y * image.width() + image.width() / 2..(y + 1) * image.width()].fill(255);
    }
    Raster::new(image.width(), image.height(), pixels).unwrap()
}
pub fn display(image: &Raster) -> Raster {
    // Center a scaled QR in a bounded 800x600 gray simulated display capture.
    let mut p = vec![160; 800 * 600];
    let (dx, dy) = ((800 - image.width()) / 2, (600 - image.height()) / 2);
    for y in 0..image.height() {
        for x in 0..image.width() {
            p[(dy + y) * 800 + dx + x] = if image.pixels()[y * image.width() + x] < 128 {
                32
            } else {
                224
            };
        }
    }
    Raster::new(800, 600, p).unwrap()
}
pub fn blur(image: &Raster) -> Raster {
    let (w, h) = (image.width(), image.height());
    let mut p = image.pixels().to_vec();
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let mut sum = 0u32;
            for yy in y - 1..=y + 1 {
                for xx in x - 1..=x + 1 {
                    sum += image.pixels()[yy * w + xx] as u32;
                }
            }
            p[y * w + x] = (sum / 9) as u8;
        }
    }
    Raster::new(w, h, p).unwrap()
}
pub fn pair(a: &Raster, b: &Raster) -> Raster {
    assert_eq!(a.height(), b.height());
    let w = a.width() + b.width();
    let h = a.height();
    let mut p = vec![255; w * h];
    for y in 0..h {
        p[y * w..y * w + a.width()]
            .copy_from_slice(&a.pixels()[y * a.width()..(y + 1) * a.width()]);
        p[y * w + a.width()..(y + 1) * w]
            .copy_from_slice(&b.pixels()[y * b.width()..(y + 1) * b.width()]);
    }
    Raster::new(w, h, p).unwrap()
}
pub fn scanned(worker: &Path, e: &Envelope) -> Envelope {
    scanned_at(worker, e, 130)
}
pub fn scanned_at(worker: &Path, e: &Envelope, now: u64) -> Envelope {
    let mut r = ImageReceiver::new(now);
    let mut found = None;
    for (i, text) in transmit(e).iter().enumerate() {
        found = r
            .receive(
                worker,
                &Raster::render(text, 4).unwrap(),
                now + 1 + i as u64,
            )
            .unwrap()
            .0;
    }
    found.unwrap()
}
