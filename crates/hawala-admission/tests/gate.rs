use ed25519_dalek::SigningKey;
use hawala_admission::{
    evaluate, sign_assertion, Authorities, Context, Error, EvidenceBundle,
    Kind, SignedAssertion,
};
use hawala_identity_core::IdentityId;
use hawala_interaction::{
    agree, sign_intent, Action, AgreedInteraction, InteractionIntent, Participant,
};
use hawala_protocol::{accept_terms, sign_terms, SignedTerms, Terms};
use hawala_qr_auth::{sign_authorization, AuthorizationRequest, SignedAuthorization};

fn key(seed: u8) -> SigningKey { SigningKey::from_bytes(&[seed; 32]) }
fn id(seed: u8) -> IdentityId { IdentityId::from_signing_key(&key(seed)) }
fn accepted() -> SignedTerms {
    let t = Terms {
        transfer_id: [10;32], sender: id(1), beneficiary: id(2),
        origin: id(3), disburser: id(4),
        funding_minor: 12000, funding_currency: *b"USD",
        payout_minor: 9600, payout_currency: *b"EUR",
        proposed_at: 100, accepted_at: 110, quote_expires_at: 120,
    };
    let s1=sign_terms(&key(3), &t).unwrap();
    let s2=sign_terms(&key(4), &t).unwrap();
    accept_terms(t, s1, s2).unwrap()
}

struct Fixture {
    terms: SignedTerms,
    agreed: AgreedInteraction,
    qr: SignedAuthorization,
    authorities: Authorities,
    evidence: EvidenceBundle,
}
fn assertion(key_id:u8, kind:Kind, c:&Context, version:u64)->SignedAssertion {
    sign_assertion(&key(key_id), kind, c.clone(), version, [key_id;32], 130, 180).unwrap()
}
fn setup(holder_machine:bool, cash_machine:bool, reverse:bool)->Fixture {
    let terms=accepted();
    let left_id=if holder_machine {7} else {2};
    let right_id=if cash_machine {8} else {4};
    let left=if holder_machine {Participant::machine(id(7),id(2)).unwrap()}
        else {Participant::human(id(2))};
    let right=if cash_machine {Participant::machine(id(8),id(4)).unwrap()}
        else {Participant::human(id(4))};
    let (first,second,s1,s2) = if reverse {
        (right,left,right_id,left_id)
    } else {
        (left,right,left_id,right_id)
    };
    let intent=InteractionIntent {
        initiator:first,responder:second,action:Action::DistributeCash,
        hawala_terms_digest:terms.terms().digest(),interaction_nonce:[21;32],
        created_at:125,expires_at:190,
    };
    let a=sign_intent(&key(s1),&intent).unwrap();
    let b=sign_intent(&key(s2),&intent).unwrap();
    let agreed=agree(intent,a,b,135).unwrap();
    let req=AuthorizationRequest {
        holder:id(left_id),verifier:id(right_id),
        transfer_id:terms.terms().transfer_id,
        terms_digest:terms.terms().digest(),
        interaction_nonce:[21;32],action:Action::DistributeCash,
        amount_minor:9600,currency:*b"EUR",disclosed_fee_minor:25,
        attempt_id:[22;32],challenge:[23;32],
        qr_nonce:[24;32],session_digest:[25;32],
        issued_at:130,expires_at:175,
    };
    let qr=sign_authorization(&key(left_id),req,&terms,&agreed,135).unwrap();
    let authorities=Authorities{
        eligibility_issuer:id(5),custody_issuer:id(6),epoch:3,
        expected_fee_minor:25,min_custody_version:11,
    };
    let ctx=Context {
        transfer_id:terms.terms().transfer_id,
        terms_digest:terms.terms().digest(),
        attempt_id:qr.request().attempt_id,
        qr_digest:qr.digest(),
        beneficiary:id(2),disburser:id(4),
        holder:id(left_id),cash_peer:id(right_id),
        amount_minor:9600,currency:*b"EUR",fee_minor:25,epoch:3,
    };
    let evidence=EvidenceBundle {
        eligibility:assertion(5,Kind::Eligibility,&ctx,15),
        custody_hold:assertion(6,Kind::CustodyHold,&ctx,11),
        holder_delegation:if holder_machine {
            Some(assertion(2,Kind::DelegateHolder,&ctx,1))
        } else {None},
        cash_peer_delegation:if cash_machine {
            Some(assertion(4,Kind::DelegateCashPeer,&ctx,1))
        } else {None},
    };
    Fixture {terms,agreed,qr,authorities,evidence}
}
fn eval(f:&Fixture)->hawala_admission::Result<hawala_admission::EvaluatedEvidence> {
    evaluate(&f.terms,&f.agreed,&f.qr,[24;32],[23;32],[25;32],
        &f.authorities,&f.evidence,150)
}

#[test]
fn accepts_signed_context_for_each_oriented_human_machine_pair() {
    for holder_machine in [false,true] {
        for cash_machine in [false,true] {
            for reverse in [false,true] {
                let f=setup(holder_machine,cash_machine,reverse);
                let result=eval(&f).unwrap();
                assert_eq!(result.transfer_id,[10;32]);
                assert_eq!(result.attempt_id,[22;32]);
                assert_eq!(result.cash_peer,if cash_machine{id(8)}else{id(4)});
                assert_eq!(result.custody_version,11);
            }
        }
    }
}

#[test]
fn missing_machine_delegation_and_unexpected_human_delegation_are_blocked() {
    let mut f=setup(false,true,false);
    f.evidence.cash_peer_delegation=None;
    assert_eq!(eval(&f),Err(Error::MissingDelegation));
    let mut f=setup(true,false,false);
    f.evidence.holder_delegation=None;
    assert_eq!(eval(&f),Err(Error::MissingDelegation));
    let mut f=setup(false,false,false);
    f.evidence.holder_delegation=Some(f.evidence.eligibility.clone());
    assert_eq!(eval(&f),Err(Error::UnexpectedDelegation));
}

#[test]
fn wrong_issuer_kind_signature_policy_or_version_are_rejected() {
    let mut f=setup(true,true,false);
    f.evidence.custody_hold.issuer=id(5);
    assert_eq!(eval(&f),Err(Error::WrongIssuer));
    let mut f=setup(false,false,false);
    f.evidence.eligibility.kind=Kind::CustodyHold;
    assert_eq!(eval(&f),Err(Error::WrongKind));
    let mut f=setup(false,false,false);
    f.evidence.eligibility.signature[0]^=1;
    assert_eq!(eval(&f),Err(Error::InvalidSignature));
    let mut f=setup(false,false,false);
    f.authorities.custody_issuer=f.authorities.eligibility_issuer;
    assert_eq!(eval(&f),Err(Error::UntrustedPolicy));
    let mut f=setup(false,false,false);
    f.authorities.min_custody_version=12;
    assert_eq!(eval(&f),Err(Error::UntrustedPolicy));
}

#[test]
fn altered_epoch_fee_attempt_and_expiration_fail_closed() {
    let mut f=setup(false,true,false);
    f.authorities.epoch=4;
    assert_eq!(eval(&f),Err(Error::WrongContext));
    let mut f=setup(false,true,false);
    f.authorities.expected_fee_minor=26;
    assert_eq!(eval(&f),Err(Error::WrongContext));
    let mut f=setup(false,true,false);
    f.evidence.custody_hold.context.attempt_id=[99;32];
    assert_eq!(eval(&f),Err(Error::WrongContext));
    let mut f=setup(false,false,false);
    f.evidence.eligibility.expires_at=151;
    assert_eq!(eval(&f),Err(Error::InvalidSignature));
    let mut f=setup(false,false,false);
    f.evidence.custody_hold.issued_at=151;
    assert_eq!(eval(&f),Err(Error::NotYetValid));
}

#[test]
fn wrong_qr_challenge_or_live_session_never_reaches_admission() {
    let f=setup(false,true,false);
    assert_eq!(evaluate(&f.terms,&f.agreed,&f.qr,[24;32],[23;32],[99;32],
        &f.authorities,&f.evidence,150),Err(Error::InvalidAuthorization));
    assert_eq!(evaluate(&f.terms,&f.agreed,&f.qr,[24;32],[99;32],[25;32],
        &f.authorities,&f.evidence,150),Err(Error::InvalidAuthorization));
}
