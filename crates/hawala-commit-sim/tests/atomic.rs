use ed25519_dalek::SigningKey;
use hawala_commit_sim::{AccountRole,Outcome,Simulator,SyntheticAccount,Error as SimError};
use hawala_admission::{sign_assertion, Authorities, Context, EvidenceBundle, Kind};
use hawala_authority::{
    admit_preview, reserve_event_id, AuthorityEnvelope, Decision, ExternalReviews,
    Quote, Review, Versions,
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
        Account{id:[43;32],currency:*b"EUR",normal:NormalSide::Credit},
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
        Posting{account:[43;32],debit_minor:0,credit_minor:900},
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
            expected_qr_nonce:self.qr.request().qr_nonce,expected_challenge:[23;32],
            expected_session:[25;32],expected_verifier:self.qr.request().verifier,
            pinned_authorities:&self.policy,assertions:&self.evidence,
            exact_quote:self.quote,versions:self.versions,
            external_reviews:self.reviews,reserve_entry:&self.reserve,
        }
    }
}
fn fixture(holder_machine:bool,cash_machine:bool,reverse:bool) -> Fixture {
    fixture_attempt(holder_machine,cash_machine,reverse,22,24)
}
fn fixture_attempt(holder_machine:bool,cash_machine:bool,reverse:bool,attempt:u8,nonce:u8) -> Fixture {
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
        disclosed_fee_minor:100,attempt_id:[attempt;32],challenge:[23;32],
        qr_nonce:[nonce;32],session_digest:[25;32],issued_at:130,expires_at:175,
    };
    let qr=sign_authorization(&key(lh),req,&terms,&agreed,135).unwrap();
    let policy=Authorities{
        eligibility_issuer:id(5),custody_issuer:id(6),epoch:3,
        expected_fee_minor:100,min_custody_version:11,
    };
    let context=Context{
        transfer_id:terms.terms().transfer_id,terms_digest:terms.terms().digest(),
        attempt_id:[attempt;32],qr_digest:qr.digest(),beneficiary:id(2),disburser:id(4),
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

fn seeded(sim:&Simulator,epoch:u64,version:u64,available:u64) {
    sim.seed_synthetic(&[
        SyntheticAccount{account:Account{id:[41;32],currency:*b"EUR",normal:NormalSide::Debit},
                         role:AccountRole::SyntheticAsset,balance_minor:available},
        SyntheticAccount{account:Account{id:[42;32],currency:*b"EUR",normal:NormalSide::Credit},
                         role:AccountRole::AvailableLiability,balance_minor:available},
        SyntheticAccount{account:Account{id:[43;32],currency:*b"EUR",normal:NormalSide::Credit},
                         role:AccountRole::ReservedLiability,balance_minor:0},
    ],epoch,version).unwrap();
}
#[test]
fn one_sqlite_commit_moves_only_synthetic_liability_to_reserved_for_all_peer_modes() {
    for human_device in [(false,false),(false,true),(true,false),(true,true)] {
        let d=tempfile::tempdir().unwrap();let path=d.path().join("sim.sqlite");
        let f=fixture(human_device.0,human_device.1,false);
        let plan=match admit_preview(&f.envelope(),&book(),140){
            Decision::Prepared(p)=>p,other=>panic!("bad preview {other:?}"),
        };
        let sim=Simulator::open(&path).unwrap();
        seeded(&sim,3,20,1000);
        let one=sim.record_simulated_hold(&plan,&f.reserve).unwrap();
        assert!(matches!(one,Outcome::RecordedSimulation(_)));
        assert_eq!(sim.balance([41;32]).unwrap(),Some(1000)); // asset unchanged
        assert_eq!(sim.balance([42;32]).unwrap(),Some(100));  // available decrease
        assert_eq!(sim.balance([43;32]).unwrap(),Some(900));  // reserved increase
        assert_eq!(sim.state_version().unwrap(),21);
        assert_eq!(sim.event_count().unwrap(),1);
        assert_eq!(sim.record_simulated_hold(&plan,&f.reserve).unwrap(),
                   Outcome::IdenticalReplay(match one{
                        Outcome::RecordedSimulation(v)=>v,_=>unreachable!()
                   }));
        drop(sim);
        let sim=Simulator::open(&path).unwrap();
        assert!(matches!(sim.record_simulated_hold(&plan,&f.reserve),
                         Ok(Outcome::IdenticalReplay(_))));
        assert_eq!(sim.balance([43;32]).unwrap(),Some(900));
        assert_eq!(sim.event_count().unwrap(),1);
    }
}
#[test]
fn stale_epoch_or_persisted_balance_prevents_partial_commit() {
    let f=fixture(false,false,false);
    let plan=match admit_preview(&f.envelope(),&book(),140){
        Decision::Prepared(p)=>p,_=>panic!("no plan"),
    };
    for (epoch,available) in [(4,1000),(3,800)] {
        let d=tempfile::tempdir().unwrap();
        let sim=Simulator::open(d.path().join("db.sqlite")).unwrap();
        seeded(&sim,epoch,20,available);
        assert!(matches!(sim.record_simulated_hold(&plan,&f.reserve),Err(SimError::Stale)));
        assert_eq!(sim.event_count().unwrap(),0);
        assert_eq!(sim.balance([42;32]).unwrap(),Some(available));
        assert_eq!(sim.balance([43;32]).unwrap(),Some(0));
    }
}
#[test]
fn altered_postings_and_reused_attempt_cannot_create_additional_reservation() {
    let f=fixture(false,true,false);
    let plan=match admit_preview(&f.envelope(),&book(),140){
        Decision::Prepared(p)=>p,_=>panic!("no plan"),
    };
    let d=tempfile::tempdir().unwrap();
    let sim=Simulator::open(d.path().join("db.sqlite")).unwrap();
    seeded(&sim,3,20,1000);
    let mut invalid=f.reserve.clone();
    invalid.postings[1].credit_minor=899;
    assert!(matches!(sim.record_simulated_hold(&plan,&invalid),Err(SimError::InvalidEntry)));
    assert_eq!(sim.event_count().unwrap(),0);
    sim.record_simulated_hold(&plan,&f.reserve).unwrap();
    let mut altered=f.reserve.clone();
    altered.postings[0].account=[41;32];
    assert!(matches!(sim.record_simulated_hold(&plan,&altered),Err(SimError::InvalidEntry)));
    assert_eq!(sim.event_count().unwrap(),1);
    assert_eq!(sim.balance([42;32]).unwrap(),Some(100));
}
#[test]
fn file_lock_and_fixture_seeding_require_one_authority() {
    let d=tempfile::tempdir().unwrap();let path=d.path().join("sim.sqlite");
    let sim=Simulator::open(&path).unwrap();
    seeded(&sim,3,20,1000);
    assert!(matches!(sim.seed_synthetic(&[],3,20),Err(SimError::Invalid)));
    assert!(matches!(Simulator::open(&path),Err(SimError::Io(_))));
    drop(sim);
    assert!(Simulator::open(&path).is_ok());
}

use hawala_commit_sim::{Admission, SignedTestCustody, TestLot, UnsupportedRealCustody};
use hawala_custody_model::{CustodyPolicy,Mode,Request,Command,sign_request};
fn custody_fixture(f:&Fixture)->(TestLot,Request,SignedTestCustody) {
    let policy=CustodyPolicy{network_id:[60;32],custody_account:[61;32],
        depositing_dar:id(4),currency:*b"EUR",mandate_digest:[62;32],epoch:3,
        mode:Mode::IndependentEscrow{controller:id(6)}};
    let r=Request{network_id:policy.network_id,custody_account:policy.custody_account,
        depositing_dar:id(4),currency:*b"EUR",mandate_digest:policy.mandate_digest,
        policy_digest:policy.digest().unwrap(),epoch:3,expected_version:11,
        request_id:f.qr.request().attempt_id,allocation_id:[63;32],obligation_digest:f.terms.terms().digest(),
        command:Command::Reserve,amount_minor:900};
    let approvals=vec![sign_request(&policy,&r,&key(6)).unwrap()];
    let lot=TestLot{id:r.allocation_id,terms_digest:r.obligation_digest,
        available_account:[42;32],reserved_account:[43;32],custody_account:r.custody_account,
        custody_version:11,custody_epoch:3};
    (lot,r,SignedTestCustody{policy,approvals,version:11,valid_until:170,capacity_minor:1000})
}
#[test]
fn integrated_admission_is_atomic_replayable_and_uncertainty_survives_restart() {
    let f=fixture(true,true,false); let (lot,r,c)=custody_fixture(&f);
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("integrated.sqlite");
    let sim=Simulator::open(&path).unwrap();seeded(&sim,3,20,1000);
    sim.register_test_lot(&lot).unwrap();
    assert_eq!(sim.admit(&f.envelope(),&f.policy,&UnsupportedRealCustody,&r,140).unwrap(),Admission::Held);
    assert_eq!(sim.event_count().unwrap(),0);
    let result=sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap();
    assert!(matches!(result,Admission::Committed(_)));
    sim.mark_uncertain([10;32]).unwrap();drop(sim);
    let sim=Simulator::open(&path).unwrap();
    assert!(sim.is_uncertain([10;32]).unwrap());
    // Receipt retrieval is idempotent even after QR/custody expiry.
    assert_eq!(sim.admit(&f.envelope(),&f.policy,&c,&r,1000).unwrap(),result);
    assert!(sim.is_uncertain([10;32]).unwrap());
    assert_eq!(sim.balance([42;32]).unwrap(),Some(100));
    assert_eq!(sim.balance([43;32]).unwrap(),Some(900));
    let mut conflicting=r.clone();conflicting.amount_minor=899;
    assert_eq!(sim.admit(&f.envelope(),&f.policy,&c,&conflicting,140).unwrap(),Admission::Rejected);
    assert_eq!(sim.event_count().unwrap(),1);
}
#[test]
fn integrated_partitions_stale_fences_and_invalid_envelopes_leave_no_effect() {
    let mut f=fixture(false,true,false);let (lot,r,mut c)=custody_fixture(&f);
    let dir=tempfile::tempdir().unwrap();let sim=Simulator::open(dir.path().join("holds.sqlite")).unwrap();
    seeded(&sim,3,20,1000);sim.register_test_lot(&lot).unwrap();
    let approvals=c.approvals.clone();c.approvals.clear();
    assert_eq!(sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap(),Admission::Held);
    c.approvals=approvals;c.version=12;
    assert_eq!(sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap(),Admission::Held);
    c.version=11;
    f.versions.authority_epoch=4;
    assert_eq!(sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap(),Admission::Held);
    f.versions.authority_epoch=3;
    f.quote.rate_num=2;
    assert_eq!(sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap(),Admission::Rejected);
    assert_eq!(sim.balance([42;32]).unwrap(),Some(1000));
    assert_eq!(sim.event_count().unwrap(),0);
}
#[test]
fn concurrent_identical_admissions_commit_once() {
    let f=fixture(false,false,false);let (lot,_,_)=custody_fixture(&f);
    let dir=tempfile::tempdir().unwrap();let sim=Simulator::open(dir.path().join("race.sqlite")).unwrap();
    seeded(&sim,3,20,1000);sim.register_test_lot(&lot).unwrap();
    std::thread::scope(|scope| {
        let handles:Vec<_>=(0..8).map(|_|scope.spawn(|| {
            let f=fixture(false,false,false);let (_,r,c)=custody_fixture(&f);
            sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap()
        })).collect();
        let results:Vec<_>=handles.into_iter().map(|h|h.join().unwrap()).collect();
        assert!(matches!(results[0],Admission::Committed(_)));
        assert!(results.iter().all(|r|*r==results[0]));
    });
    assert_eq!(sim.event_count().unwrap(),1);
    assert_eq!(sim.state_version().unwrap(),21);
}
#[test]
fn storage_failure_after_nonce_and_balance_writes_rolls_back_everything() {
    let f=fixture(false,false,false);let (lot,r,c)=custody_fixture(&f);
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("fault.sqlite");
    let sim=Simulator::open(&path).unwrap();seeded(&sim,3,20,1000);sim.register_test_lot(&lot).unwrap();
    let fault=rusqlite::Connection::open(&path).unwrap();
    fault.execute_batch("CREATE TRIGGER fail_receipt BEFORE INSERT ON admission_receipts BEGIN SELECT RAISE(ABORT,'injected storage failure'); END;").unwrap();
    assert!(sim.admit(&f.envelope(),&f.policy,&c,&r,140).is_err());
    assert_eq!(sim.event_count().unwrap(),0);
    assert_eq!(sim.state_version().unwrap(),20);
    assert_eq!(sim.balance([42;32]).unwrap(),Some(1000));
    drop(sim);fault.execute_batch("DROP TRIGGER fail_receipt").unwrap();drop(fault);
    let sim=Simulator::open(&path).unwrap();
    assert!(matches!(sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap(),Admission::Committed(_)));
    assert_eq!(sim.event_count().unwrap(),1);
}
#[test]
fn concurrent_distinct_signed_attempts_cannot_spend_the_same_lot_twice() {
    let f=fixture(false,false,false);let (lot,_,_)=custody_fixture(&f);
    let dir=tempfile::tempdir().unwrap();let sim=Simulator::open(dir.path().join("spend.sqlite")).unwrap();
    seeded(&sim,3,20,1000);sim.register_test_lot(&lot).unwrap();
    std::thread::scope(|scope| {
        let handles:Vec<_>=(0..8).map(|i|scope.spawn({let sim=&sim; move || {
            let f=fixture_attempt(false,false,false,22+i,24+i);let (_,r,c)=custody_fixture(&f);
            sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap()
        }})).collect();
        let results:Vec<_>=handles.into_iter().map(|h|h.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r|matches!(r,Admission::Committed(_))).count(),1);
        assert_eq!(results.iter().filter(|r|matches!(r,Admission::Rejected)).count(),7);
    });
    assert_eq!(sim.event_count().unwrap(),1);
    assert_eq!(sim.balance([43;32]).unwrap(),Some(900));
}
#[test]
fn crash_child_commits_without_running_destructors() {
    let Some(path)=std::env::var_os("HAWALA_TEST_CRASH_DB") else {return};
    let f=fixture(false,false,false);let (_,r,c)=custody_fixture(&f);
    let sim=Simulator::open(path).unwrap();
    assert!(matches!(sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap(),Admission::Committed(_)));
    sim.mark_uncertain([10;32]).unwrap();
    std::process::exit(0);
}
#[test]
fn abrupt_process_exit_recovers_the_same_intent_and_retains_hold() {
    let f=fixture(false,false,false);let (lot,r,c)=custody_fixture(&f);
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("crash.sqlite");
    let sim=Simulator::open(&path).unwrap();seeded(&sim,3,20,1000);sim.register_test_lot(&lot).unwrap();drop(sim);
    let status=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","crash_child_commits_without_running_destructors"])
        .env("HAWALA_TEST_CRASH_DB",&path).status().unwrap();
    assert!(status.success());
    let sim=Simulator::open(&path).unwrap();
    assert!(sim.is_uncertain([10;32]).unwrap());
    assert!(matches!(sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap(),Admission::Committed(_)));
    assert_eq!(sim.event_count().unwrap(),1);
    assert_eq!(sim.balance([42;32]).unwrap(),Some(100));
}
#[test]
fn integrated_federations_hold_during_partition_then_commit_one_reservation() {
    for (n,q,faults) in [(4,3,1),(8,6,2)] {
        let f=fixture(false,false,false);let (lot,mut r,mut c)=custody_fixture(&f);
        c.policy.mode=Mode::Federated{controllers:(70..70+n).map(id).collect(),threshold:q as usize,max_faults:faults};
        r.policy_digest=c.policy.digest().unwrap();
        c.approvals=(70..70+q-1).map(|i|sign_request(&c.policy,&r,&key(i)).unwrap()).collect();
        let dir=tempfile::tempdir().unwrap();let sim=Simulator::open(dir.path().join("quorum.sqlite")).unwrap();
        seeded(&sim,3,20,1000);sim.register_test_lot(&lot).unwrap();
        assert_eq!(sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap(),Admission::Held);
        assert_eq!(sim.balance([42;32]).unwrap(),Some(1000));
        c.approvals.push(sign_request(&c.policy,&r,&key(70+q-1)).unwrap());
        assert!(matches!(sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap(),Admission::Committed(_)));
    }
}
#[test]
fn forged_custody_or_peer_selected_financial_issuer_is_rejected() {
    let f=fixture(false,false,false);let (lot,r,mut c)=custody_fixture(&f);
    let dir=tempfile::tempdir().unwrap();let sim=Simulator::open(dir.path().join("forged.sqlite")).unwrap();
    seeded(&sim,3,20,1000);sim.register_test_lot(&lot).unwrap();
    c.approvals[0].signature[0]^=1;
    assert_eq!(sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap(),Admission::Rejected);
    let (_,_,c)=custody_fixture(&f);
    let mut pin=f.policy;pin.eligibility_issuer=id(9);
    assert_eq!(sim.admit(&f.envelope(),&pin,&c,&r,140).unwrap(),Admission::Rejected);
    let mut a=f.envelope();a.expected_verifier=id(9);
    assert_eq!(sim.admit(&a,&f.policy,&c,&r,140).unwrap(),Admission::Rejected);
    assert_eq!(sim.event_count().unwrap(),0);
}
