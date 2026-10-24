use ed25519_dalek::SigningKey;
use hawala_atm::{sign_observation, Capabilities, ItemsTaken, TerminalObservation};
use hawala_identity_core::IdentityId;
use hawala_interaction::{agree, sign_intent, Action, AgreedInteraction, InteractionIntent, Participant};
use hawala_journal::{Journal, JournalError, Phase};
use hawala_protocol::{accept_terms, sign_terms, SignedTerms, Terms};
use hawala_qr_auth::{sign_authorization, AuthorizationRequest, SignedAuthorization};

fn key(n:u8)->SigningKey { SigningKey::from_bytes(&[n;32]) }
fn id(n:u8)->IdentityId { IdentityId::from_signing_key(&key(n)) }
fn accepted()->SignedTerms {
 let t=Terms{transfer_id:[1;32],sender:id(1),beneficiary:id(2),origin:id(3),disburser:id(4),
 funding_minor:10_000,funding_currency:*b"USD",payout_minor:9_000,payout_currency:*b"EUR",
 proposed_at:100,accepted_at:110,quote_expires_at:120};
 let a=sign_terms(&key(3),&t).unwrap();let b=sign_terms(&key(4),&t).unwrap();
 accept_terms(t,a,b).unwrap()
}
fn evidence(machine:bool,nonce:u8,attempt:u8)->(SignedTerms,AgreedInteraction,SignedAuthorization,IdentityId) {
 let a=accepted();let v=if machine {9} else {4};
 let holder=Participant::human(id(2));
 let cash_peer=if machine {Participant::machine(id(9),id(4)).unwrap()}
   else {Participant::human(id(4))};
 let intent=InteractionIntent{
  initiator:holder,responder:cash_peer,action:Action::DistributeCash,
  hawala_terms_digest:a.terms().digest(),interaction_nonce:[6;32],
  created_at:125,expires_at:200,
 };
 let s1=sign_intent(&key(2),&intent).unwrap();
 let s2=sign_intent(&key(v),&intent).unwrap();
 let agreed=agree(intent,s1,s2,130).unwrap();
 let r=AuthorizationRequest {
  holder:id(2),verifier:id(v),transfer_id:a.terms().transfer_id,
  terms_digest:a.terms().digest(),interaction_nonce:[6;32],
  action:Action::DistributeCash,amount_minor:9_000,currency:*b"EUR",
  disclosed_fee_minor:7,attempt_id:[attempt;32],
  challenge:[13;32],qr_nonce:[nonce;32],session_digest:[15;32],
  issued_at:130,expires_at:180,
 };
 let signed=sign_authorization(&key(2),r,&a,&agreed,135).unwrap();
 (a,agreed,signed,id(v))
}
fn attempt(j:&Journal,a:&SignedTerms,i:&AgreedInteraction,s:&SignedAuthorization,v:IdentityId)
 -> hawala_journal::Result<()> {
 j.begin_qr_cashout_intent(a,i,s,[s.request().qr_nonce[0];32],[13;32],[15;32],v,135)
}

#[test]
fn local_once_only_cash_intent_and_qr_nonce_are_atomic_across_restart(){
 let d=tempfile::tempdir().unwrap();let path=d.path().join("journal.db");
 let (a,i,s,v)=evidence(false,20,21);
 {
  let j=Journal::open(&path).unwrap();
  j.register(&a).unwrap();
  assert!(!j.qr_authorization_recorded(a.terms().transfer_id).unwrap());
  assert!(attempt(&j,&a,&i,&s,v).is_ok());
  assert!(j.qr_authorization_recorded(a.terms().transfer_id).unwrap());
  assert_eq!(j.status(a.terms().transfer_id).unwrap(),Some(Phase::Dispensing));
  assert!(matches!(attempt(&j,&a,&i,&s,v),Err(JournalError::AuthorizationReplay)));
  assert_eq!(j.event_count(a.terms().transfer_id).unwrap(),2);
 }
 let j=Journal::open(&path).unwrap();
 assert_eq!(j.recovered_uncertain(),1);
 assert_eq!(j.status(a.terms().transfer_id).unwrap(),Some(Phase::Uncertain));
 assert!(matches!(attempt(&j,&a,&i,&s,v),Err(JournalError::AuthorizationReplay)));
 assert!(matches!(j.begin_attempt(a.terms().transfer_id,[80;32]),Err(JournalError::InvalidTransition)));
}

#[test]
fn wrong_expected_qr_session_and_verifier_do_not_enter_journal(){
 let d=tempfile::tempdir().unwrap();
 let j=Journal::open(d.path().join("journal.db")).unwrap();
 let (a,i,s,v)=evidence(false,20,21);
 j.register(&a).unwrap();
 assert!(matches!(
  j.begin_qr_cashout_intent(&a,&i,&s,[22;32],[13;32],[15;32],v,135),
  Err(JournalError::InvalidQrAuthorization)
 ));
 assert!(matches!(
  j.begin_qr_cashout_intent(&a,&i,&s,[20;32],[13;32],[18;32],v,135),
  Err(JournalError::InvalidQrAuthorization)
 ));
 assert!(matches!(
  j.begin_qr_cashout_intent(&a,&i,&s,[20;32],[13;32],[15;32],id(5),135),
  Err(JournalError::InvalidQrAuthorization)
 ));
 assert!(!j.qr_authorization_recorded(a.terms().transfer_id).unwrap());
 assert_eq!(j.status(a.terms().transfer_id).unwrap(),Some(Phase::Registered));
 assert!(attempt(&j,&a,&i,&s,v).is_ok());
}

#[test]
fn wrong_second_intent_fails_without_authorizing_another_attempt(){
 let d=tempfile::tempdir().unwrap();
 let j=Journal::open(d.path().join("journal.db")).unwrap();
 let (a,i,s,v)=evidence(false,20,21);
 j.register(&a).unwrap();
 assert!(attempt(&j,&a,&i,&s,v).is_ok());
 let (b,k,t,w)=evidence(false,22,23);
 assert!(matches!(attempt(&j,&b,&k,&t,w),Err(JournalError::Conflict)));
 assert_eq!(j.event_count(a.terms().transfer_id).unwrap(),2);
 assert_eq!(j.status(a.terms().transfer_id).unwrap(),Some(Phase::Dispensing));
}

#[test]
fn machine_verifier_is_pinned_to_atm_and_cannot_accept_a_human_receipt(){
 let d=tempfile::tempdir().unwrap();
 let j=Journal::open(d.path().join("journal.db")).unwrap();
 let (a,i,s,v)=evidence(true,20,21);
 j.register(&a).unwrap();
 assert!(attempt(&j,&a,&i,&s,v).is_ok());
 let ob=TerminalObservation{
  terminal:id(9),transfer_id:a.terms().transfer_id,
  terms_digest:a.terms().digest(),attempt_id:[21;32],observation_id:[24;32],
  payout_minor:9_000,currency:*b"EUR",moved_to_output_minor:9_000,
  customer_access_possible:true,items_taken:ItemsTaken::ObservedTaken,
  retracted_minor:0,terminal_finished:true,terminal_log_digest:[25;32],recorded_at:200,
 };
 let signed=sign_observation(&key(9),ob).unwrap();
 let caps=Capabilities{dispense:true,deposit:false,recycle:false,items_taken_sensor:true};
 assert!(j.record_terminal_report(&signed,caps).unwrap());
 assert_eq!(j.status(a.terms().transfer_id).unwrap(),Some(Phase::Uncertain));
 assert!(matches!(attempt(&j,&a,&i,&s,v),Err(JournalError::AuthorizationReplay)));
}
