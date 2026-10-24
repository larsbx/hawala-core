use ed25519_dalek::SigningKey;
use hawala_admission::{sign_assertion, Authorities, Context, EvidenceBundle, Kind};
use hawala_authority::{
    admit_preview, reserve_event_id, AuthorityEnvelope, Decision, ExternalReviews,
    HoldReason, Quote, RejectReason, Review, Versions,
};
use hawala_identity_core::IdentityId;
use hawala_interaction::{
    agree, sign_intent, Action, AgreedInteraction, InteractionIntent, Participant,
};
use hawala_ledger::{Account, Book, Entry, NormalSide, Posting};
use hawala_protocol::{accept_terms, sign_terms, SignedTerms, Terms};
use hawala_qr_auth::{sign_authorization, AuthorizationRequest, SignedAuthorization};

fn key(n: u8) -> SigningKey { SigningKey::from_bytes(&[n;32]) }
fn id(n: u8) -> IdentityId { IdentityId::from_signing_key(&key(n)) }
fn accepted() -> SignedTerms {
    let t = Terms {
        transfer_id: [10;32], sender: id(1), beneficiary: id(2),
        origin: id(3), disburser: id(4),
        funding_minor: 1000, funding_currency: *b"USD",
        payout_minor: 900, payout_currency: *b"EUR",
        proposed_at: 100, accepted_at: 110, quote_expires_at: 120,
    };
    let o = sign_terms(&key(3),&t).unwrap();
    let d = sign_terms(&key(4),&t).unwrap();
    accept_terms(t,o,d).unwrap()
}
fn book() -> Book {
    let mut book = Book::new([
        Account{id:[41;32],currency:*b"EUR",normal:NormalSide::Debit},
        Account{id:[42;32],currency:*b"EUR",normal:NormalSide::Credit},
    ]).unwrap();
    assert_eq!(book.post(Entry{event_id:[99;32],postings:vec![
        Posting{account:[41;32],debit_minor:1000,credit_minor:0},
        Posting{account:[42;32],debit_minor:0,credit_minor:1000},
    ]}),Ok(true));
    book
}
fn reserve() -> Entry {
    Entry{event_id:reserve_event_id([10;32]),postings:vec![
        Posting{account:[42;32],debit_minor:900,credit_minor:0},
        Posting{account:[41;32],debit_minor:0,credit_minor:900},
    ]}
}
struct Fixture {
    terms: SignedTerms,
    agreed: AgreedInteraction,
    qr: SignedAuthorization,
    policy: Authorities,
    evidence: EvidenceBundle,
    quote: Quote,
    versions: Versions,
    reviews: ExternalReviews,
    reserve: Entry,
}
impl Fixture {
    fn envelope(&self) -> AuthorityEnvelope<'_> {
        AuthorityEnvelope {
            accepted:&self.terms,agreement:&self.agreed,qr:&self.qr,
            expected_qr_nonce:[24;32],expected_challenge:[23;32],
            expected_session:[25;32],expected_verifier:self.qr.request().verifier,
            pinned_authorities:&self.policy,assertions:&self.evidence,
            exact_quote:self.quote,versions:self.versions,
            external_reviews:self.reviews,reserve_entry:&self.reserve,
        }
    }
}
fn fixture(holder_machine:bool,cash_machine:bool,reverse:bool) -> Fixture {
    let terms=accepted();
    let lh=if holder_machine {7} else {2};
    let rh=if cash_machine {8} else {4};
    let left=if holder_machine {Participant::machine(id(lh),id(2)).unwrap()}
        else {Participant::human(id(2))};
    let right=if cash_machine {Participant::machine(id(rh),id(4)).unwrap()}
        else {Participant::human(id(4))};
    let (first,second,first_key,second_key)=if reverse {
        (right,left,rh,lh)
    } else { (left,right,lh,rh) };
    let intent=InteractionIntent {
        initiator:first,responder:second,action:Action::DistributeCash,
        hawala_terms_digest:terms.terms().digest(),interaction_nonce:[21;32],
        created_at:125,expires_at:190,
    };
    let s1=sign_intent(&key(first_key),&intent).unwrap();
    let s2=sign_intent(&key(second_key),&intent).unwrap();
    let agreed=agree(intent,s1,s2,135).unwrap();
    let req=AuthorizationRequest {
        holder:id(lh),verifier:id(rh),transfer_id:terms.terms().transfer_id,
        terms_digest:terms.terms().digest(),interaction_nonce:[21;32],
        action:Action::DistributeCash,amount_minor:900,currency:*b"EUR",
        disclosed_fee_minor:100,attempt_id:[22;32],challenge:[23;32],
        qr_nonce:[24;32],session_digest:[25;32],issued_at:130,expires_at:175,
    };
    let qr=sign_authorization(&key(lh),req,&terms,&agreed,135).unwrap();
    let policy=Authorities{
        eligibility_issuer:id(5),custody_issuer:id(6),epoch:3,
        expected_fee_minor:100,min_custody_version:11,
    };
    let context=Context{
        transfer_id:terms.terms().transfer_id,terms_digest:terms.terms().digest(),
        attempt_id:[22;32],qr_digest:qr.digest(),beneficiary:id(2),disburser:id(4),
        holder:id(lh),cash_peer:id(rh),amount_minor:900,
        currency:*b"EUR",fee_minor:100,epoch:3,
    };
    let assertion=|who,kind,version|sign_assertion(
        &key(who),kind,context.clone(),version,[who;32],130,180,
    ).unwrap();
    let evidence=EvidenceBundle{
        eligibility:assertion(5,Kind::Eligibility,15),
        custody_hold:assertion(6,Kind::CustodyHold,11),
        holder_delegation:if holder_machine{
            Some(assertion(2,Kind::DelegateHolder,1))
        } else {None},
        cash_peer_delegation:if cash_machine{
            Some(assertion(4,Kind::DelegateCashPeer,1))
        } else {None},
    };
    Fixture{
        terms,agreed,qr,policy,evidence,
        quote:Quote{rate_num:1,rate_den:1,source_fee_minor:0,
                    destination_fee_minor:100,quote_digest:[50;32]},
        versions:Versions{
            authority_epoch:3,account_version:20,
            custody_account_version:11,custody_authority_epoch:3,
        },
        reviews:ExternalReviews{
            custody_asset_control:Review::ReportedClear,
            funding_finality:Review::ReportedClear,
            dar_eligibility:Review::ReportedClear,
            sanctions_and_aml:Review::ReportedClear,
            secured_exposure:Review::ReportedClear,
            recipient_quote_acceptance:Review::ReportedClear,
        },
        reserve:reserve(),
    }
}
#[test]
fn all_oriented_human_machine_cases_preflight_identical_financial_terms() {
    for h in [false,true] {
        for m in [false,true] {
            for reverse in [false,true] {
                let f=fixture(h,m,reverse);
                let original=book();
                let result=admit_preview(&f.envelope(),&original,140);
                match result {
                    Decision::Prepared(plan)=>{
                        assert_eq!(plan.transfer_id(),[10;32]);
                        assert_eq!(plan.attempt_id(),[22;32]);
                        assert_eq!(plan.payout_minor(),900);
                        assert_eq!(plan.ledger_event_id(),reserve_event_id([10;32]));
                    }
                    other=>panic!("fixture should preflight: {other:?}")
                }
                assert_eq!(original.balance([41;32]),Some(1000)); // pure!
                assert_eq!(original.event_count(),1);
            }
        }
    }
}
#[test]
fn unknown_or_denied_independent_checks_fail_closed() {
    let mut f=fixture(false,true,false);
    f.reviews.custody_asset_control=Review::Unknown;
    assert_eq!(admit_preview(&f.envelope(),&book(),140),
        Decision::Held(HoldReason::MissingIndependentEvidence));
    f.reviews.custody_asset_control=Review::Denied;
    assert_eq!(admit_preview(&f.envelope(),&book(),140),
        Decision::Rejected(RejectReason::DeniedByExternalControl));
    f.reviews.custody_asset_control=Review::ReportedClear;
    f.evidence.cash_peer_delegation=None;
    assert_eq!(admit_preview(&f.envelope(),&book(),140),
        Decision::Rejected(RejectReason::InvalidEvidence));
}
#[test]
fn fx_fee_stale_epoch_and_inconsistent_reserve_cannot_prepare() {
    let mut f=fixture(false,false,false);
    f.quote.rate_num=2;
    assert_eq!(admit_preview(&f.envelope(),&book(),140),
        Decision::Rejected(RejectReason::InvalidQuote));
    f.quote.rate_num=1;
    f.versions.custody_account_version=10;
    assert_eq!(admit_preview(&f.envelope(),&book(),140),
        Decision::Held(HoldReason::StaleVersionOrEpoch));
    f.versions.custody_account_version=11;
    f.versions.authority_epoch=2;
    assert_eq!(admit_preview(&f.envelope(),&book(),140),
        Decision::Held(HoldReason::StaleVersionOrEpoch));
    f.versions.authority_epoch=3;
    f.reserve.postings[0].debit_minor=901;
    assert_eq!(admit_preview(&f.envelope(),&book(),140),
        Decision::Rejected(RejectReason::InvalidLedgerEntry));
    f.reserve=reserve();
    f.reserve.event_id=[60;32];
    assert_eq!(admit_preview(&f.envelope(),&book(),140),
        Decision::Rejected(RejectReason::WrongEventId));
}
#[test]
fn a_prepared_plan_is_not_state_mutation_or_reusable_spend() {
    let f=fixture(false,false,false);
    let mut b=book();
    let first=admit_preview(&f.envelope(),&b,140);
    let second=admit_preview(&f.envelope(),&b,140);
    assert_eq!(first,second);
    assert_eq!(b.balance([42;32]),Some(1000));
    assert_eq!(b.post(f.reserve.clone()),Ok(true));
    assert_eq!(admit_preview(&f.envelope(),&b,140),
        Decision::Rejected(RejectReason::InvalidLedgerEntry));
}
