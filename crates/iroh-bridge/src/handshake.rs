//! Authenticated bilateral Hawala identity handshake over an Iroh QUIC connection.
//!
//! Four strictly ordered, bounded messages in independent uni-directional
//! QUIC streams. Neither the claimed public key nor a self-reported endpoint
//! key alone authenticates a peer. Both parties MUST already know the other's
//! FULL identity from an independently scanned QR (with name/code displayed).
//!
//! This is identity authentication only; no money transfer authority.
use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::SigningKey;
use hawala_identity_core::{
    sign_endpoint_proof, verify_endpoint_proof, EndpointChallenge, Error as IdentityError,
    IdentityId, ReplayGuard, SignedEndpointProof, VerifiedPeer,
};
use iroh::{endpoint::Connection, Endpoint};

use crate::IDENTITY_ALPN;

const MAGIC: &[u8; 8] = b"HWHAND01";
const PURPOSE: &str = "peer-auth";
const PROOF_LIFETIME: u64 = 120;
const MAX_FRAME: usize = 192;
const HELLO: u8 = 1;
const RESPOND: u8 = 2;
const PROVE: u8 = 3;
const ACK: u8 = 4;

#[derive(Debug)]
pub enum HandshakeError {
    WrongAlpn,
    MalformedFrame,
    UnexpectedIdentity,
    Clock,
    Randomness,
    Transport,
    Identity(IdentityError),
}

impl From<IdentityError> for HandshakeError {
    fn from(value: IdentityError) -> Self { Self::Identity(value) }
}
pub type Result<T> = std::result::Result<T, HandshakeError>;

fn now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH).map_err(|_| HandshakeError::Clock)?.as_secs())
}

fn fresh_nonce() -> Result<[u8; 32]> {
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce).map_err(|_| HandshakeError::Randomness)?;
    if nonce == [0u8; 32] { return Err(HandshakeError::Randomness); }
    Ok(nonce)
}

fn stamp(v: &[u8]) -> Result<u64> {
    Ok(u64::from_be_bytes(v.try_into().map_err(|_| HandshakeError::MalformedFrame)?))
}

fn as_id(v: &[u8]) -> Result<IdentityId> {
    let bytes: [u8; 32] = v.try_into().map_err(|_| HandshakeError::MalformedFrame)?;
    IdentityId::from_bytes(bytes).map_err(HandshakeError::Identity)
}

fn frame(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(9 + payload.len());
    bytes.extend_from_slice(MAGIC);
    bytes.push(kind);
    bytes.extend_from_slice(payload);
    bytes
}

fn payload(bytes: &[u8], kind: u8, len: usize) -> Result<&[u8]> {
    if bytes.len() != 9 + len
        || bytes.get(..8) != Some(MAGIC.as_slice())
        || bytes[8] != kind
    {
        return Err(HandshakeError::MalformedFrame);
    }
    Ok(&bytes[9..])
}

fn proof_from(input: &[u8]) -> Result<SignedEndpointProof> {
    if input.len() != 96 { return Err(HandshakeError::MalformedFrame); }
    let mut sig = [0u8; 64];
    sig.copy_from_slice(&input[32..96]);
    Ok(SignedEndpointProof {subject: as_id(&input[0..32])?, signature: sig})
}

fn proof_bytes(proof: &SignedEndpointProof) -> Vec<u8> {
    let mut result = Vec::with_capacity(96);
    result.extend_from_slice(proof.subject.as_bytes());
    result.extend_from_slice(&proof.signature);
    result
}

fn challenge(
    verifier: IdentityId,
    subject: IdentityId,
    endpoint_key: [u8; 32],
    nonce: [u8; 32],
    issued_at: u64,
    expires_at: u64,
) -> EndpointChallenge {
    EndpointChallenge {
        verifier, subject, endpoint_key, nonce,
        purpose: PURPOSE.to_string(), issued_at, expires_at,
    }
}

fn check_alpn(connection: &Connection) -> Result<()> {
    if connection.alpn() == IDENTITY_ALPN { Ok(()) }
    else { Err(HandshakeError::WrongAlpn) }
}

async fn send(connection: &Connection, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_FRAME { return Err(HandshakeError::MalformedFrame); }
    let mut tx = connection.open_uni().await.map_err(|_| HandshakeError::Transport)?;
    tx.write_all(bytes).await.map_err(|_| HandshakeError::Transport)?;
    tx.finish().map_err(|_| HandshakeError::Transport)?;
    // Do not return while buffered proof/ACK bytes might still be abandoned
    // if the caller drops its QUIC connection immediately after this step.
    match tx.stopped().await.map_err(|_| HandshakeError::Transport)? {
        None => Ok(()),
        Some(_) => Err(HandshakeError::Transport),
    }
}

async fn receive(connection: &Connection, kind: u8, exact_payload: usize) -> Result<Vec<u8>> {
    let mut rx = connection.accept_uni().await.map_err(|_| HandshakeError::Transport)?;
    let bytes = rx.read_to_end(MAX_FRAME).await.map_err(|_| HandshakeError::Transport)?;
    payload(&bytes, kind, exact_payload)?;
    Ok(bytes)
}

fn transcript(hello: &[u8], response: &[u8], proof: &[u8]) -> [u8; 32] {
    let mut h = blake3::Hasher::new_derive_key("hawala-iroh/handshake-transcript/v1");
    for msg in [hello, response, proof] {
        h.update(&(msg.len() as u32).to_be_bytes());
        h.update(msg);
    }
    *h.finalize().as_bytes()
}

/// Initiator: requires a fully authenticated Iroh connection to a peer whose
/// FULL Hawala identity was independently scanned beforehand.
///
/// Never use this result to authorize cash, credits, or bilateral exposure.
/// The durable guard must coordinate across this verifier's devices.
pub async fn initiate(
    own_endpoint: &Endpoint,
    connection: &Connection,
    local_identity_key: &SigningKey,
    scanned_remote: IdentityId,
    guard: &impl ReplayGuard,
) -> Result<VerifiedPeer> {
    check_alpn(connection)?;
    let local = IdentityId::from_signing_key(local_identity_key);
    if local == scanned_remote { return Err(HandshakeError::UnexpectedIdentity); }
    let started = now()?;
    let expiry = started.checked_add(PROOF_LIFETIME).ok_or(HandshakeError::Clock)?;
    let nonce = fresh_nonce()?;
    let mut hello_body = Vec::with_capacity(112);
    hello_body.extend_from_slice(local.as_bytes());
    hello_body.extend_from_slice(scanned_remote.as_bytes());
    hello_body.extend_from_slice(&nonce);
    hello_body.extend_from_slice(&started.to_be_bytes());
    hello_body.extend_from_slice(&expiry.to_be_bytes());
    let hello = frame(HELLO, &hello_body);
    send(connection, &hello).await?;

    let response = receive(connection, RESPOND, 144).await?;
    let r = payload(&response, RESPOND, 144)?;
    let remote_proof = proof_from(&r[0..96])?;
    let mut second_nonce = [0u8; 32];
    second_nonce.copy_from_slice(&r[96..128]);
    let second_issued = stamp(&r[128..136])?;
    let second_expiry = stamp(&r[136..144])?;
    let remote_endpoint = *connection.remote_id().as_bytes();
    let remote_challenge = challenge(local, scanned_remote, remote_endpoint, nonce, started, expiry);
    let verified = verify_endpoint_proof(
        scanned_remote, remote_endpoint, &remote_challenge, &remote_proof, now()?, guard,
    )?;

    let local_challenge = challenge(
        scanned_remote, local, *own_endpoint.id().as_bytes(),
        second_nonce, second_issued, second_expiry,
    );
    let local_proof = sign_endpoint_proof(
        local_identity_key, *own_endpoint.id().as_bytes(), &local_challenge,
    )?;
    let prove = frame(PROVE, &proof_bytes(&local_proof));
    send(connection, &prove).await?;

    let ack = receive(connection, ACK, 32).await?;
    if payload(&ack, ACK, 32)? != transcript(&hello, &response, &prove) {
        return Err(HandshakeError::MalformedFrame);
    }
    Ok(verified)
}

/// Acceptor: expected_initiator must come from the counterparty's scanned
/// full QR identity, NOT from the unauthenticated HELLO or an Iroh address.
/// This function never exposes a VerifiedPeer before checking both proofs.
pub async fn accept(
    own_endpoint: &Endpoint,
    connection: &Connection,
    local_identity_key: &SigningKey,
    expected_initiator: IdentityId,
    guard: &impl ReplayGuard,
) -> Result<VerifiedPeer> {
    check_alpn(connection)?;
    let local = IdentityId::from_signing_key(local_identity_key);
    if local == expected_initiator { return Err(HandshakeError::UnexpectedIdentity); }
    let hello = receive(connection, HELLO, 112).await?;
    let h = payload(&hello, HELLO, 112)?;
    if as_id(&h[0..32])? != expected_initiator || as_id(&h[32..64])? != local {
        return Err(HandshakeError::UnexpectedIdentity);
    }
    let mut first_nonce = [0u8; 32];
    first_nonce.copy_from_slice(&h[64..96]);
    let first_issued = stamp(&h[96..104])?;
    let first_expiry = stamp(&h[104..112])?;

    let challenge_to_local = challenge(
        expected_initiator, local, *own_endpoint.id().as_bytes(),
        first_nonce, first_issued, first_expiry,
    );
    // A forged or expired HELLO cannot solicit an arbitrarily long proof.
    if now()? < first_issued || now()? >= first_expiry {
        return Err(HandshakeError::Clock);
    }
    let local_proof = sign_endpoint_proof(
        local_identity_key, *own_endpoint.id().as_bytes(), &challenge_to_local,
    )?;
    let second_issued = now()?;
    let second_expiry = second_issued.checked_add(PROOF_LIFETIME).ok_or(HandshakeError::Clock)?;
    let second_nonce = fresh_nonce()?;
    let challenge_to_initiator = challenge(
        local, expected_initiator, *connection.remote_id().as_bytes(),
        second_nonce, second_issued, second_expiry,
    );
    let mut resp_body = proof_bytes(&local_proof);
    resp_body.extend_from_slice(&second_nonce);
    resp_body.extend_from_slice(&second_issued.to_be_bytes());
    resp_body.extend_from_slice(&second_expiry.to_be_bytes());
    let response = frame(RESPOND, &resp_body);
    send(connection, &response).await?;

    let prove = receive(connection, PROVE, 96).await?;
    let initiator_proof = proof_from(payload(&prove, PROVE, 96)?)?;
    let verified = verify_endpoint_proof(
        expected_initiator, *connection.remote_id().as_bytes(),
        &challenge_to_initiator, &initiator_proof, now()?, guard,
    )?;
    let ack = frame(ACK, &transcript(&hello, &response, &prove));
    send(connection, &ack).await?;
    Ok(verified)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_validation_rejects_truncation_trailing_bytes_and_wrong_type() {
        let original = frame(HELLO, &[8u8; 112]);
        assert!(payload(&original, HELLO, 112).is_ok());
        assert!(payload(&original[..40], HELLO, 112).is_err());
        let mut appended = original.clone(); appended.push(0);
        assert!(payload(&appended, HELLO, 112).is_err());
        assert!(payload(&original, RESPOND, 112).is_err());
        let mut forged = original; forged[0] = 0;
        assert!(payload(&forged, HELLO, 112).is_err());
    }

    #[test]
    fn transcript_digest_is_order_and_content_sensitive() {
        let a = frame(HELLO, &[1; 112]);
        let b = frame(RESPOND, &[2; 144]);
        let c = frame(PROVE, &[3; 96]);
        assert_ne!(transcript(&a, &b, &c), transcript(&a, &c, &b));
        let mut forged = c.clone(); forged[12] ^= 1;
        assert_ne!(transcript(&a, &b, &c), transcript(&a, &b, &forged));
    }
}
