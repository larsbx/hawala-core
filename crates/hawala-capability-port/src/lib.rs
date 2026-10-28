//! `HawalaCapabilityV1` local port: authenticated, versioned calls from a host
//! platform (for example a biz-kit installation) to its Hawala service, and
//! signed responses back. See ADR 0005 gaps G6 and G8.
//!
//! Admission decides only that a call may be *processed*. It is not a grant
//! of financial authority, a reservation, or evidence of payment. Participant
//! consent is never taken from the host's word: the service re-verifies the
//! participant-signed object itself and refuses consent signed by any key the
//! host holds. Transaction-QR nonce consumption stays with the financial
//! authority, atomically with its reservation (see `hawala-qr-auth`).
use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hawala_identity_core::{IdentityId, ReplayGuard};
use hawala_interaction::AgreedInteraction;
use hawala_protocol::SignedTerms;
use hawala_qr_auth::{verify_signed, SignedAuthorization, VerifiedAuthorization};

pub const VERSION: u16 = 1;
pub const MAX_CALL_TTL_SECS: u64 = 60;

const CALL_DOMAIN: &[u8] = b"hawala/capability-port/call/v1\0";
const RESPONSE_DOMAIN: &[u8] = b"hawala/capability-port/response/v1\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidCall,
    InvalidPolicy,
    UnsupportedVersion,
    WrongService,
    UnknownHost,
    WrongSigner,
    InvalidSignature,
    NotYetValid,
    Expired,
    TtlTooLong,
    UnknownGrant,
    GrantMismatch,
    OperationNotGranted,
    WrongOperation,
    BodyMismatch,
    HostSignedForParticipant,
    Participant(hawala_qr_auth::Error),
    WrongCall,
    Replay,
    Storage,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Operation {
    QueryStatus,
    ProposeTerms,
    SubmitAuthorization,
}

impl Operation {
    fn tag(self) -> &'static [u8] {
        match self {
            Self::QueryStatus => b"query-status",
            Self::ProposeTerms => b"propose-terms",
            Self::SubmitAuthorization => b"submit-authorization",
        }
    }
}

/// The host's grant that authorizes this call: an opaque grant ID and a
/// digest of its exact content. Hawala does not interpret host grants; it
/// only checks them against the locally pinned `PortPolicy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantRef {
    pub id: [u8; 16],
    pub digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostCall {
    pub version: u16,
    pub host: IdentityId,
    pub service: IdentityId,
    pub grant: GrantRef,
    pub operation: Operation,
    pub body_digest: [u8; 32],
    pub nonce: [u8; 32],
    pub issued_at: u64,
    pub expires_at: u64,
}

impl HostCall {
    fn validate(&self) -> Result<()> {
        if self.host == self.service || self.nonce == [0; 32] || self.expires_at <= self.issued_at {
            return Err(Error::InvalidCall);
        }
        Ok(())
    }

    pub fn signing_bytes(&self) -> Vec<u8> {
        let version = self.version.to_be_bytes();
        let issued = self.issued_at.to_be_bytes();
        let expires = self.expires_at.to_be_bytes();
        encode(
            CALL_DOMAIN,
            &[
                &version,
                self.host.as_bytes(),
                self.service.as_bytes(),
                &self.grant.id,
                &self.grant.digest,
                self.operation.tag(),
                &self.body_digest,
                &self.nonce,
                &issued,
                &expires,
            ],
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedHostCall {
    call: HostCall,
    signature: [u8; 64],
}

impl SignedHostCall {
    /// Reassemble a call received from the host. Nothing is trusted until
    /// `admit_call` verifies it.
    pub fn from_parts(call: HostCall, signature: [u8; 64]) -> Self {
        Self { call, signature }
    }

    pub fn call(&self) -> &HostCall {
        &self.call
    }

    pub fn signature(&self) -> &[u8; 64] {
        &self.signature
    }

    /// Commits to the exact signed call; responses are bound to it.
    pub fn digest(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new_derive_key("hawala/capability-port/call-digest/v1");
        h.update(&self.call.signing_bytes());
        h.update(&self.signature);
        *h.finalize().as_bytes()
    }
}

pub fn sign_call(key: &SigningKey, call: HostCall) -> Result<SignedHostCall> {
    call.validate()?;
    if IdentityId::from_signing_key(key) != call.host {
        return Err(Error::WrongSigner);
    }
    let signature = key.sign(&call.signing_bytes()).to_bytes();
    Ok(SignedHostCall { call, signature })
}

/// Digest the service computes over a request body it actually received.
pub fn body_digest(body: &[u8]) -> [u8; 32] {
    *blake3::Hasher::new_derive_key("hawala/capability-port/body/v1")
        .update(body)
        .finalize()
        .as_bytes()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrantPolicy {
    pub digest: [u8; 32],
    pub operations: BTreeSet<Operation>,
}

/// The service's local, pinned view of its host. `host_keys` lists every key
/// the host holds (node and operator keys, including `host`); none of them
/// may ever appear as a participant's signer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortPolicy {
    pub service: IdentityId,
    pub host: IdentityId,
    pub host_keys: BTreeSet<IdentityId>,
    pub grants: BTreeMap<[u8; 16], GrantPolicy>,
    pub max_ttl_secs: u64,
}

impl PortPolicy {
    fn validate(&self) -> Result<()> {
        if !self.host_keys.contains(&self.host)
            || self.host_keys.contains(&self.service)
            || self.max_ttl_secs == 0
            || self.max_ttl_secs > MAX_CALL_TTL_SECS
        {
            return Err(Error::InvalidPolicy);
        }
        Ok(())
    }
}

/// Proof that one call passed port admission, and nothing more.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmittedCall {
    call_digest: [u8; 32],
    grant_id: [u8; 16],
    operation: Operation,
}

impl AdmittedCall {
    pub fn call_digest(&self) -> [u8; 32] {
        self.call_digest
    }

    pub fn grant_id(&self) -> [u8; 16] {
        self.grant_id
    }

    pub fn operation(&self) -> Operation {
        self.operation
    }
}

/// Verify a host call against the pinned policy and the digest of the body
/// the service received, then consume its nonce. Every check runs before
/// consumption, so a rejected call never burns its nonce.
pub fn admit_call(
    policy: &PortPolicy,
    signed: &SignedHostCall,
    received_body_digest: [u8; 32],
    now: u64,
    guard: &impl ReplayGuard,
) -> Result<AdmittedCall> {
    policy.validate()?;
    let c = &signed.call;
    c.validate()?;
    if c.version != VERSION {
        return Err(Error::UnsupportedVersion);
    }
    if c.service != policy.service {
        return Err(Error::WrongService);
    }
    if c.host != policy.host {
        return Err(Error::UnknownHost);
    }
    verify(&c.host, &c.signing_bytes(), &signed.signature)?;
    if now < c.issued_at {
        return Err(Error::NotYetValid);
    }
    if now >= c.expires_at {
        return Err(Error::Expired);
    }
    if c.expires_at - c.issued_at > policy.max_ttl_secs {
        return Err(Error::TtlTooLong);
    }
    let grant = policy.grants.get(&c.grant.id).ok_or(Error::UnknownGrant)?;
    if grant.digest != c.grant.digest {
        return Err(Error::GrantMismatch);
    }
    if !grant.operations.contains(&c.operation) {
        return Err(Error::OperationNotGranted);
    }
    if c.body_digest != received_body_digest {
        return Err(Error::BodyMismatch);
    }
    match guard.consume_once(c.service, c.host, c.nonce) {
        Ok(true) => Ok(AdmittedCall {
            call_digest: signed.digest(),
            grant_id: c.grant.id,
            operation: c.operation,
        }),
        Ok(false) => Err(Error::Replay),
        Err(_) => Err(Error::Storage),
    }
}

/// What the service expects of a participant's transaction-QR consent.
#[derive(Clone, Copy, Debug)]
pub struct AuthorizationContext<'a> {
    pub accepted: &'a SignedTerms,
    pub agreed: &'a AgreedInteraction,
    pub qr_nonce: [u8; 32],
    pub challenge: [u8; 32],
    pub session_digest: [u8; 32],
    pub verifier: IdentityId,
}

/// Admit a `SubmitAuthorization` call. The service verifies the
/// participant-signed consent itself, refuses consent signed by any host key
/// (G8), and requires the call to commit to exactly that signed object.
pub fn admit_authorization(
    policy: &PortPolicy,
    signed: &SignedHostCall,
    authorization: &SignedAuthorization,
    expected: &AuthorizationContext<'_>,
    now: u64,
    guard: &impl ReplayGuard,
) -> Result<(AdmittedCall, VerifiedAuthorization)> {
    if signed.call.operation != Operation::SubmitAuthorization {
        return Err(Error::WrongOperation);
    }
    let verified = verify_signed(
        authorization,
        expected.accepted,
        expected.agreed,
        expected.qr_nonce,
        expected.challenge,
        expected.session_digest,
        expected.verifier,
        now,
    )
    .map_err(Error::Participant)?;
    if policy.host_keys.contains(&verified.holder()) {
        return Err(Error::HostSignedForParticipant);
    }
    let admitted = admit_call(policy, signed, verified.proof_digest(), now, guard)?;
    Ok((admitted, verified))
}

/// Port-level status only. `Received` means the call was accepted for
/// processing; it carries no financial meaning (see ADR 0005 gap G1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Rejected,
    Held,
    Received,
}

impl Status {
    fn tag(self) -> &'static [u8] {
        match self {
            Self::Rejected => b"rejected",
            Self::Held => b"held",
            Self::Received => b"received",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceResponse {
    pub call_digest: [u8; 32],
    pub status: Status,
    pub issued_at: u64,
}

impl ServiceResponse {
    fn signing_bytes(&self) -> Vec<u8> {
        let issued = self.issued_at.to_be_bytes();
        encode(
            RESPONSE_DOMAIN,
            &[&self.call_digest, self.status.tag(), &issued],
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedResponse {
    response: ServiceResponse,
    signature: [u8; 64],
}

impl SignedResponse {
    pub fn from_parts(response: ServiceResponse, signature: [u8; 64]) -> Self {
        Self {
            response,
            signature,
        }
    }

    pub fn signing_bytes(&self) -> Vec<u8> {
        self.response.signing_bytes()
    }
}

pub fn sign_response(service_key: &SigningKey, response: ServiceResponse) -> SignedResponse {
    let signature = service_key.sign(&response.signing_bytes()).to_bytes();
    SignedResponse {
        response,
        signature,
    }
}

/// Host-side check that a response comes from the pinned service and answers
/// exactly the call the host sent.
pub fn verify_response(
    pinned_service: IdentityId,
    signed: &SignedResponse,
    expected_call_digest: [u8; 32],
) -> Result<ServiceResponse> {
    verify(
        &pinned_service,
        &signed.response.signing_bytes(),
        &signed.signature,
    )?;
    if signed.response.call_digest != expected_call_digest {
        return Err(Error::WrongCall);
    }
    Ok(signed.response.clone())
}

fn verify(signer: &IdentityId, message: &[u8], signature: &[u8; 64]) -> Result<()> {
    VerifyingKey::from_bytes(signer.as_bytes())
        .map_err(|_| Error::InvalidSignature)?
        .verify_strict(message, &Signature::from_bytes(signature))
        .map_err(|_| Error::InvalidSignature)
}

fn encode(domain: &[u8], parts: &[&[u8]]) -> Vec<u8> {
    let mut out = domain.to_vec();
    for part in parts {
        out.extend_from_slice(&(part.len() as u32).to_be_bytes());
        out.extend_from_slice(part);
    }
    out
}
