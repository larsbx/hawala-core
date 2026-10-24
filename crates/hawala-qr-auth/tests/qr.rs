use std::collections::HashSet;
use std::sync::Mutex;
use ed25519_dalek::SigningKey;
use hawala_identity_core::{IdentityId, ReplayGuard};
use hawala_interaction::{agree, sign_intent, Action, AgreedInteraction, InteractionIntent, Participant};
use hawala_protocol::{accept_terms, sign_terms, SignedTerms, Terms};
use hawala_qr_auth::{sign_authorization, verify_and_consume, AuthorizationRequest, Error};

fn key(n:u8)->SigningKey { SigningKey::from_bytes(&[n;32]) }
fn id(n:u8)->IdentityId { IdentityId::from_signing_key(&key(n)) }
fn terms()->SignedTerms {
 let t=Terms {transfer_id:[1;32],sender:id(1),beneficiary:id(2),
 origin:id(3),disburser:id(4),funding_minor:10_000,funding_currency:*b"USD",
 payout_minor:9_000,payout_currency:*b"EUR",proposed_at:100,accepted_at:110,
 quote_expires_at:120};
 let a=sign_terms(&key(3),&t).unwrap();let b=sign_terms(&key(4),&t).unwrap();
 accept_terms(t,a,b).unwrap()
}
#[derive(Default)]
struct Guard(Mutex<HashSet<(IdentityId,IdentityId,[u8;32])>>);
impl ReplayGuard for Guard {
 fn consume_once(&self,v:IdentityId,c:IdentityId,n:[u8;32])->hawala_identity_core::Result<bool>{
  Ok(self.0.lock().unwrap().insert((v,c,n)))
 }
}
fn get(action:Action,human_holder:bool,human_verifier:bool,reverse:bool)
->(SignedTerms,AgreedInteraction,SigningKey,AuthorizationRequest) {
 let accepted=terms();
 let (holder_principal,verifier_principal,amount,currency)=match action {
  Action::AcceptCash=>(1,3,10_000,*b"USD"),
  Action::DistributeCash=>(2,4,9_000,*b"EUR"),
  _=>panic!("cash only"),
 };
 let hs=if human_holder {holder_principal} else {12};
 let vs=if human_verifier {verifier_principal} else {14};
 let holder=if human_holder{Participant::human(id(holder_principal))}
  else{Participant::machine(id(hs),id(holder_principal)).unwrap()};
 let verifier=if human_verifier{Participant::human(id(verifier_principal))}
  else{Participant::machine(id(vs),id(verifier_principal)).unwrap()};
 let (initiator,responder,ik,rk)=if reverse {
  (verifier,holder,key(vs),key(hs))
 } else {(holder,verifier,key(hs),key(vs))};
 let intent=InteractionIntent {
  initiator,responder,action,hawala_terms_digest:accepted.terms().digest(),
  interaction_nonce:[6;32],created_at:125,expires_at:200
 };
 let a=sign_intent(&ik,&intent).unwrap();
 let b=sign_intent(&rk,&intent).unwrap();
 let agreed=agree(intent,a,b,130).unwrap();
 let r=AuthorizationRequest{
  holder:id(hs),verifier:id(vs),
  transfer_id:accepted.terms().transfer_id,terms_digest:accepted.terms().digest(),
  interaction_nonce:[6;32],action,amount_minor:amount,currency,
  disclosed_fee_minor:7,attempt_id:[10;32],challenge:[11;32],
  qr_nonce:[12;32],session_digest:[13;32],issued_at:130,expires_at:180,
 };
 (accepted,agreed,key(hs),r)
}
#[test]
fn temporary_qr_signatures_work_for_all_cashed_peer_orientations(){
 for action in [Action::AcceptCash,Action::DistributeCash] {
  for (h,v) in [(true,true),(true,false),(false,true),(false,false)] {
   for reverse in [false,true] {
    let (terms,agreement,signer,request)=get(action,h,v,reverse);
    let qr=request.qr_payload();
    assert_eq!(AuthorizationRequest::parse_qr(&qr).unwrap(),request.qr_nonce);
    assert!(!qr.contains("USD") && !qr.contains("EUR"));
    let signed=sign_authorization(&signer,request.clone(),&terms,&agreement,135).unwrap();
    let guard=Guard::default();
    let verify=||verify_and_consume(
     &signed,&terms,&agreement,request.qr_nonce,request.challenge,
     request.session_digest,request.verifier,135,&guard
    );
    assert_eq!(verify(),Ok(()));
    assert_eq!(verify(),Err(Error::Replay));
   }
  }
 }
}
#[test]
fn rejects_wrong_audience_challenge_session_qr_clock_and_amount(){
 let (terms,agreement,signer,request)=get(Action::DistributeCash,true,false,false);
 let signed=sign_authorization(&signer,request.clone(),&terms,&agreement,135).unwrap();
 let g=Guard::default();
 let check=|qr,challenge,session,verifier,now|
  verify_and_consume(&signed,&terms,&agreement,qr,challenge,session,verifier,now,&g);
 assert_eq!(check([88;32],request.challenge,request.session_digest,request.verifier,135),Err(Error::WrongQr));
 assert_eq!(check(request.qr_nonce,[88;32],request.session_digest,request.verifier,135),Err(Error::WrongChallenge));
 assert_eq!(check(request.qr_nonce,request.challenge,[88;32],request.verifier,135),Err(Error::WrongSession));
 assert_eq!(check(request.qr_nonce,request.challenge,request.session_digest,id(3),135),Err(Error::WrongAudience));
 assert_eq!(check(request.qr_nonce,request.challenge,request.session_digest,request.verifier,181),Err(Error::Expired));
 let mut wrong=request.clone();wrong.currency=*b"USD";
 assert_eq!(sign_authorization(&signer,wrong,&terms,&agreement,135).err(),Some(Error::WrongSignedFlow));
 let mut wrong=request.clone();wrong.amount_minor+=1;
 assert_eq!(sign_authorization(&signer,wrong,&terms,&agreement,135).err(),Some(Error::WrongSignedFlow));
 let mut wrong=request.clone();wrong.challenge=wrong.qr_nonce;
 assert_eq!(sign_authorization(&signer,wrong,&terms,&agreement,135).err(),Some(Error::InvalidRequest));
 let mut wrong=request.clone();wrong.expires_at=252;
 assert_eq!(sign_authorization(&signer,wrong,&terms,&agreement,135).err(),Some(Error::InvalidRequest));
 assert_eq!(sign_authorization(&key(5),request,&terms,&agreement,135).err(),Some(Error::WrongSigner));
}
#[test]
fn static_identity_qr_and_malformed_auth_reference_are_not_valid_payment_qr(){
 let id_qr=id(1).qr_payload();
 assert_eq!(AuthorizationRequest::parse_qr(&id_qr),Err(Error::WrongQr));
 assert_eq!(AuthorizationRequest::parse_qr("hawala:auth:v1:00"),Err(Error::WrongQr));
 assert_eq!(AuthorizationRequest::parse_qr(&format!("hawala:auth:v1:{}", "00".repeat(32))),Err(Error::WrongQr));
}
