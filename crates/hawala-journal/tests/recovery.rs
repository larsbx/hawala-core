use ed25519_dalek::SigningKey;
use hawala_identity_core::IdentityId;
use hawala_protocol::{
    accept_terms, attest_payout, sign_payout, sign_terms, ObligationEvidence,
    PayoutClaim, SignedTerms, Terms,
};
use hawala_journal::{Journal, JournalError, Phase};

fn key(n: u8) -> SigningKey { SigningKey::from_bytes(&[n; 32]) }
fn id(n: u8) -> IdentityId { IdentityId::from_signing_key(&key(n)) }
fn terms() -> Terms {
    Terms {
        transfer_id: [21; 32],
        sender: id(1), beneficiary: id(2), origin: id(3), disburser: id(4),
        funding_minor: 10_000, funding_currency: *b"USD",
        payout_minor: 9_000, payout_currency: *b"EUR",
        proposed_at: 100, accepted_at: 110, quote_expires_at: 120,
    }
}
fn accepted(t: Terms) -> SignedTerms {
    let o = sign_terms(&key(3), &t).unwrap();
    let d = sign_terms(&key(4), &t).unwrap();
    accept_terms(t, o, d).unwrap()
}

#[test]
fn payout_does_not_restart_after_crash_or_unknown_hardware_outcome() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("payment.sqlite");
    let accepted = accepted(terms());
    let transfer_id = accepted.terms().transfer_id;
    {
        let journal = Journal::open(&path).unwrap();
        assert_eq!(journal.register(&accepted).unwrap(), true);
        assert_eq!(journal.register(&accepted).unwrap(), false);
        assert_eq!(journal.status(transfer_id).unwrap(), Some(Phase::Registered));
        assert_eq!(journal.event_count(transfer_id).unwrap(), 1);
        journal.begin_attempt(transfer_id, [87; 32]).unwrap();
        assert_eq!(journal.status(transfer_id).unwrap(), Some(Phase::Dispensing));
        assert!(matches!(journal.begin_attempt(transfer_id, [88; 32]), Err(JournalError::InvalidTransition)));
        assert_eq!(journal.event_count(transfer_id).unwrap(), 2);
        // Imagine hardware dispenses but this process dies BEFORE receipt.
    }
    let journal = Journal::open(&path).unwrap();
    assert_eq!(journal.recovered_uncertain(), 1);
    assert_eq!(journal.status(transfer_id).unwrap(), Some(Phase::Uncertain));
    assert!(matches!(journal.begin_attempt(transfer_id, [99; 32]), Err(JournalError::InvalidTransition)));
    assert!(matches!(journal.cancel_unstarted(transfer_id), Err(JournalError::InvalidTransition)));
    assert_eq!(journal.event_count(transfer_id).unwrap(), 3);
    assert_eq!(journal.register(&accepted).unwrap(), false);
}

#[test]
fn duplicate_transfer_id_with_mutated_fx_cannot_replace_original_terms() {
    let d = tempfile::tempdir().unwrap();
    let journal = Journal::open(d.path().join("journal.sqlite")).unwrap();
    let first = accepted(terms());
    assert!(journal.register(&first).unwrap());
    let mut altered = terms();
    altered.payout_minor += 1;
    let signed_altered = accepted(altered);
    assert!(matches!(journal.register(&signed_altered), Err(JournalError::Conflict)));
    assert_eq!(journal.event_count(terms().transfer_id).unwrap(), 1);
}

#[test]
fn cancelled_intent_cannot_begin_dispensing_after_restart() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("journal.sqlite");
    let a = accepted(terms());
    let transfer = a.terms().transfer_id;
    {
        let journal = Journal::open(&path).unwrap();
        journal.register(&a).unwrap();
        journal.cancel_unstarted(transfer).unwrap();
        assert!(matches!(journal.begin_attempt(transfer, [7; 32]), Err(JournalError::InvalidTransition)));
    }
    let journal = Journal::open(&path).unwrap();
    assert_eq!(journal.recovered_uncertain(), 0);
    assert_eq!(journal.status(transfer).unwrap(), Some(Phase::Cancelled));
    assert_eq!(journal.event_count(transfer).unwrap(), 2);
}

#[test]
fn signed_payout_record_is_only_an_attestation_not_settlement() {
    let d = tempfile::tempdir().unwrap();
    let j = Journal::open(d.path().join("journal.sqlite")).unwrap();
    let a = accepted(terms());
    let t = a.terms();
    let mut evidence = ObligationEvidence::new(a.clone()).unwrap();
    let payout_claim = PayoutClaim {
        transfer_id: t.transfer_id, terms_digest: t.digest(),
        receipt_id: [12; 32], payout_minor: t.payout_minor,
        payout_currency: t.payout_currency, paid_at: 200,
    };
    let sig_a = sign_payout(&key(4), &a, &payout_claim).unwrap();
    let sig_b = sign_payout(&key(2), &a, &payout_claim).unwrap();
    let receipt = attest_payout(&a, payout_claim, sig_a, sig_b).unwrap();
    evidence.add_payout(receipt).unwrap();
    assert!(evidence.payout_attested());
    assert!(!evidence.settlement_attested());
    j.register(&a).unwrap();
    assert!(matches!(j.note_payout_attestation(&evidence), Err(JournalError::InvalidTransition)));
    j.begin_attempt(t.transfer_id, [7; 32]).unwrap();
    j.note_payout_attestation(&evidence).unwrap();
    assert_eq!(j.status(t.transfer_id).unwrap(), Some(Phase::PayoutAttested));
    assert!(matches!(j.begin_attempt(t.transfer_id, [8; 32]), Err(JournalError::InvalidTransition)));
}

#[test]
fn independent_writer_cannot_steal_single_active_authority() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("journal.sqlite");
    let first = Journal::open(&path).unwrap();
    assert!(matches!(Journal::open(&path), Err(JournalError::Io(_))));
    drop(first);
    assert!(Journal::open(&path).is_ok());
}
