//! M6-C3: escrow/federation signature-policy **simulation**, not asset control.
//!
//! The source of policy, mandate, controller independence, externally held
//! assets, legal authority, and anti-equivocation ordering is OUTSIDE this
//! module. Even a passing quorum provides signed claims, not bank settlement,
//! custody finality, real collateral or permission to dispense cash.
use std::collections::BTreeSet;
use ed25519_dalek::{Signature,Signer,SigningKey,VerifyingKey};
use hawala_identity_core::IdentityId;

#[derive(Clone,Debug,PartialEq,Eq)]
pub enum Mode {
    IndependentEscrow { controller:IdentityId },
    Federated {
        /// Exact enrolled roster, anchored by an independently agreed mandate.
        controllers:Vec<IdentityId>,
        threshold:usize,
        /// Maximum colluding Byzantine controllers in the claimed fault model.
        max_faults:usize,
    },
}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct CustodyPolicy {
    pub network_id:[u8;32],
    pub custody_account:[u8;32],
    pub depositing_dar:IdentityId,
    pub currency:[u8;3],
    pub mandate_digest:[u8;32],
    pub epoch:u64,
    pub mode:Mode,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Command { Reserve, Release, WithdrawFree, Query }
impl Command {
    fn tag(self)->u8 {
        match self {
            Self::Reserve=>1,Self::Release=>2,
            Self::WithdrawFree=>3,Self::Query=>4,
        }
    }
}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct Request {
    pub network_id:[u8;32],
    pub custody_account:[u8;32],
    pub depositing_dar:IdentityId,
    pub currency:[u8;3],
    pub mandate_digest:[u8;32],
    pub policy_digest:[u8;32],
    pub epoch:u64,
    pub expected_version:u64,
    pub request_id:[u8;32],
    pub allocation_id:[u8;32],
    pub obligation_digest:[u8;32],
    pub command:Command,
    pub amount_minor:u64,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Error {
    InvalidPolicy, WrongPolicy, InvalidRequest,
    UnenrolledSigner, DuplicateSigner, InvalidSignature,
}
pub type Result<T>=std::result::Result<T,Error>;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Approval {
    pub signer:IdentityId,
    pub signature:[u8;64],
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct SignerEvidence {
    pub verified_signers:usize,
    pub policy_digest:[u8;32],
    pub request_digest:[u8;32],
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Decision {
    /// All required controller signatures for *one specific request* exist.
    /// This is NOT evidence that the custodian performed the operation.
    SufficientSignatures(SignerEvidence),
    /// A partition or missing controller response must not be treated
    /// as a confirmed custody change.
    Hold {got:usize,needed:usize},
}
fn len_prefixed(out:&mut Vec<u8>,part:&[u8]) {
    out.extend_from_slice(&(part.len() as u32).to_be_bytes());
    out.extend_from_slice(part);
}
impl CustodyPolicy {
    pub fn validate(&self)->Result<()> {
        if self.network_id==[0;32] || self.custody_account==[0;32]
            || self.mandate_digest==[0;32] || self.epoch==0
            || !self.currency.iter().all(u8::is_ascii_uppercase)
        {return Err(Error::InvalidPolicy);}
        let mut unique=BTreeSet::new();
        let (keys,q,f)=match &self.mode {
            Mode::IndependentEscrow{controller} => {
                if *controller==self.depositing_dar {
                    return Err(Error::InvalidPolicy);
                }
                (vec![*controller],1usize,0usize)
            }
            Mode::Federated{controllers,threshold,max_faults} => {
                (controllers.clone(),*threshold,*max_faults)
            }
        };
        if keys.is_empty() || keys.len()>64
            || q==0 || q>keys.len()
            || f>=keys.len()
            || q<=f
        {return Err(Error::InvalidPolicy);}
        for k in &keys {
            if *k==self.depositing_dar || !unique.insert(*k) {
                return Err(Error::InvalidPolicy);
            }
        }
        // Fault-model claim: any two accepted q-subsets intersect in > f
        // controllers, so they share at least one correct signer IF at most f
        // are corrupt and honest members never sign conflicting state steps.
        // This arithmetic alone is NOT a working consensus implementation.
        if matches!(&self.mode,Mode::Federated{..}) {
            if keys.len()<f.saturating_mul(3).saturating_add(1)
                || q.saturating_mul(2)<=keys.len().saturating_add(f)
            {
                return Err(Error::InvalidPolicy);
            }
        }
        Ok(())
    }
    fn members(&self)->Vec<IdentityId> {
        let mut xs=match &self.mode {
            Mode::IndependentEscrow{controller}=>vec![*controller],
            Mode::Federated{controllers,..}=>controllers.clone(),
        };
        xs.sort();
        xs
    }
    fn quorum(&self)->usize {
        match &self.mode {
            Mode::IndependentEscrow{..}=>1,
            Mode::Federated{threshold,..}=>*threshold,
        }
    }
    pub fn digest(&self)->Result<[u8;32]> {
        self.validate()?;
        let mut out=b"hawala/custody-policy/v1\0".to_vec();
        let epoch = self.epoch.to_be_bytes();
        for part in [
            self.network_id.as_slice(),self.custody_account.as_slice(),
            self.depositing_dar.as_bytes().as_slice(),
            self.currency.as_slice(),self.mandate_digest.as_slice(),
            epoch.as_slice(),
        ] {len_prefixed(&mut out,part);}
        match &self.mode {
            Mode::IndependentEscrow{..}=>out.push(1),
            Mode::Federated{threshold,max_faults,..}=>{
                out.push(2);
                out.extend_from_slice(&(*threshold as u64).to_be_bytes());
                out.extend_from_slice(&(*max_faults as u64).to_be_bytes());
            }
        }
        for key in self.members(){len_prefixed(&mut out,key.as_bytes());}
        Ok(*blake3::hash(&out).as_bytes())
    }
}
impl Request {
    fn validate(&self,policy:&CustodyPolicy)->Result<()> {
        policy.validate()?;
        if self.network_id!=policy.network_id
            || self.custody_account!=policy.custody_account
            || self.depositing_dar!=policy.depositing_dar
            || self.currency!=policy.currency
            || self.mandate_digest!=policy.mandate_digest
            || self.epoch!=policy.epoch
            || self.policy_digest!=policy.digest()?
        {return Err(Error::WrongPolicy);}
        if self.expected_version==0 || self.request_id==[0;32]
            || self.allocation_id==[0;32] || self.obligation_digest==[0;32]
            || self.amount_minor==0 || self.amount_minor>i64::MAX as u64
        {return Err(Error::InvalidRequest);}
        Ok(())
    }
    fn statement(&self)->Vec<u8> {
        let mut out=b"hawala/custody-command/v1\0".to_vec();
        let command=[self.command.tag()];
        let epoch=self.epoch.to_be_bytes();
        let version=self.expected_version.to_be_bytes();
        let amount=self.amount_minor.to_be_bytes();
        for p in [
            self.network_id.as_slice(),self.custody_account.as_slice(),
            self.depositing_dar.as_bytes().as_slice(),
            self.currency.as_slice(),self.mandate_digest.as_slice(),
            self.policy_digest.as_slice(),epoch.as_slice(),version.as_slice(),
            self.request_id.as_slice(),self.allocation_id.as_slice(),
            self.obligation_digest.as_slice(),command.as_slice(),amount.as_slice(),
        ] {len_prefixed(&mut out,p);}
        out
    }
    pub fn digest(&self)->[u8;32] {*blake3::hash(&self.statement()).as_bytes()}
}
pub fn sign_request(policy:&CustodyPolicy,request:&Request,key:&SigningKey)
    ->Result<Approval> {
    request.validate(policy)?;
    let signer=IdentityId::from_signing_key(key);
    if !policy.members().contains(&signer){return Err(Error::UnenrolledSigner);}
    Ok(Approval{signer,signature:key.sign(&request.statement()).to_bytes()})
}
/// Validates *all* provided signatures, even if enough valid ones appear
/// earlier. Rejects duplicate controller keys and changed request bodies.
pub fn examine(policy:&CustodyPolicy,request:&Request,approvals:&[Approval])
    ->Result<Decision> {
    request.validate(policy)?;
    let allowed=policy.members();
    let mut seen=BTreeSet::new();
    for a in approvals {
        if !allowed.contains(&a.signer){return Err(Error::UnenrolledSigner);}
        if !seen.insert(a.signer){return Err(Error::DuplicateSigner);}
        let verifying=VerifyingKey::from_bytes(a.signer.as_bytes())
            .map_err(|_|Error::InvalidSignature)?;
        verifying.verify_strict(
            &request.statement(),&Signature::from_bytes(&a.signature)
        ).map_err(|_|Error::InvalidSignature)?;
    }
    let q=policy.quorum();
    if seen.len()<q {
        Ok(Decision::Hold{got:seen.len(),needed:q})
    } else {
        Ok(Decision::SufficientSignatures(SignerEvidence{
            verified_signers:seen.len(),policy_digest:policy.digest()?,
            request_digest:request.digest(),
        }))
    }
}
