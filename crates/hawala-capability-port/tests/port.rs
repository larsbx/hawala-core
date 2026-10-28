//! G6 (port authentication) and G8 (participant keys stay on participant
//! devices) for the HawalaCapabilityV1 local port. Admission here grants no
//! financial authority; it only decides whether a host call may be processed.
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Mutex;

use ed25519_dalek::SigningKey;
use hawala_capability_port::*;
use hawala_identity_core::{IdentityId, ReplayGuard};
use hawala_interaction::{
    agree, sign_intent, Action, AgreedInteraction, InteractionIntent, Participant,
};
use hawala_protocol::{accept_terms, sign_terms, SignedTerms, Terms};
use hawala_qr_auth::{sign_authorization, AuthorizationRequest, SignedAuthorization};

const HOST: u8 = 40;
const HOST_NODE: u8 = 41;
const SERVICE: u8 = 50;
const GRANT: [u8; 16] = [7; 16];
const NOW: u64 = 135;

fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}

fn id(n: u8) -> IdentityId {
    IdentityId::from_signing_key(&key(n))
}

#[derive(Default)]
struct Guard(Mutex<HashSet<(IdentityId, IdentityId, [u8; 32])>>);

impl ReplayGuard for Guard {
    fn consume_once(
        &self,
        v: IdentityId,
        c: IdentityId,
        n: [u8; 32],
    ) -> hawala_identity_core::Result<bool> {
        Ok(self.0.lock().unwrap().insert((v, c, n)))
    }
}

fn policy(operations: &[Operation]) -> PortPolicy {
    PortPolicy {
        service: id(SERVICE),
        host: id(HOST),
        host_keys: BTreeSet::from([id(HOST), id(HOST_NODE)]),
        grants: BTreeMap::from([(
            GRANT,
            GrantPolicy {
                digest: [8; 32],
                operations: operations.iter().copied().collect(),
            },
        )]),
        max_ttl_secs: 30,
    }
}

fn call(operation: Operation, body: [u8; 32]) -> HostCall {
    HostCall {
        version: VERSION,
        host: id(HOST),
        service: id(SERVICE),
        grant: GrantRef {
            id: GRANT,
            digest: [8; 32],
        },
        operation,
        body_digest: body,
        nonce: [9; 32],
        issued_at: 130,
        expires_at: 150,
    }
}

fn signed(c: HostCall) -> SignedHostCall {
    sign_call(&key(HOST), c).unwrap()
}

fn status_call() -> (SignedHostCall, [u8; 32]) {
    let body = body_digest(b"status of attempt 10");
    (signed(call(Operation::QueryStatus, body)), body)
}

#[test]
fn admits_an_authenticated_granted_call_exactly_once() {
    let (c, body) = status_call();
    let guard = Guard::default();
    let p = policy(&[Operation::QueryStatus]);
    let admitted = admit_call(&p, &c, body, NOW, &guard).unwrap();
    assert_eq!(admitted.operation(), Operation::QueryStatus);
    assert_eq!(admitted.grant_id(), GRANT);
    assert_eq!(admitted.call_digest(), c.digest());
    assert_eq!(
        admit_call(&p, &c, body, NOW, &guard).unwrap_err(),
        Error::Replay
    );
}

#[test]
fn call_signing_bytes_live_in_the_hawala_domain() {
    // ADR 0004 D3 / G9: biz-kit refuses to sign bytes with this prefix, so a
    // biz-kit ledger key can never produce a valid host call.
    assert!(call(Operation::QueryStatus, [1; 32])
        .signing_bytes()
        .starts_with(b"hawala"));
}

#[test]
fn rejects_unpinned_hosts_forged_signatures_and_wrong_audience() {
    let (_, body) = status_call();
    let p = policy(&[Operation::QueryStatus]);

    let mut stranger = call(Operation::QueryStatus, body);
    stranger.host = id(60);
    let stranger = sign_call(&key(60), stranger).unwrap();
    assert_eq!(
        admit_call(&p, &stranger, body, NOW, &Guard::default()).unwrap_err(),
        Error::UnknownHost
    );

    assert_eq!(
        sign_call(&key(60), call(Operation::QueryStatus, body)).unwrap_err(),
        Error::WrongSigner
    );

    let mut tampered = call(Operation::QueryStatus, body);
    tampered.nonce = [10; 32];
    let forged = SignedHostCall::from_parts(tampered, *status_call().0.signature());
    assert_eq!(
        admit_call(&p, &forged, body, NOW, &Guard::default()).unwrap_err(),
        Error::InvalidSignature
    );

    let mut elsewhere = call(Operation::QueryStatus, body);
    elsewhere.service = id(61);
    assert_eq!(
        admit_call(&p, &signed(elsewhere), body, NOW, &Guard::default()).unwrap_err(),
        Error::WrongService
    );
}

#[test]
fn rejects_unsupported_versions_and_out_of_window_calls() {
    let (_, body) = status_call();
    let p = policy(&[Operation::QueryStatus]);
    let admit =
        |c: HostCall, now| admit_call(&p, &signed(c), body, now, &Guard::default()).unwrap_err();

    let mut v2 = call(Operation::QueryStatus, body);
    v2.version = VERSION + 1;
    assert_eq!(admit(v2, NOW), Error::UnsupportedVersion);

    assert_eq!(
        admit(call(Operation::QueryStatus, body), 129),
        Error::NotYetValid
    );
    assert_eq!(
        admit(call(Operation::QueryStatus, body), 150),
        Error::Expired
    );

    let mut long = call(Operation::QueryStatus, body);
    long.expires_at = 130 + 31;
    assert_eq!(admit(long, NOW), Error::TtlTooLong);
}

#[test]
fn binds_each_call_to_a_pinned_grant_operation_and_body() {
    let (c, body) = status_call();
    let guard = Guard::default();

    let mut other_grant = call(Operation::QueryStatus, body);
    other_grant.grant.id = [1; 16];
    assert_eq!(
        admit_call(
            &policy(&[Operation::QueryStatus]),
            &signed(other_grant),
            body,
            NOW,
            &guard
        )
        .unwrap_err(),
        Error::UnknownGrant
    );

    let mut stale = call(Operation::QueryStatus, body);
    stale.grant.digest = [0xEE; 32];
    assert_eq!(
        admit_call(
            &policy(&[Operation::QueryStatus]),
            &signed(stale),
            body,
            NOW,
            &guard
        )
        .unwrap_err(),
        Error::GrantMismatch
    );

    assert_eq!(
        admit_call(&policy(&[Operation::ProposeTerms]), &c, body, NOW, &guard).unwrap_err(),
        Error::OperationNotGranted
    );

    assert_eq!(
        admit_call(
            &policy(&[Operation::QueryStatus]),
            &c,
            body_digest(b"other"),
            NOW,
            &guard
        )
        .unwrap_err(),
        Error::BodyMismatch
    );
}

#[test]
fn rejected_calls_do_not_consume_their_nonce() {
    let (c, body) = status_call();
    let guard = Guard::default();
    let p = policy(&[Operation::QueryStatus]);
    assert_eq!(
        admit_call(&p, &c, body_digest(b"other"), NOW, &guard).unwrap_err(),
        Error::BodyMismatch
    );
    assert!(admit_call(&p, &c, body, NOW, &guard).is_ok());
}

#[test]
fn rejects_malformed_policies() {
    let (c, body) = status_call();
    let mut p = policy(&[Operation::QueryStatus]);
    p.host_keys.remove(&id(HOST));
    assert_eq!(
        admit_call(&p, &c, body, NOW, &Guard::default()).unwrap_err(),
        Error::InvalidPolicy
    );

    let mut p = policy(&[Operation::QueryStatus]);
    p.max_ttl_secs = MAX_CALL_TTL_SECS + 1;
    assert_eq!(
        admit_call(&p, &c, body, NOW, &Guard::default()).unwrap_err(),
        Error::InvalidPolicy
    );

    let mut p = policy(&[Operation::QueryStatus]);
    p.host_keys.insert(id(SERVICE));
    assert_eq!(
        admit_call(&p, &c, body, NOW, &Guard::default()).unwrap_err(),
        Error::InvalidPolicy
    );
}

// --- G8: participant evidence is re-verified by the service and never host-signed ---

fn terms() -> SignedTerms {
    let t = Terms {
        transfer_id: [1; 32],
        sender: id(1),
        beneficiary: id(2),
        origin: id(3),
        disburser: id(4),
        funding_minor: 10_000,
        funding_currency: *b"USD",
        payout_minor: 9_000,
        payout_currency: *b"EUR",
        proposed_at: 100,
        accepted_at: 110,
        quote_expires_at: 120,
    };
    let a = sign_terms(&key(3), &t).unwrap();
    let b = sign_terms(&key(4), &t).unwrap();
    accept_terms(t, a, b).unwrap()
}

/// A cash-in authorization whose holder is a machine device `holder_device`
/// acting for the sender (1), verified by the origin dar (3).
fn authorization(holder_device: u8) -> (SignedTerms, AgreedInteraction, SignedAuthorization) {
    let accepted = terms();
    let holder = Participant::machine(id(holder_device), id(1)).unwrap();
    let verifier = Participant::human(id(3));
    let intent = InteractionIntent {
        initiator: holder,
        responder: verifier,
        action: Action::AcceptCash,
        hawala_terms_digest: accepted.terms().digest(),
        interaction_nonce: [6; 32],
        created_at: 125,
        expires_at: 200,
    };
    let a = sign_intent(&key(holder_device), &intent).unwrap();
    let b = sign_intent(&key(3), &intent).unwrap();
    let agreed = agree(intent, a, b, 130).unwrap();
    let request = AuthorizationRequest {
        holder: id(holder_device),
        verifier: id(3),
        transfer_id: accepted.terms().transfer_id,
        terms_digest: accepted.terms().digest(),
        interaction_nonce: [6; 32],
        action: Action::AcceptCash,
        amount_minor: 10_000,
        currency: *b"USD",
        disclosed_fee_minor: 7,
        attempt_id: [10; 32],
        challenge: [11; 32],
        qr_nonce: [12; 32],
        session_digest: [13; 32],
        issued_at: 130,
        expires_at: 180,
    };
    let signed = sign_authorization(&key(holder_device), request, &accepted, &agreed, NOW).unwrap();
    (accepted, agreed, signed)
}

fn expect<'a>(
    accepted: &'a SignedTerms,
    agreed: &'a AgreedInteraction,
) -> AuthorizationContext<'a> {
    AuthorizationContext {
        accepted,
        agreed,
        qr_nonce: [12; 32],
        challenge: [11; 32],
        session_digest: [13; 32],
        verifier: id(3),
    }
}

fn submit(auth: &SignedAuthorization) -> SignedHostCall {
    signed(call(Operation::SubmitAuthorization, auth.digest()))
}

#[test]
fn admits_device_signed_consent_bound_to_the_call() {
    let (accepted, agreed, auth) = authorization(12);
    let guard = Guard::default();
    let p = policy(&[Operation::SubmitAuthorization]);
    let (admitted, verified) = admit_authorization(
        &p,
        &submit(&auth),
        &auth,
        &expect(&accepted, &agreed),
        NOW,
        &guard,
    )
    .unwrap();
    assert_eq!(admitted.operation(), Operation::SubmitAuthorization);
    assert_eq!(verified.holder(), id(12));
    assert_eq!(verified.proof_digest(), auth.digest());
}

#[test]
fn refuses_consent_signed_by_any_host_key() {
    for host_key in [HOST, HOST_NODE] {
        let (accepted, agreed, auth) = authorization(host_key);
        let p = policy(&[Operation::SubmitAuthorization]);
        assert_eq!(
            admit_authorization(
                &p,
                &submit(&auth),
                &auth,
                &expect(&accepted, &agreed),
                NOW,
                &Guard::default()
            )
            .unwrap_err(),
            Error::HostSignedForParticipant
        );
    }
}

#[test]
fn service_reverifies_consent_instead_of_trusting_the_host() {
    let (accepted, agreed, auth) = authorization(12);
    let p = policy(&[Operation::SubmitAuthorization]);
    let mut wrong_session = expect(&accepted, &agreed);
    wrong_session.session_digest = [99; 32];
    assert_eq!(
        admit_authorization(
            &p,
            &submit(&auth),
            &auth,
            &wrong_session,
            NOW,
            &Guard::default()
        )
        .unwrap_err(),
        Error::Participant(hawala_qr_auth::Error::WrongSession)
    );
}

#[test]
fn consent_must_be_the_exact_object_the_host_signed_for() {
    let (accepted, agreed, auth) = authorization(12);
    let (_, _, other) = authorization(13);
    let p = policy(&[Operation::SubmitAuthorization]);
    let guard = Guard::default();
    assert_eq!(
        admit_authorization(
            &p,
            &submit(&other),
            &auth,
            &expect(&accepted, &agreed),
            NOW,
            &guard
        )
        .unwrap_err(),
        Error::BodyMismatch
    );
    let (status, _) = status_call();
    assert_eq!(
        admit_authorization(&p, &status, &auth, &expect(&accepted, &agreed), NOW, &guard)
            .unwrap_err(),
        Error::WrongOperation
    );
}

// --- G6: responses authenticate the service to the host ---

#[test]
fn host_verifies_service_responses_against_the_pinned_service_key() {
    let (c, _) = status_call();
    let response = ServiceResponse {
        call_digest: c.digest(),
        status: Status::Held,
        issued_at: NOW,
    };
    let signed = sign_response(&key(SERVICE), response.clone());
    assert_eq!(
        verify_response(id(SERVICE), &signed, c.digest()),
        Ok(response)
    );
    assert_eq!(
        verify_response(id(61), &signed, c.digest()),
        Err(Error::InvalidSignature)
    );
    assert_eq!(
        verify_response(id(SERVICE), &signed, [0; 32]),
        Err(Error::WrongCall)
    );
    assert!(signed.signing_bytes().starts_with(b"hawala"));
}
