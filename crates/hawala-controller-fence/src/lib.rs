//! Durable *local controller* anti-equivocation for simulated Hawala custody.
//!
//! This crate NEVER controls an asset, disburses money, enforces external
//! custody, runs consensus or proves legal ownership. It only keeps a
//! controller from signing TWO DIFFERENT requests at the SAME account version,
//! assuming its signing key and SQLite storage have not been copied, rolled
//! back, compromised, or used outside this fenced API.
//!
//! One database = one controller identity + one policy/account epoch +
//! one externally VERIFIED account checkpoint. Refuse version/epoch upgrades
//! until real custody state continuity and controller succession exist.
use std::{fs::{File, OpenOptions}, path::{Path, PathBuf}, sync::Mutex, time::Duration};
use ed25519_dalek::SigningKey;
use fs2::FileExt;
use hawala_custody_model::{
    sign_request, Approval, CustodyPolicy, Error as ProtocolError, Mode, Request,
};
use hawala_identity_core::IdentityId;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

#[derive(Debug)]
pub enum FenceError {
    InvalidPath,
    InvalidCheckpoint,
    Unenrolled,
    PinnedStateMismatch,
    ConflictingRequest,
    WrongVersion,
    CorruptHistory,
    Signing(ProtocolError),
    Database(rusqlite::Error),
    Io(std::io::Error),
    LockPoisoned,
}
impl From<rusqlite::Error> for FenceError {
    fn from(e:rusqlite::Error) -> Self { Self::Database(e) }
}
impl From<std::io::Error> for FenceError {
    fn from(e:std::io::Error) -> Self { Self::Io(e) }
}
pub type Result<T> = std::result::Result<T,FenceError>;

/// Holds an exclusive lock across the entire signing-key lifetime.
/// Reusing the same signing key with an independently copied database is
/// prohibited: that is outside the fault model and defeats this fence.
pub struct FencedController {
    db: Mutex<Connection>,
    _lock: File,
    key: SigningKey,
    policy: CustodyPolicy,
    digest: [u8;32],
    controller: IdentityId,
    checkpoint_version: u64,
}

fn valid_member(policy:&CustodyPolicy, controller:IdentityId) -> bool {
    match &policy.mode {
        Mode::IndependentEscrow { controller: enrolled } => *enrolled == controller,
        Mode::Federated { controllers, .. } => controllers.contains(&controller),
    }
}
fn signed_bytes(v:Vec<u8>) -> Result<[u8;64]> {
    v.try_into().map_err(|_|FenceError::CorruptHistory)
}
fn valid_version(n:u64) -> Result<i64> {
    if n==0 {return Err(FenceError::InvalidCheckpoint);}
    i64::try_from(n).map_err(|_|FenceError::InvalidCheckpoint)
}
impl FencedController {
    /// `checkpoint_version` MUST be established from an authoritative and
    /// independently authenticated custody snapshot before provisioning.
    /// This library cannot prove it, and will not advance it automatically.
    pub fn open(
        path:impl AsRef<Path>,
        key:SigningKey,
        policy:CustodyPolicy,
        checkpoint_version:u64,
    ) -> Result<Self> {
        let path=path.as_ref();
        if path.as_os_str().is_empty() || path==Path::new(":memory:") {
            return Err(FenceError::InvalidPath);
        }
        let checked_version=valid_version(checkpoint_version)?;
        let digest=policy.digest().map_err(FenceError::Signing)?;
        let controller=IdentityId::from_signing_key(&key);
        if !valid_member(&policy,controller) {
            return Err(FenceError::Unenrolled);
        }
        let mut name=path.as_os_str().to_os_string();
        name.push(".lock");
        let lock=OpenOptions::new().write(true).create(true)
            .open(PathBuf::from(name))?;
        lock.try_lock_exclusive()?;
        let db=Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS controller_checkpoint(
                 id INTEGER PRIMARY KEY CHECK(id=1),
                 policy_digest BLOB NOT NULL CHECK(length(policy_digest)=32),
                 controller BLOB NOT NULL CHECK(length(controller)=32),
                 epoch INTEGER NOT NULL CHECK(epoch>0),
                 version INTEGER NOT NULL CHECK(version>0)
             );
             CREATE TABLE IF NOT EXISTS signed_request(
                 id INTEGER PRIMARY KEY CHECK(id=1),
                 request_digest BLOB NOT NULL CHECK(length(request_digest)=32),
                 signature BLOB NOT NULL CHECK(length(signature)=64)
             );"
        )?;
        // The lock makes initialization/retained-state inspection exclusive.
        db.execute(
            "INSERT INTO controller_checkpoint
             (id,policy_digest,controller,epoch,version)
             VALUES(1,?1,?2,?3,?4) ON CONFLICT(id) DO NOTHING",
            params![
                digest.as_slice(),controller.as_bytes().as_slice(),
                valid_version(policy.epoch)?,checked_version
            ],
        )?;
        let saved:(Vec<u8>,Vec<u8>,i64,i64)=db.query_row(
            "SELECT policy_digest,controller,epoch,version
             FROM controller_checkpoint WHERE id=1",
            [],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))
        )?;
        if saved.0.as_slice()!=digest || saved.1.as_slice()!=controller.as_bytes()
            || saved.2!=valid_version(policy.epoch)? || saved.3!=checked_version
        {return Err(FenceError::PinnedStateMismatch);}
        Ok(Self{
            db:Mutex::new(db),_lock:lock,key,policy,digest,
            controller,checkpoint_version,
        })
    }
    pub fn controller(&self) -> IdentityId {self.controller}
    pub fn policy_digest(&self) -> [u8;32] {self.digest}
    pub fn checkpoint_version(&self) -> u64 {self.checkpoint_version}

    /// Sign at most one *distinct* request for the verified checkpoint.
    /// Identical replays return the same stored signature. Conflicting
    /// requests cannot obtain another signature after crash/restart.
    /// This is NOT a custody effect; no reserve/release command is executed.
    pub fn sign(&self,request:&Request) -> Result<Approval> {
        if request.expected_version!=self.checkpoint_version {
            return Err(FenceError::WrongVersion);
        }
        let candidate=sign_request(&self.policy,request,&self.key)
            .map_err(FenceError::Signing)?;
        let mut db=self.db.lock().map_err(|_|FenceError::LockPoisoned)?;
        let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let saved:(Vec<u8>,Vec<u8>,i64,i64)=tx.query_row(
            "SELECT policy_digest,controller,epoch,version
             FROM controller_checkpoint WHERE id=1",
            [],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))
        )?;
        if saved.0.as_slice()!=self.digest
            || saved.1.as_slice()!=self.controller.as_bytes()
            || saved.2!=valid_version(self.policy.epoch)?
            || saved.3!=valid_version(self.checkpoint_version)?
        {return Err(FenceError::PinnedStateMismatch);}
        let prior:Option<(Vec<u8>,Vec<u8>)>=tx.query_row(
            "SELECT request_digest,signature FROM signed_request WHERE id=1",
            [],|r|Ok((r.get(0)?,r.get(1)?))
        ).optional()?;
        let digest=request.digest();
        if let Some((old_digest,old_signature))=prior {
            if old_digest.as_slice()!=digest {
                return Err(FenceError::ConflictingRequest);
            }
            let old_signature=signed_bytes(old_signature)?;
            if old_signature!=candidate.signature {
                return Err(FenceError::CorruptHistory);
            }
            return Ok(Approval{signer:self.controller,signature:old_signature});
        }
        tx.execute(
            "INSERT INTO signed_request(id,request_digest,signature)
             VALUES(1,?1,?2)",
            params![digest.as_slice(),candidate.signature.as_slice()]
        )?;
        tx.commit()?;
        Ok(candidate)
    }
    pub fn has_signed(&self) -> Result<bool> {
        let db=self.db.lock().map_err(|_|FenceError::LockPoisoned)?;
        let exists:bool=db.query_row(
            "SELECT EXISTS(SELECT 1 FROM signed_request)",
            [],|r|r.get(0)
        )?;
        Ok(exists)
    }
}
