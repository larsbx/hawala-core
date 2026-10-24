use ed25519_dalek::SigningKey;
use hawala_atm::{
    sign_observation, Capabilities, Error, ItemsTaken, ReportClass, SignedTerminalObservation,
    TerminalObservation,
};
use hawala_identity_core::IdentityId;

fn key(n: u8) -> SigningKey { SigningKey::from_bytes(&[n; 32]) }
fn id(n: u8) -> IdentityId { IdentityId::from_signing_key(&key(n)) }
fn machine(sensor: bool) -> Capabilities {
    Capabilities { dispense: true, deposit: false, recycle: false, items_taken_sensor: sensor }
}
fn specimen() -> TerminalObservation {
    TerminalObservation {
        terminal: id(7), transfer_id: [1; 32], terms_digest: [2; 32],
        attempt_id: [3; 32], observation_id: [4; 32],
        payout_minor: 20_000, currency: *b"USD",
        moved_to_output_minor: 20_000, customer_access_possible: true,
        items_taken: ItemsTaken::ObservedTaken, retracted_minor: 0,
        terminal_finished: true, terminal_log_digest: [5; 32], recorded_at: 100,
    }
}
fn signed(o: TerminalObservation) -> SignedTerminalObservation {
    sign_observation(&key(7), o).unwrap()
}

#[test]
fn signed_atm_report_is_scoped_to_exact_operator_terminal() {
    let s = signed(specimen());
    assert_eq!(s.verify(id(7)), Ok(()));
    assert_eq!(s.verify(id(8)), Err(Error::WrongTerminal));
    assert_eq!(s.classify(machine(true)), ReportClass::ReportedDelivered);
    let mut tampered = s.clone();
    tampered.observation.payout_minor = 100;
    assert_eq!(tampered.verify(id(7)), Err(Error::BadSignature));
    let mut tampered = s.clone();
    tampered.observation.items_taken = ItemsTaken::ObservedNotTaken;
    assert_eq!(tampered.verify(id(7)), Err(Error::BadSignature));
    assert_ne!(tampered.digest(), s.digest());
    assert_eq!(
        sign_observation(&key(8), specimen()).err(),
        Some(Error::WrongTerminal)
    );
}

#[test]
fn missing_sensor_partial_cash_and_accessible_retraction_are_unknown() {
    let s = signed(specimen());
    assert_eq!(s.classify(machine(false)), ReportClass::Uncertain);
    let mut o = specimen();
    o.items_taken = ItemsTaken::NotObserved;
    assert_eq!(signed(o).classify(machine(true)), ReportClass::Uncertain);
    let mut o = specimen();
    o.moved_to_output_minor = 10_000;
    assert_eq!(signed(o).classify(machine(true)), ReportClass::Uncertain);
    let mut o = specimen();
    o.retracted_minor = 10_000;
    assert_eq!(signed(o).classify(machine(true)), ReportClass::Uncertain);
    let mut o = specimen();
    o.terminal_finished = false;
    assert_eq!(signed(o).classify(machine(true)), ReportClass::Uncertain);
}

#[test]
fn no_delivery_requires_report_of_no_customer_access() {
    let mut o = specimen();
    o.moved_to_output_minor = 0;
    o.customer_access_possible = false;
    o.items_taken = ItemsTaken::NotObserved;
    assert_eq!(signed(o.clone()).classify(machine(false)), ReportClass::ReportedNotDelivered);
    o.customer_access_possible = true;
    assert_eq!(signed(o.clone()).classify(machine(false)), ReportClass::Uncertain);
    o.customer_access_possible = false;
    o.moved_to_output_minor = 50;
    assert_eq!(signed(o).classify(machine(false)), ReportClass::Uncertain);
}

#[test]
fn recycler_capabilities_and_malformed_observations_fail_safe() {
    let invalid = Capabilities {
        dispense: true, deposit: false, recycle: true, items_taken_sensor: true,
    };
    assert!(!invalid.valid());
    assert_eq!(signed(specimen()).classify(invalid), ReportClass::Uncertain);
    let recycler = Capabilities {
        dispense: true, deposit: true, recycle: true, items_taken_sensor: true,
    };
    assert!(recycler.valid());
    let mut o = specimen();
    o.attempt_id = [0; 32];
    assert_eq!(sign_observation(&key(7), o).err(), Some(Error::InvalidInput));
}
