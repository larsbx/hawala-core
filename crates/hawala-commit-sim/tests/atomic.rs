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
    assert_eq!(sim.admit(&f.envelope(),&f.policy,&c,&r,1000).unwrap(),result);
    sim.mark_uncertain([10;32]).unwrap();drop(sim);
    let sim=Simulator::open(&path).unwrap();
    assert!(sim.is_uncertain([10;32]).unwrap());
    // Unknown external execution permanently holds even an identical replay.
    assert_eq!(sim.admit(&f.envelope(),&f.policy,&c,&r,1000).unwrap(),Admission::Held);
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
    assert_eq!(sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap(),Admission::Held);
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
#[test]
fn unrelated_funded_accounts_do_not_exceed_reference_journal_limit() {
    let f=fixture(false,false,false);let (lot,r,c)=custody_fixture(&f);
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("large.sqlite");
    let sim=Simulator::open(&path).unwrap();
    let mut accounts=vec![
        SyntheticAccount{account:Account{id:[41;32],currency:*b"EUR",normal:NormalSide::Debit},role:AccountRole::SyntheticAsset,balance_minor:1000},
        SyntheticAccount{account:Account{id:[42;32],currency:*b"EUR",normal:NormalSide::Credit},role:AccountRole::AvailableLiability,balance_minor:1000},
        SyntheticAccount{account:Account{id:[43;32],currency:*b"EUR",normal:NormalSide::Credit},role:AccountRole::ReservedLiability,balance_minor:0},
    ];
    for n in 0..130u32 {
        let mut asset=[90;32];asset[..4].copy_from_slice(&n.to_be_bytes());
        let mut liability=[91;32];liability[..4].copy_from_slice(&n.to_be_bytes());
        accounts.push(SyntheticAccount{account:Account{id:asset,currency:*b"EUR",normal:NormalSide::Debit},role:AccountRole::SyntheticAsset,balance_minor:7});
        accounts.push(SyntheticAccount{account:Account{id:liability,currency:*b"EUR",normal:NormalSide::Credit},role:AccountRole::AvailableLiability,balance_minor:7});
    }
    sim.seed_synthetic(&accounts,3,20).unwrap();sim.register_test_lot(&lot).unwrap();
    assert!(matches!(sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap(),Admission::Committed(_)));
    assert_eq!(sim.balance([42;32]).unwrap(),Some(100));
    for a in &accounts[3..] {assert_eq!(sim.balance(a.account.id).unwrap(),Some(7));}
}

use hawala_commit_sim::{ExecutionObservation,ObservationKind,ObservationOutcome,SignedObservation,OBSERVATION_FRAME_LEN};
fn report(cash_key:u8,kind:ObservationKind)->SignedObservation {
    SignedObservation::sign(&key(cash_key),ExecutionObservation{
        transfer_id:[10;32],attempt_id:[22;32],observation_id:[80;32],
        epoch:3,observed_at:145,expires_at:160,kind,
    }).unwrap()
}
#[test]
fn signed_observation_replay_conflict_and_restart_never_release_or_repeat() {
    let f=fixture(false,false,false);let (lot,r,c)=custody_fixture(&f);
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("reports.sqlite");
    let sim=Simulator::open(&path).unwrap();seeded(&sim,3,20,1000);sim.register_test_lot(&lot).unwrap();
    sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap();
    let timeout=report(4,ObservationKind::Timeout);
    assert_eq!(timeout.encode().len(),OBSERVATION_FRAME_LEN);
    assert_eq!(SignedObservation::decode(&timeout.encode()).unwrap(),timeout);
    assert_eq!(sim.record_observation(&timeout,id(4),150).unwrap(),ObservationOutcome::RecordedHeld);
    let delivery=report(4,ObservationKind::ReportedDelivery);
    let mut future=delivery.observation();future.observed_at=151;
    let future=SignedObservation::sign(&key(4),future).unwrap();
    assert_eq!(sim.record_observation(&future,id(4),150).unwrap(),ObservationOutcome::Rejected);
    let mut expired=delivery.observation();expired.expires_at=149;
    let expired=SignedObservation::sign(&key(4),expired).unwrap();
    assert_eq!(sim.record_observation(&expired,id(4),150).unwrap(),ObservationOutcome::Rejected);
    assert_eq!(sim.record_observation(&delivery,id(4),150).unwrap(),ObservationOutcome::ConflictHeld);
    drop(sim);let sim=Simulator::open(&path).unwrap();
    assert_eq!(sim.record_observation(&timeout,id(4),1000).unwrap(),ObservationOutcome::IdenticalReplayHeld);
    assert_eq!(sim.record_observation(&delivery,id(4),1000).unwrap(),ObservationOutcome::ConflictHeld);
    assert!(sim.is_uncertain([10;32]).unwrap());
    assert_eq!(sim.admit(&f.envelope(),&f.policy,&c,&r,150).unwrap(),Admission::Held);
    assert_eq!(sim.event_count().unwrap(),1);assert_eq!(sim.state_version().unwrap(),21);
    assert_eq!(sim.balance([42;32]).unwrap(),Some(100));assert_eq!(sim.balance([43;32]).unwrap(),Some(900));
    let audit=rusqlite::Connection::open(&path).unwrap();
    assert_eq!(audit.query_row("SELECT count(*) FROM recovery_conflicts",[],|r|r.get::<_,u64>(0)).unwrap(),1);
}
#[test]
fn forged_misbound_stale_and_uncommitted_reports_fail_closed() {
    let f=fixture(false,true,false);let (lot,r,c)=custody_fixture(&f);
    let dir=tempfile::tempdir().unwrap();let sim=Simulator::open(dir.path().join("bad-report.sqlite")).unwrap();
    seeded(&sim,3,20,1000);sim.register_test_lot(&lot).unwrap();
    let good=report(8,ObservationKind::ReportedDelivery);
    assert_eq!(sim.record_observation(&good,id(8),150).unwrap(),ObservationOutcome::Rejected);
    sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap();
    for peer in [id(2),id(4)] {assert_eq!(sim.record_observation(&good,peer,150).unwrap(),ObservationOutcome::Rejected);}
    assert_eq!(sim.record_observation(&report(4,ObservationKind::Timeout),id(4),150).unwrap(),ObservationOutcome::Rejected);
    for which in 0..5 {
        let mut o=good.observation();
        match which {0=>o.epoch=4,1=>o.attempt_id=[99;32],2=>o.transfer_id=[99;32],3=>o.observed_at=151,_=>o.expires_at=149}
        let bad=SignedObservation::sign(&key(8),o).unwrap();
        assert_eq!(sim.record_observation(&bad,id(8),150).unwrap(),ObservationOutcome::Rejected);
    }
    let wire=good.encode();
    for i in [0,8,40,72,104,136,144,152,160,161,224] {
        let mut bad=wire.clone();bad[i]^=1;assert!(SignedObservation::decode(&bad).is_err());
    }
    assert!(SignedObservation::decode(&wire[..wire.len()-1]).is_err());
    let mut trailing=wire;trailing.push(0);assert!(SignedObservation::decode(&trailing).is_err());
    assert!(!sim.is_uncertain([10;32]).unwrap());
    assert_eq!(sim.balance([43;32]).unwrap(),Some(900));
}

#[derive(Default)]
struct RecoveryHandshakeNonces(std::sync::Mutex<std::collections::HashSet<(IdentityId,IdentityId,[u8;32])>>);
impl hawala_identity_core::ReplayGuard for RecoveryHandshakeNonces {
    fn consume_once(&self,v:IdentityId,c:IdentityId,n:[u8;32])->hawala_identity_core::Result<bool> {
        Ok(self.0.lock().unwrap().insert((v,c,n)))
    }
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn signed_recovery_over_live_iroh_preserves_holds_for_all_peer_modes() {
    use hawala_iroh_bridge::{handshake,IDENTITY_ALPN};
    use iroh::{Endpoint,EndpointAddr,endpoint::presets};
    for (holder_machine,cash_machine) in [(false,false),(false,true),(true,false),(true,true)] {
        let f=fixture(holder_machine,cash_machine,false);let (lot,r,c)=custody_fixture(&f);
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("iroh-recovery.sqlite");
        let sim=Simulator::open(&path).unwrap();seeded(&sim,3,20,1000);sim.register_test_lot(&lot).unwrap();
        sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap();
        let holder=if holder_machine {7}else{2};let cash=if cash_machine {8}else{4};
        // Explicit loopback addresses avoid discovery/relay dependencies in the harness.
        let sender=Endpoint::builder(presets::Minimal).alpns(vec![IDENTITY_ALPN.to_vec()])
            .bind_addr("127.0.0.1:0").unwrap().bind().await.unwrap();
        let receiver=Endpoint::builder(presets::Minimal).alpns(vec![IDENTITY_ALPN.to_vec()])
            .bind_addr("127.0.0.1:0").unwrap().bind().await.unwrap();
        let target=EndpointAddr::new(receiver.id()).with_ip_addr(receiver.bound_sockets()[0]);
        let report=report(cash,ObservationKind::Timeout);let wire=report.encode();
        let left_guard=RecoveryHandshakeNonces::default();let right_guard=RecoveryHandshakeNonces::default();
        tokio::time::timeout(std::time::Duration::from_secs(30),async {
            tokio::join!(async {
                let conn=sender.connect(target,IDENTITY_ALPN).await.unwrap();
                handshake::initiate(&sender,&conn,&key(cash),id(holder),&left_guard).await.unwrap();
                let mut stream=conn.open_uni().await.unwrap();stream.write_all(&wire).await.unwrap();stream.finish().unwrap();
                stream.stopped().await.unwrap();
            },async {
                let conn=receiver.accept().await.unwrap().await.unwrap();
                let verified=handshake::accept(&receiver,&conn,&key(holder),id(cash),&right_guard).await.unwrap();
                let mut stream=conn.accept_uni().await.unwrap();
                let bytes=stream.read_to_end(OBSERVATION_FRAME_LEN).await.unwrap();
                let observation=SignedObservation::decode(&bytes).unwrap();
                assert_eq!(sim.record_observation(&observation,verified.identity(),150).unwrap(),ObservationOutcome::RecordedHeld);
            });
        }).await.expect("signed recovery transport timed out");
        sender.close().await;receiver.close().await;
        drop(sim);let sim=Simulator::open(&path).unwrap();
        assert_eq!(sim.record_observation(&report,id(cash),1000).unwrap(),ObservationOutcome::IdenticalReplayHeld);
        assert_eq!(sim.admit(&f.envelope(),&f.policy,&c,&r,150).unwrap(),Admission::Held);
        assert_eq!(sim.event_count().unwrap(),1);assert_eq!(sim.balance([43;32]).unwrap(),Some(900));
    }
}
#[test]
fn observation_and_uncertainty_transition_roll_back_together() {
    let f=fixture(false,false,false);let (lot,r,c)=custody_fixture(&f);
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("report-fault.sqlite");
    let sim=Simulator::open(&path).unwrap();seeded(&sim,3,20,1000);sim.register_test_lot(&lot).unwrap();
    sim.admit(&f.envelope(),&f.policy,&c,&r,140).unwrap();
    let fault=rusqlite::Connection::open(&path).unwrap();
    fault.execute_batch("CREATE TRIGGER reject_uncertainty BEFORE UPDATE ON admission_receipts BEGIN SELECT RAISE(ABORT,'injected report failure'); END;").unwrap();
    let signed=report(4,ObservationKind::Timeout);
    assert!(sim.record_observation(&signed,id(4),150).is_err());
    assert!(!sim.is_uncertain([10;32]).unwrap());
    assert_eq!(fault.query_row("SELECT count(*) FROM recovery_observations",[],|r|r.get::<_,u64>(0)).unwrap(),0);
    fault.execute_batch("DROP TRIGGER reject_uncertainty").unwrap();
    assert_eq!(sim.record_observation(&signed,id(4),150).unwrap(),ObservationOutcome::RecordedHeld);
    assert_eq!(sim.event_count().unwrap(),1);assert_eq!(sim.balance([43;32]).unwrap(),Some(900));
}
