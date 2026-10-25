use std::sync::Arc;
use std::thread;
use ed25519_dalek::SigningKey;
use hawala_controller_fence::{FenceError,FencedController};
use hawala_custody_model::{
    examine, Command, CustodyPolicy, Decision, Error as ProtocolError, Mode, Request,
};
use hawala_identity_core::IdentityId;

fn key(n:u8)->SigningKey {SigningKey::from_bytes(&[n;32])}
fn id(n:u8)->IdentityId {IdentityId::from_signing_key(&key(n))}
fn policy(n:usize,threshold:usize,faults:usize)->CustodyPolicy {
    CustodyPolicy {
        network_id:[1;32],custody_account:[2;32],
        depositing_dar:id(4),currency:*b"EUR",
        mandate_digest:[3;32],epoch:10,
        mode:Mode::Federated {
            controllers:(10..10+n).map(|k|id(k as u8)).collect(),
            threshold,max_faults:faults,
        }
    }
}
fn escrow()->CustodyPolicy {
    CustodyPolicy{
        mode:Mode::IndependentEscrow{controller:id(10)},
        ..policy(4,3,1)
    }
}
fn request(p:&CustodyPolicy)->Request {
    Request {
        network_id:p.network_id,custody_account:p.custody_account,
        depositing_dar:p.depositing_dar,currency:p.currency,
        mandate_digest:p.mandate_digest,policy_digest:p.digest().unwrap(),
        epoch:p.epoch,expected_version:20,request_id:[5;32],
        allocation_id:[6;32],obligation_digest:[7;32],
        command:Command::Reserve,amount_minor:900,
    }
}

#[test]
fn single_escrow_controller_fences_conflicts_across_restart() {
    let p=escrow();let r=request(&p);
    let d=tempfile::tempdir().unwrap();let file=d.path().join("escrow.db");
    let original;
    {
        let signer=FencedController::open(&file,key(10),p.clone(),20).unwrap();
        assert_eq!(signer.controller(),id(10));
        assert!(!signer.has_signed().unwrap());
        original=signer.sign(&r).unwrap();
        assert!(signer.has_signed().unwrap());
        assert_eq!(signer.sign(&r).unwrap(),original);
        assert!(matches!(FencedController::open(&file,key(10),p.clone(),20),
                          Err(FenceError::Io(_))));
        let mut conflict=r.clone();
        conflict.amount_minor=901;
        assert!(matches!(signer.sign(&conflict),Err(FenceError::ConflictingRequest)));
        let mut conflict=r.clone();
        conflict.request_id=[8;32];
        assert!(matches!(signer.sign(&conflict),Err(FenceError::ConflictingRequest)));
    }
    let reopened=FencedController::open(&file,key(10),p.clone(),20).unwrap();
    assert_eq!(reopened.sign(&r).unwrap(),original);
    assert_eq!(examine(&p,&r,&[original]).unwrap(),
               Decision::SufficientSignatures(hawala_custody_model::SignerEvidence {
                   verified_signers:1,policy_digest:p.digest().unwrap(),
                   request_digest:r.digest()
               }));
    let mut conflict=r;
    conflict.command=Command::WithdrawFree;
    assert!(matches!(reopened.sign(&conflict),Err(FenceError::ConflictingRequest)));
}

#[test]
fn four_and_eight_controller_thresholds_require_real_distinct_local_fences() {
    for (n,q,faults) in [(4usize,3usize,1usize),(8,6,2)] {
        let p=policy(n,q,faults);let r=request(&p);
        let dir=tempfile::tempdir().unwrap();
        let mut approvals=Vec::new();
        for ix in 0..n {
            let who=10+ix as u8;
            let path=dir.path().join(format!("controller-{who}.db"));
            let signer=FencedController::open(path,key(who),p.clone(),20).unwrap();
            approvals.push(signer.sign(&r).unwrap());
        }
        assert_eq!(examine(&p,&r,&approvals[..q-1]).unwrap(),
                   Decision::Hold{got:q-1,needed:q});
        assert!(matches!(examine(&p,&r,&approvals[..q]),
                         Ok(Decision::SufficientSignatures(_))));
        let mut different=r.clone();different.allocation_id=[88;32];
        let signer=FencedController::open(
            dir.path().join("controller-10.db"),key(10),p.clone(),20
        ).unwrap();
        assert!(matches!(signer.sign(&different),Err(FenceError::ConflictingRequest)));
        assert!(matches!(examine(&p,&different,&approvals[..q]),
                         Err(ProtocolError::InvalidSignature)));
    }
}

#[test]
fn mismatch_of_policy_epoch_account_checkpoint_and_identity_fails_closed() {
    let p=policy(4,3,1);let r=request(&p);
    let dir=tempfile::tempdir().unwrap();let file=dir.path().join("fence.db");
    {
        let signer=FencedController::open(&file,key(10),p.clone(),20).unwrap();
        assert!(matches!(signer.sign(&Request{expected_version:21,..r.clone()}),
                         Err(FenceError::WrongVersion)));
        assert!(matches!(signer.sign(&Request{epoch:11,..r.clone()}),
                         Err(FenceError::Signing(ProtocolError::WrongPolicy))));
        assert!(matches!(signer.sign(&Request{command:Command::WithdrawFree,..r.clone()}),Ok(_)));
    }
    assert!(matches!(FencedController::open(&file,key(10),p.clone(),21),
                     Err(FenceError::PinnedStateMismatch)));
    assert!(matches!(FencedController::open(&file,key(11),p.clone(),20),
                     Err(FenceError::PinnedStateMismatch)));
    let upgraded=CustodyPolicy{epoch:11,..p.clone()};
    assert!(matches!(FencedController::open(&file,key(10),upgraded,20),
                     Err(FenceError::PinnedStateMismatch)));
    assert!(matches!(FencedController::open(dir.path().join("invalid.db"),
                         key(8),p,20),Err(FenceError::Unenrolled)));
}

#[test]
fn concurrent_threads_obtain_one_effective_signature_and_no_second_request() {
    let p=policy(4,3,1);let r=request(&p);
    let dir=tempfile::tempdir().unwrap();
    let signer=Arc::new(FencedController::open(
        dir.path().join("race.db"),key(10),p,20
    ).unwrap());
    let mut workers=Vec::new();
    for ix in 0..16 {
        let signer=Arc::clone(&signer);
        let mut r=r.clone();
        if ix%2==1 {r.request_id=[77;32];}
        workers.push(thread::spawn(move ||signer.sign(&r)));
    }
    let results:Vec<_>=workers.into_iter().map(|t|t.join().unwrap()).collect();
    let winners:Vec<_>=results.iter().filter_map(|r|r.as_ref().ok()).collect();
    assert!(!winners.is_empty());
    assert!(winners.len()<=8);
    assert!(winners.iter().all(|s|**s==*winners[0]));
    assert_eq!(results.iter().filter(|r|matches!(r,Err(FenceError::ConflictingRequest))).count(),8);
    assert!(signer.has_signed().unwrap());
}

#[test]
fn impossible_policy_and_memory_storage_rejected() {
    let p=policy(4,2,1);
    let dir=tempfile::tempdir().unwrap();
    assert!(matches!(FencedController::open(
        dir.path().join("bad.db"),key(10),p,20
    ),Err(FenceError::Signing(ProtocolError::InvalidPolicy))));
    assert!(matches!(FencedController::open(":memory:",key(10),escrow(),20),
                     Err(FenceError::InvalidPath)));
    assert!(matches!(FencedController::open(
        dir.path().join("zero.db"),key(10),escrow(),0
    ),Err(FenceError::InvalidCheckpoint)));
}
