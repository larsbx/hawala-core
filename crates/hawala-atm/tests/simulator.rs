use ed25519_dalek::SigningKey;
use hawala_atm::{
    simulator::{simulate, MockRequest, NormalizedEvent as Event, SimulationError},
    Capabilities, ReportClass,
};
use hawala_identity_core::IdentityId;

fn key(n: u8) -> SigningKey { SigningKey::from_bytes(&[n; 32]) }
fn id(n: u8) -> IdentityId { IdentityId::from_signing_key(&key(n)) }
fn caps(sensor: bool) -> Capabilities {
    Capabilities { dispense: true, deposit: false, recycle: false, items_taken_sensor: sensor }
}
fn request() -> MockRequest {
    MockRequest {
        terminal: id(7), transfer_id: [1; 32], terms_digest: [2; 32],
        attempt_id: [3; 32], payout_minor: 12_500, currency: *b"USD",
        recorded_at: 100,
    }
}

#[test]
fn complete_taken_amount_is_reported_only_with_sensor() {
    let events = [
        Event::TransportedMinor(12_500),
        Event::CustomerAccessOpened, Event::ItemsTakenSensorFired, Event::Finished,
    ];
    let signed = simulate(&key(7), request(), caps(true), &events).unwrap();
    assert_eq!(signed.verify(id(7)), Ok(()));
    assert_eq!(signed.classify(caps(true)), ReportClass::ReportedDelivered);
    assert_eq!(signed.classify(caps(false)), ReportClass::Uncertain);
    let without_sensor = simulate(&key(7), request(), caps(false), &events).unwrap();
    assert_eq!(without_sensor.classify(caps(false)), ReportClass::Uncertain);
}

#[test]
fn jam_partial_and_retracted_access_require_manual_reconciliation() {
    let partial = [Event::TransportedMinor(5_000), Event::CustomerAccessOpened,
                   Event::ItemsTakenSensorFired, Event::Finished];
    assert_eq!(
        simulate(&key(7), request(), caps(true), &partial).unwrap().classify(caps(true)),
        ReportClass::Uncertain,
    );
    let jam = [Event::TransportedMinor(12_500), Event::Indeterminate];
    assert_eq!(
        simulate(&key(7), request(), caps(true), &jam).unwrap().classify(caps(true)),
        ReportClass::Uncertain,
    );
    let retract = [Event::TransportedMinor(12_500), Event::CustomerAccessOpened,
                   Event::RetractedMinor(12_500), Event::Finished];
    assert_eq!(
        simulate(&key(7), request(), caps(true), &retract).unwrap().classify(caps(true)),
        ReportClass::Uncertain,
    );
}

#[test]
fn finished_no_movement_can_only_report_no_delivery() {
    let signed = simulate(&key(7), request(), caps(false), &[Event::Finished]).unwrap();
    assert_eq!(signed.classify(caps(false)), ReportClass::ReportedNotDelivered);
    assert_eq!(signed.verify(id(7)), Ok(()));
}

#[test]
fn invalid_event_logs_and_wrong_terminal_are_rejected() {
    assert_eq!(
        simulate(&key(8), request(), caps(true), &[Event::Finished]).err(),
        Some(SimulationError::WrongSigner),
    );
    assert_eq!(
        simulate(&key(7), request(), caps(true), &[]).err(),
        Some(SimulationError::MalformedEventSequence),
    );
    assert_eq!(
        simulate(&key(7), request(), caps(true),
            &[Event::Finished, Event::CustomerAccessOpened]).err(),
        Some(SimulationError::MalformedEventSequence),
    );
    assert_eq!(
        simulate(&key(7), request(), caps(true),
            &[Event::TransportedMinor(0)]).err(),
        Some(SimulationError::MalformedEventSequence),
    );
    let overflow = [
        Event::TransportedMinor(u64::MAX),
        Event::TransportedMinor(1),
        Event::Finished,
    ];
    assert_eq!(
        simulate(&key(7), request(), caps(true), &overflow).err(),
        Some(SimulationError::ArithmeticOverflow),
    );
}

#[test]
fn event_order_and_attempt_context_are_bound_into_signed_report() {
    let initial = [Event::TransportedMinor(12_500), Event::CustomerAccessOpened,
                   Event::ItemsTakenSensorFired, Event::Finished];
    let altered = [Event::CustomerAccessOpened, Event::TransportedMinor(12_500),
                   Event::ItemsTakenSensorFired, Event::Finished];
    let s1 = simulate(&key(7), request(), caps(true), &initial).unwrap();
    let s2 = simulate(&key(7), request(), caps(true), &altered).unwrap();
    assert_ne!(s1.digest(), s2.digest());
    let mut another_attempt = request();
    another_attempt.attempt_id = [9; 32];
    let s3 = simulate(&key(7), another_attempt, caps(true), &initial).unwrap();
    assert_ne!(s1.digest(), s3.digest());
}
