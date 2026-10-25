//! TEST ONLY. Optical transport of signed evidence, never a dispense capability.
//! Security cameras supply untrusted QR text. Identity keys and trusted time are
//! independent of the camera. No Iroh VerifiedPeer or hardware command is made.
mod frames;
mod store;
mod wire;
pub use frames::*;
pub use store::*;
pub use wire::*;

#[derive(Debug)]
pub enum Error {
    Invalid,
    Signature,
    Binding,
    Time,
    Replay,
    Storage(rusqlite::Error),
    Io(std::io::Error),
}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Storage(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;
