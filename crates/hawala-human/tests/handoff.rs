use ed25519_dalek::SigningKey;
use hawala_human::{assemble_handoff, sign_handoff, Error, HandoffClaim};
use hawala_identity_core::IdentityId;
use hawala_protocol::{accept_terms, sign_terms, SignedTerms, Terms};
fn key(n:u8)->SigningKey{SigningKey::from_bytes(&[n;32])}
fn id(n:u8)->IdentityId{IdentityId::from_signing_key(&key(n))}
fn terms()->SignedTerms{
 let t=Terms{transfer_id:[11;32],sender:id(1),beneficiary:id(2),origin:id(3),disburser:id(4),
 funding_minor:12000,funding_currency:*b"USD",payout_minor:10000,payout_currency:*b"EUR",
 proposed_at:100,accepted_at:110,quote_expires_at:120};
 let a=sign_terms(&key(3),&t).unwrap();let b=sign_terms(&key(4),&t).unwrap();
 accept_terms(t,a,b).unwrap()
}
fn claim(a:&SignedTerms)->HandoffClaim{
 HandoffClaim{transfer_id:a.terms().transfer_id,terms_digest:a.terms().digest(),
 attempt_id:[21;32],receipt_id:[22;32],beneficiary:id(2),disburser:id(4),
 payout_minor:a.terms().payout_minor,currency:a.terms().payout_currency,asserted_handoff_at:180}
}
#[test] fn exact_dual_key_evidence_required(){
 let a=terms();let c=claim(&a);
 let b=sign_handoff(&key(2),&a,&c).unwrap();let d=sign_handoff(&key(4),&a,&c).unwrap();
 let receipt=assemble_handoff(&a,c.clone(),b,d).unwrap();
 assert_eq!(receipt.verify(&a),Ok(()));
 assert_ne!(receipt.digest(),[0;32]);
 assert_eq!(sign_handoff(&key(3),&a,&c),Err(Error::WrongSigner));
 assert_eq!(assemble_handoff(&a,c.clone(),d,b),Err(Error::InvalidSignature));
 let mut altered=c.clone();altered.attempt_id=[99;32];
 assert_eq!(assemble_handoff(&a,altered,b,d),Err(Error::InvalidSignature));
 let mut altered=c.clone();altered.receipt_id=[88;32];
 assert_eq!(assemble_handoff(&a,altered,b,d),Err(Error::InvalidSignature));
 let mut altered=c.clone();altered.currency=*b"USD";
 assert_eq!(assemble_handoff(&a,altered,b,d),Err(Error::WrongTerms));
 let mut altered=c;altered.payout_minor+=1;
 assert_eq!(assemble_handoff(&a,altered,b,d),Err(Error::WrongTerms));
}
