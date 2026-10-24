use ed25519_dalek::SigningKey;
use hawala_atm::{
    sign_observation, Capabilities, ItemsTaken, ReportClass, TerminalObservation,
};
use hawala_identity_core::IdentityId;
use hawala_journal::{Journal, JournalError, Phase};
use hawala_protocol::{accept_terms, sign_terms, SignedTerms, Terms};

fn key(n: u8) -> SigningKey { SigningKey::from_bytes(&[n; 32]) }
fn id(n: u8) -> IdentityId { IdentityId::from_signing_key(&key(n)) }
fn accepted() -> SignedTerms {
    let terms = Terms {
        transfer_id: [21; 32], sender: id(1), beneficiary: id(2),
        origin: id(3), disburser: id(4),
        funding_minor: 20_000, funding_currency: *b"USD",
        payout_minor: 18_000, payout_currency: *b"EUR",
        proposed_at: 100, accepted_at: 110, quote_expires_at: 120,
    };
    let origin = sign_terms(&key(3), &terms).unwrap();
    let disburser = sign_terms(&key(4), &terms).unwrap();
    accept_terms(terms, origin, disburser).unwrap()
}
fn capabilities() -> Capabilities {
    Capabilities { dispense: true, deposit: false, recycle: false, items_taken_sensor: true }
}
fn observation(a: &SignedTerms) -> TerminalObservation {
    TerminalObservation {
        terminal: id(7), transfer_id: a.terms().transfer_id,
        terms_digest: a.terms().digest(), attempt_id: [11; 32],
        observation_id: [33; 32], payout_minor: a.terms().payout_minor,
        currency: a.terms().payout_currency,
        moved_to_output_minor: a.terms().payout_minor,
        customer_access_possible: true, items_taken: ItemsTaken::ObservedTaken,
        retracted_minor: 0, terminal_finished: true,
        terminal_log_digest: [88; 32], recorded_at: 170,
    }
}

#[test]
fn atm_report_keeps_journal_frozen_even_when_operator_says_cash_was_taken() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.sqlite");
    let a = accepted();
    {
        let journal = Journal::open(&path).unwrap();
        journal.register(&a).unwrap();
        journal.begin_atm_attempt(a.terms().transfer_id, [11; 32], id(7)).unwrap();
        let report = sign_observation(&key(7), observation(&a)).unwrap();
        assert!(journal.record_terminal_report(&report, capabilities()).unwrap());
        assert!(!journal.record_terminal_report(&report, capabilities()).unwrap());
        assert_eq!(
            journal.terminal_report(a.terms().transfer_id).unwrap(),
            Some(ReportClass::ReportedDelivered)
        );
        assert_eq!(journal.status(a.terms().transfer_id).unwrap(), Some(Phase::Uncertain));
        assert!(matches!(
            journal.begin_atm_attempt(a.terms().transfer_id, [12; 32], id(7)),
            Err(JournalError::InvalidTransition)
        ));
        assert_eq!(journal.event_count(a.terms().transfer_id).unwrap(), 3);
    }
    let journal = Journal::open(&path).unwrap();
    assert_eq!(journal.recovered_uncertain(), 0);
    assert_eq!(journal.status(a.terms().transfer_id).unwrap(), Some(Phase::Uncertain));
    assert_eq!(
        journal.terminal_report(a.terms().transfer_id).unwrap(),
        Some(ReportClass::ReportedDelivered)
    );
}

#[test]
fn wrong_terminal_attempt_currency_and_signed_data_cannot_replace_report() {
    let dir = tempfile::tempdir().unwrap();
    let journal = Journal::open(dir.path().join("journal.sqlite")).unwrap();
    let a = accepted();
    journal.register(&a).unwrap();
    journal.begin_atm_attempt(a.terms().transfer_id, [11; 32], id(7)).unwrap();
    let o = observation(&a);
    let mut wrong = o.clone(); wrong.terminal = id(8);
    let report = sign_observation(&key(8), wrong).unwrap();
    assert!(matches!(
        journal.record_terminal_report(&report, capabilities()),
        Err(JournalError::InvalidTerminalEvidence)
    ));
    let mut wrong = o.clone(); wrong.attempt_id = [12; 32];
    let report = sign_observation(&key(7), wrong).unwrap();
    assert!(matches!(
        journal.record_terminal_report(&report, capabilities()),
        Err(JournalError::InvalidTerminalEvidence)
    ));
    let mut wrong = o.clone(); wrong.currency = *b"USD";
    let report = sign_observation(&key(7), wrong).unwrap();
    assert!(matches!(
        journal.record_terminal_report(&report, capabilities()),
        Err(JournalError::InvalidTerminalEvidence)
    ));
    let mut report = sign_observation(&key(7), o.clone()).unwrap();
    report.observation.moved_to_output_minor = 0;
    assert!(matches!(
        journal.record_terminal_report(&report, capabilities()),
        Err(JournalError::InvalidTerminalEvidence)
    ));
    let first = sign_observation(&key(7), o).unwrap();
    assert!(journal.record_terminal_report(&first, capabilities()).unwrap());
    let mut conflicting = first.observation.clone();
    conflicting.observation_id = [44; 32];
    let second = sign_observation(&key(7), conflicting).unwrap();
    assert!(matches!(
        journal.record_terminal_report(&second, capabilities()),
        Err(JournalError::Conflict)
    ));
    assert_eq!(journal.event_count(a.terms().transfer_id).unwrap(), 3);
}

#[test]
fn only_bound_atm_attempt_can_accept_terminal_reports() {
    let dir = tempfile::tempdir().unwrap();
    let journal = Journal::open(dir.path().join("journal.sqlite")).unwrap();
    let a = accepted();
    journal.register(&a).unwrap();
    journal.begin_attempt(a.terms().transfer_id, [11; 32]).unwrap();
    let report = sign_observation(&key(7), observation(&a)).unwrap();
    assert!(matches!(
        journal.record_terminal_report(&report, capabilities()),
        Err(JournalError::UntrustedTerminal)
    ));
}

#[test]
fn report_claiming_no_cash_delivered_does_not_allow_cash_retry() {
    let dir = tempfile::tempdir().unwrap();
    let journal = Journal::open(dir.path().join("journal.sqlite")).unwrap();
    let a = accepted();
    journal.register(&a).unwrap();
    journal.begin_atm_attempt(a.terms().transfer_id, [11; 32], id(7)).unwrap();
    let mut o = observation(&a);
    o.moved_to_output_minor = 0;
    o.customer_access_possible = false;
    o.items_taken = ItemsTaken::NotObserved;
    let report = sign_observation(&key(7), o).unwrap();
    journal.record_terminal_report(&report, capabilities()).unwrap();
    assert_eq!(journal.terminal_report(a.terms().transfer_id).unwrap(),
        Some(ReportClass::ReportedNotDelivered));
    assert_eq!(journal.status(a.terms().transfer_id).unwrap(), Some(Phase::Uncertain));
    assert!(matches!(
        journal.begin_atm_attempt(a.terms().transfer_id, [22; 32], id(7)),
        Err(JournalError::InvalidTransition)
    ));
}
