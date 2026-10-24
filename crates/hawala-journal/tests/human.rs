use ed25519_dalek::SigningKey;
use hawala_human::{assemble_handoff,sign_handoff,HandoffClaim};
use hawala_identity_core::IdentityId;
use hawala_journal::{Journal,JournalError,Phase};
use hawala_protocol::{accept_terms,sign_terms,SignedTerms,Terms};
fn key(n:u8)->SigningKey{SigningKey::from_bytes(&[n;32])}
fn id(n:u8)->IdentityId{IdentityId::from_signing_key(&key(n))}
fn accepted()->SignedTerms{
 let t=Terms{transfer_id:[11;32],sender:id(1),beneficiary:id(2),origin:id(3),disburser:id(4),
 funding_minor:12000,funding_currency:*b"USD",payout_minor:10000,payout_currency:*b"EUR",
 proposed_at:100,accepted_at:110,quote_expires_at:120};
 let o=sign_terms(&key(3),&t).unwrap();let d=sign_terms(&key(4),&t).unwrap();
 accept_terms(t,o,d).unwrap()
}
fn handoff(a:&SignedTerms,attempt:u8,receipt:u8)->hawala_human::SignedHandoff{
 let c=HandoffClaim{transfer_id:a.terms().transfer_id,terms_digest:a.terms().digest(),
 attempt_id:[attempt;32],receipt_id:[receipt;32],beneficiary:id(2),disburser:id(4),
 payout_minor:a.terms().payout_minor,currency:a.terms().payout_currency,asserted_handoff_at:200};
 let b=sign_handoff(&key(2),a,&c).unwrap();let d=sign_handoff(&key(4),a,&c).unwrap();
 assemble_handoff(a,c,b,d).unwrap()
}
#[test] fn human_handoff_persisted_but_never_unlocks_cash_retry(){
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("j.sqlite");
 let a=accepted();let id=a.terms().transfer_id;
 {
  let j=Journal::open(&path).unwrap();j.register(&a).unwrap();
  j.begin_attempt(id,[7;32]).unwrap();
  assert!(j.record_human_handoff(&a,&handoff(&a,7,8)).unwrap());
  assert!(!j.record_human_handoff(&a,&handoff(&a,7,8)).unwrap());
  assert_eq!(j.status(id).unwrap(),Some(Phase::Uncertain));
  assert!(j.human_handoff_recorded(id).unwrap());
  assert!(matches!(j.begin_attempt(id,[8;32]),Err(JournalError::InvalidTransition)));
 }
 let j=Journal::open(&path).unwrap();
 assert_eq!(j.status(id).unwrap(),Some(Phase::Uncertain));
 assert!(matches!(j.record_human_handoff(&a,&handoff(&a,7,9)),Err(JournalError::Conflict)));
 assert!(matches!(j.record_human_handoff(&a,&handoff(&a,9,8)),Err(JournalError::InvalidHandoffEvidence)));
 assert_eq!(j.event_count(id).unwrap(),3);
}
#[test] fn machine_bound_attempt_cannot_use_human_receipt(){
 let d=tempfile::tempdir().unwrap();let j=Journal::open(d.path().join("j.db")).unwrap();
 let a=accepted();let id=a.terms().transfer_id;
 j.register(&a).unwrap();j.begin_atm_attempt(id,[7;32],id_fn()).unwrap();
 assert!(matches!(j.record_human_handoff(&a,&handoff(&a,7,8)),Err(JournalError::InvalidHandoffEvidence)));
 assert_eq!(j.status(id).unwrap(),Some(Phase::Dispensing));
}
fn id_fn()->IdentityId{id(9)}
#[test] fn crash_before_receipt_remains_uncertain(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("j.db");let a=accepted();
 let id=a.terms().transfer_id;
 {let j=Journal::open(&path).unwrap();j.register(&a).unwrap();j.begin_attempt(id,[7;32]).unwrap();}
 let j=Journal::open(&path).unwrap();
 assert_eq!(j.recovered_uncertain(),1);
 assert!(j.record_human_handoff(&a,&handoff(&a,7,8)).unwrap());
 assert_eq!(j.status(id).unwrap(),Some(Phase::Uncertain));
 assert!(matches!(j.begin_attempt(id,[9;32]),Err(JournalError::InvalidTransition)));
}
