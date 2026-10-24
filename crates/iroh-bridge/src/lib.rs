//! Authenticated Iroh endpoint binding, and a mutual on-wire QR identity
//! handshake. Neither a VerifiedPeer nor an Iroh QUIC connection alone grants
//! financial authorization or the right to accept/disburse funds.
pub mod handshake;

use hawala_identity_core::{
    verify_endpoint_proof, EndpointChallenge, IdentityId, ReplayGuard,
    Result, SignedEndpointProof, VerifiedPeer,
};
use iroh::endpoint::Connection;

/// Only ALPN accepted for identity handshake; not a financial-operation ALPN.
pub const IDENTITY_ALPN: &[u8] = b"hawala/peer-identity/1";

/// Extracts remote EndpointId from TLS authenticated QUIC peer certificate.
/// `scanned_identity` MUST be obtained from QR, never from the wire alone.
pub fn verify_connected_peer(
    connection: &Connection,
    scanned_identity: IdentityId,
    challenge: &EndpointChallenge,
    response: &SignedEndpointProof,
    now: u64,
    durable_guard: &impl ReplayGuard,
) -> Result<VerifiedPeer> {
    if connection.alpn() != IDENTITY_ALPN {
        return Err(hawala_identity_core::Error::PermissionDenied);
    }
    verify_endpoint_proof(
        scanned_identity, *connection.remote_id().as_bytes(),
        challenge, response, now, durable_guard,
    )
}
