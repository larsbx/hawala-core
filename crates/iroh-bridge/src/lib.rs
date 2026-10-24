//! Authenticated Iroh endpoint binding only. This is not a P2P message
//! handler, financial authorization API, discovery service, or settlement.
use hawala_identity_core::{
    verify_endpoint_proof, EndpointChallenge, IdentityId, ReplayGuard,
    Result, SignedEndpointProof, VerifiedPeer,
};
use iroh::endpoint::Connection;

/// Protocol selection must be enforced for inbound and outbound connections.
pub const IDENTITY_ALPN: &[u8] = b"hawala/peer-identity/1";

/// The remote EndpointId is cryptographically authenticated by Iroh QUIC.
/// The QR-scanned Hawala identity is independently authenticated by signature.
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
