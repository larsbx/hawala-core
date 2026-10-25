use ed25519_dalek::SigningKey;
use hawala_custody_model::{
    sign_request,examine,Approval,Command,CustodyPolicy,Decision,Error,Mode,Request
};
use hawala_identity_core::IdentityId;

fn key(n:u8)->SigningKey {SigningKey::from_bytes(&[n;32])}
fn id(n:u8)->IdentityId {IdentityId::from_signing_key(&key(n))}
fn escrow()->CustodyPolicy {
    CustodyPolicy{
        network_id:[1;32],custody_account:[2;32],depositing_dar:id(4),
        currency:*b"EUR",mandate_digest:[3;32],epoch:10,
        mode:Mode::IndependentEscrow{controller:id(10)},
    }
}
fn federation(n:usize,threshold:usize,faults:usize)->CustodyPolicy {
    CustodyPolicy{
        mode:Mode::Federated{
            controllers:(0..n).map(|i|id(10+i as u8)).collect(),
            threshold,max_faults:faults,
        },
        ..escrow()
    }
}
fn request(policy:&CustodyPolicy)->Request {
    Request{
        network_id:policy.network_id,custody_account:policy.custody_account,
        depositing_dar:policy.depositing_dar,currency:policy.currency,
        mandate_digest:policy.mandate_digest,
        policy_digest:policy.digest().unwrap(),
        epoch:policy.epoch,expected_version:20,request_id:[5;32],
        allocation_id:[6;32],obligation_digest:[7;32],
        command:Command::Reserve,amount_minor:900,
    }
}
#[test]
fn escrow_requires_the_one_pinned_controller_and_exact_request(){
    let p=escrow();let r=request(&p);
    assert_eq!(examine(&p,&r,&[]),Ok(Decision::Hold{got:0,needed:1}));
    let a=sign_request(&p,&r,&key(10)).unwrap();
    assert!(matches!(examine(&p,&r,&[a]),Ok(Decision::SufficientSignatures(_))));
    assert_eq!(sign_request(&p,&r,&key(11)),Err(Error::UnenrolledSigner));
    let mut changed=r.clone();changed.amount_minor=901;
    assert_eq!(examine(&p,&changed,&[a]),Err(Error::InvalidSignature));
    let mut changed=r.clone();changed.expected_version=21;
    assert_eq!(examine(&p,&changed,&[a]),Err(Error::InvalidSignature));
    let mut changed=r.clone();changed.epoch=11;
    assert_eq!(examine(&p,&changed,&[a]),Err(Error::WrongPolicy));
}
#[test]
fn four_controller_federation_requires_three_and_defers_partition(){
    let p=federation(4,3,1);let r=request(&p);
    let s:Vec<Approval>=(10..14).map(|n|sign_request(&p,&r,&key(n)).unwrap()).collect();
    assert_eq!(examine(&p,&r,&s[..2]),Ok(Decision::Hold{got:2,needed:3}));
    let enough=examine(&p,&r,&s[..3]).unwrap();
    assert!(matches!(enough,Decision::SufficientSignatures(_)));
    assert_eq!(examine(&p,&r,&[s[0],s[0],s[1]]),
        Err(Error::DuplicateSigner));
    assert_eq!(examine(&p,&r,&[s[0],s[1],Approval{signer:id(99),signature:s[2].signature}]),
        Err(Error::UnenrolledSigner));
    let mut altered=s[2];altered.signature[4]^=1;
    assert_eq!(examine(&p,&r,&[s[0],s[1],altered]),
        Err(Error::InvalidSignature));
}
#[test]
fn eight_controller_federation_requires_fault_intersecting_threshold(){
    let p=federation(8,6,2);let r=request(&p);
    let s:Vec<Approval>=(10..18).map(|n|sign_request(&p,&r,&key(n)).unwrap()).collect();
    assert_eq!(examine(&p,&r,&s[..5]),Ok(Decision::Hold{got:5,needed:6}));
    assert!(matches!(examine(&p,&r,&s[..6]),Ok(Decision::SufficientSignatures(_))));
    let bad=federation(8,5,2);
    assert_eq!(bad.validate(),Err(Error::InvalidPolicy));
    assert_eq!(federation(4,3,2).validate(),Err(Error::InvalidPolicy));
}
#[test]
fn cannot_rewrite_policy_membership_mandate_currency_or_action(){
    let p=federation(4,3,1);let r=request(&p);
    let approvals:Vec<Approval>=(10..13).map(|n|sign_request(&p,&r,&key(n)).unwrap()).collect();
    let mut changed=r.clone();changed.command=Command::WithdrawFree;
    assert_eq!(examine(&p,&changed,&approvals),Err(Error::InvalidSignature));
    let mut changed=r.clone();changed.currency=*b"USD";
    assert_eq!(examine(&p,&changed,&approvals),Err(Error::WrongPolicy));
    let mut changed=r.clone();changed.mandate_digest=[15;32];
    assert_eq!(examine(&p,&changed,&approvals),Err(Error::WrongPolicy));
    let changed_policy=CustodyPolicy{epoch:11,..p.clone()};
    assert_eq!(examine(&changed_policy,&r,&approvals),Err(Error::WrongPolicy));
    let mut reord=p.clone();
    if let Mode::Federated{ref mut controllers,..}=reord.mode {controllers.reverse();}
    assert_eq!(p.digest(),reord.digest());
}
#[test]
fn unsafe_or_self_signed_custody_policy_is_rejected(){
    let p=CustodyPolicy{
        mode:Mode::IndependentEscrow{controller:id(4)},..escrow()
    };
    assert_eq!(p.validate(),Err(Error::InvalidPolicy));
    let p=CustodyPolicy{
        mode:Mode::Federated{controllers:vec![id(10),id(10),id(11)],
                              threshold:2,max_faults:0},
        ..escrow()
    };
    assert_eq!(p.validate(),Err(Error::InvalidPolicy));
}
