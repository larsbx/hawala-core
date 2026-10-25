//! M6-C2 experimental ATOMIC DATA MODEL, never a financial authority.
//!
//! This SQLite simulation commits a preview-plan reference, QR nonce,
//! attempt tombstone, and balanced *liability hold* postings in ONE local
//! transaction. Its seed account balances are synthetic! It does NOT verify
//! external custody mandates, asset control, licence, live payment ordering,
//! Iroh quorum, authenticator possession, or external physical payout.
//! DO NOT USE THIS CRATE FOR REAL MONEY OR CASH DEVICE CONTROL.
use std::{collections::BTreeMap, fs::{File,OpenOptions},path::{Path,PathBuf},
          sync::Mutex,time::Duration};
use fs2::FileExt;
use hawala_authority::{PreviewPlan,reserve_event_id};
use hawala_ledger::{Account,Entry,NormalSide};
use rusqlite::{params,Connection,OptionalExtension,TransactionBehavior};
mod admission;
pub use admission::*;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum AccountRole { SyntheticAsset, AvailableLiability, ReservedLiability }
impl AccountRole {
    fn code(self)->u8 {
        match self { Self::SyntheticAsset=>1,Self::AvailableLiability=>2,Self::ReservedLiability=>3 }
    }
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct SyntheticAccount { pub account:Account,pub role:AccountRole,pub balance_minor:u64 }
#[derive(Debug)]
pub enum Error {
    Sql(rusqlite::Error), Io(std::io::Error), Invalid,AlreadySeeded,
    NotSeeded,Conflict,Stale,InvalidEntry,Insufficient,
    StoragePoisoned,
}
impl From<rusqlite::Error> for Error {
    fn from(e:rusqlite::Error)->Self {Self::Sql(e)}
}
impl From<std::io::Error> for Error {
    fn from(e:std::io::Error)->Self {Self::Io(e)}
}
pub type Result<T>=std::result::Result<T,Error>;

/// A synthetic record of a local intent reservation. No operator/ATM may
/// interpret this as an execution permit or proof of customer money.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct SimulatedRecord {
    pub transfer_id:[u8;32], pub attempt_id:[u8;32],
    pub event_id:[u8;32], pub committed_state_version:u64,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Outcome {
    RecordedSimulation(SimulatedRecord),
    IdenticalReplay(SimulatedRecord),
}
pub struct Simulator {
    db:Mutex<Connection>,
    _lock:File,
}
fn i64_amount(n:u64)->Result<i64> {
    i64::try_from(n).map_err(|_|Error::Invalid)
}
fn fixed32(v:Vec<u8>)->Result<[u8;32]> {
    v.try_into().map_err(|_|Error::Invalid)
}
impl Simulator {
    pub fn open(path:impl AsRef<Path>)->Result<Self> {
        let path=path.as_ref();
        if path.as_os_str().is_empty() || path==Path::new(":memory:") {
            return Err(Error::Invalid);
        }
        let mut name=path.as_os_str().to_os_string();
        name.push(".lock");
        let lock=OpenOptions::new().write(true).create(true).open(PathBuf::from(name))?;
        lock.try_lock_exclusive()?;
        let db=Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS sim_state(
                 id INTEGER PRIMARY KEY CHECK(id=1),
                 epoch INTEGER NOT NULL CHECK(epoch>0),
                 version INTEGER NOT NULL CHECK(version>0)
             );
             CREATE TABLE IF NOT EXISTS sim_accounts(
                 id BLOB PRIMARY KEY CHECK(length(id)=32),
                 currency BLOB NOT NULL CHECK(length(currency)=3),
                 normal INTEGER NOT NULL CHECK(normal IN (1,2)),
                 role INTEGER NOT NULL CHECK(role IN (1,2,3)),
                 balance INTEGER NOT NULL CHECK(balance>=0)
             ) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS sim_events(
                 event_id BLOB PRIMARY KEY CHECK(length(event_id)=32),
                 transfer_id BLOB UNIQUE NOT NULL CHECK(length(transfer_id)=32),
                 attempt_id BLOB UNIQUE NOT NULL CHECK(length(attempt_id)=32),
                 nonce BLOB UNIQUE NOT NULL CHECK(length(nonce)=32),
                 plan_digest BLOB NOT NULL CHECK(length(plan_digest)=32),
                 postings_digest BLOB NOT NULL CHECK(length(postings_digest)=32),
                 committed_version INTEGER NOT NULL,
                 state TEXT NOT NULL CHECK(state='reserved_no_execution')
             ) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS sim_postings(
                 event_id BLOB NOT NULL,
                 idx INTEGER NOT NULL,
                 account BLOB NOT NULL,
                 debit INTEGER NOT NULL CHECK(debit>=0),
                 credit INTEGER NOT NULL CHECK(credit>=0),
                 PRIMARY KEY(event_id,idx)
             ) WITHOUT ROWID;",
        )?;
        Ok(Self {db:Mutex::new(db),_lock:lock})
    }

    /// ONLY fixtures. No escrow/bank/ATM provider is involved. A real wallet
    /// MUST NOT populate accounts using this API or treat these as assets.
    pub fn seed_synthetic(
        &self, accounts:&[SyntheticAccount], epoch:u64,version:u64,
    )->Result<()> {
        if epoch==0 || version==0 || accounts.len()<3 {
            return Err(Error::Invalid);
        }
        let mut totals:BTreeMap<[u8;3],(u128,u128)>=BTreeMap::new();
        for a in accounts {
            if a.account.id==[0;32]
                || !a.account.currency.iter().all(u8::is_ascii_uppercase)
                || a.balance_minor>i64::MAX as u64
                || (a.role==AccountRole::SyntheticAsset
                    && a.account.normal!=NormalSide::Debit)
                || (a.role!=AccountRole::SyntheticAsset
                    && a.account.normal!=NormalSide::Credit)
            {return Err(Error::Invalid);}
            let t=totals.entry(a.account.currency).or_insert((0,0));
            match a.account.normal {
                NormalSide::Debit=>t.0+=u128::from(a.balance_minor),
                NormalSide::Credit=>t.1+=u128::from(a.balance_minor),
            }
        }
        if totals.values().any(|(d,c)|d!=c) {return Err(Error::Invalid);}
        let mut db=self.db.lock().map_err(|_|Error::StoragePoisoned)?;
        let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx.query_row("SELECT 1 FROM sim_state WHERE id=1",[],|r|r.get::<_,u8>(0))
            .optional()?.is_some() {return Err(Error::AlreadySeeded);}
        for a in accounts {
            tx.execute("INSERT INTO sim_accounts(id,currency,normal,role,balance)
                VALUES (?1,?2,?3,?4,?5)",params![
                a.account.id.as_slice(),a.account.currency.as_slice(),
                match a.account.normal {NormalSide::Debit=>1,NormalSide::Credit=>2},
                a.role.code(),i64_amount(a.balance_minor)?,
            ])?;
        }
        tx.execute("INSERT INTO sim_state(id,epoch,version) VALUES (1,?1,?2)",
            params![i64_amount(epoch)?,i64_amount(version)?])?;
        tx.commit()?;
        Ok(())
    }

    pub fn balance(&self,id:[u8;32])->Result<Option<u64>> {
        let db=self.db.lock().map_err(|_|Error::StoragePoisoned)?;
        let n:Option<i64>=db.query_row(
            "SELECT balance FROM sim_accounts WHERE id=?1",
            params![id.as_slice()],|r|r.get(0)).optional()?;
        n.map(|n|u64::try_from(n).map_err(|_|Error::Invalid)).transpose()
    }
    pub fn event_count(&self)->Result<u64> {
        let db=self.db.lock().map_err(|_|Error::StoragePoisoned)?;
        let n:i64=db.query_row("SELECT count(*) FROM sim_events",[],|r|r.get(0))?;
        u64::try_from(n).map_err(|_|Error::Invalid)
    }
    pub fn state_version(&self)->Result<u64> {
        let db=self.db.lock().map_err(|_|Error::StoragePoisoned)?;
        let n:i64=db.query_row("SELECT version FROM sim_state WHERE id=1",[],|r|r.get(0))
            .optional()?.ok_or(Error::NotSeeded)?;
        u64::try_from(n).map_err(|_|Error::Invalid)
    }

    /// A single durable SQLite commit combines once-only QR nonce, fixed
    /// attempt ID, constrained *liability-to-liability hold* postings, and
    /// monotonically increasing local state version. Does not dispense cash.
    ///
    /// Consumes a PreviewPlan that can only be returned by the pure preview
    /// verifier, but DOES NOT trust the preview to attest real custody assets.
    /// Cross-device and P2P settlement ordering remain entirely UNSOLVED.
    pub fn record_simulated_hold(
        &self,plan:&PreviewPlan,entry:&Entry
    )->Result<Outcome> {
        let mut db=self.db.lock().map_err(|_|Error::StoragePoisoned)?;
        let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let outcome=Self::record_in_transaction(&tx,plan,entry)?;
        tx.commit()?;
        Ok(outcome)
    }
    fn record_in_transaction(tx:&rusqlite::Transaction<'_>,plan:&PreviewPlan,entry:&Entry)->Result<Outcome> {
        if plan.transfer_id()==[0;32] || plan.attempt_id()==[0;32]
            || plan.nonce()==[0;32] || plan.plan_digest()==[0;32]
            || entry.event_id!=plan.ledger_event_id()
            || entry.event_id!=reserve_event_id(plan.transfer_id())
            || entry.postings.len()!=2 || plan.payout_minor()==0
        {return Err(Error::InvalidEntry);}
        let mut sorted=entry.postings.clone();
        sorted.sort_by_key(|p|p.account);
        if sorted[0].account==sorted[1].account {
            return Err(Error::InvalidEntry);
        }
        let existing:Option<(Vec<u8>,Vec<u8>,Vec<u8>,Vec<u8>,i64)>=tx.query_row(
            "SELECT attempt_id,nonce,plan_digest,postings_digest,committed_version
             FROM sim_events WHERE transfer_id=?1",
            params![plan.transfer_id().as_slice()],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)),
        ).optional()?;

        let mut sums=(0u128,0u128);
        let mut checked=Vec::with_capacity(2);
        let mut previous=blake3::Hasher::new_derive_key("hawala/reference-ledger-before/v1");
        let mut entry_hash=blake3::Hasher::new_derive_key("hawala/synthetic-hold-postings/v1");
        entry_hash.update(&entry.event_id);
        for p in &sorted {
            if (p.debit_minor==0)==(p.credit_minor==0) {
                return Err(Error::InvalidEntry);
            }
            let row:Option<(Vec<u8>,i64,i64,i64)>=tx.query_row(
                "SELECT currency,normal,role,balance FROM sim_accounts WHERE id=?1",
                params![p.account.as_slice()],
                |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
            ).optional()?;
            let (currency,normal,role,balance)=row.ok_or(Error::InvalidEntry)?;
            if currency.as_slice()!=plan.currency().as_slice()
                || normal!=2 || balance<0
                || p.debit_minor>i64::MAX as u64
                || p.credit_minor>i64::MAX as u64
                || (role==2 && (p.debit_minor!=plan.payout_minor() || p.credit_minor!=0))
                || (role==3 && (p.credit_minor!=plan.payout_minor() || p.debit_minor!=0))
                || (role!=2 && role!=3)
            {return Err(Error::InvalidEntry);}
            sums.0+=u128::from(p.debit_minor);sums.1+=u128::from(p.credit_minor);
            let before=u64::try_from(balance).map_err(|_|Error::InvalidEntry)?;
            previous.update(&p.account);
            previous.update(&currency);
            previous.update(&[normal as u8]);
            previous.update(&before.to_be_bytes());
            entry_hash.update(&p.account);
            entry_hash.update(&currency);
            entry_hash.update(&[normal as u8]);
            entry_hash.update(&p.debit_minor.to_be_bytes());
            entry_hash.update(&p.credit_minor.to_be_bytes());
            checked.push((p.account,balance,role,p.debit_minor,p.credit_minor));
        }
        if sums.0!=sums.1 || sums.0!=u128::from(plan.payout_minor()) {
            return Err(Error::InvalidEntry);
        }
        let entry_digest=*entry_hash.finalize().as_bytes();
        if let Some((attempt,nonce,digest,posted,version))=existing {
            if attempt==plan.attempt_id() && nonce==plan.nonce()
                && digest==plan.plan_digest() && posted==entry_digest
            {
                return Ok(Outcome::IdenticalReplay(SimulatedRecord{
                    transfer_id:plan.transfer_id(),attempt_id:plan.attempt_id(),
                    event_id:entry.event_id,
                    committed_state_version:u64::try_from(version).map_err(|_|Error::Invalid)?,
                }));
            }
            return Err(Error::Conflict);
        }
        if *previous.finalize().as_bytes()!=plan.balance_checkpoint() {
            return Err(Error::Stale);
        }
        let (epoch,version):(i64,i64)=tx.query_row(
            "SELECT epoch,version FROM sim_state WHERE id=1",[],
            |r|Ok((r.get(0)?,r.get(1)?)),
        ).optional()?.ok_or(Error::NotSeeded)?;
        if epoch!=i64_amount(plan.authority_epoch())?
            || version!=i64_amount(plan.account_version())?
        {return Err(Error::Stale);}
        let next_version=version.checked_add(1).ok_or(Error::Invalid)?;
        for (account,old,role,debit,credit) in &checked {
            let delta=if *role==2 {
                -(i64_amount(*debit)?)
            } else {
                i64_amount(*credit)?
            };
            let new=old.checked_add(delta).filter(|n|*n>=0).ok_or(Error::Insufficient)?;
            let changed=tx.execute(
                "UPDATE sim_accounts SET balance=?1 WHERE id=?2 AND balance=?3",
                params![new,account.as_slice(),old],
            )?;
            if changed!=1{return Err(Error::Stale);}
        }
        let changed=tx.execute(
            "UPDATE sim_state SET version=?1 WHERE id=1 AND epoch=?2 AND version=?3",
            params![next_version,epoch,version],
        )?;
        if changed!=1{return Err(Error::Stale);}
        tx.execute(
            "INSERT INTO sim_events(event_id,transfer_id,attempt_id,nonce,
                plan_digest,postings_digest,committed_version,state)
                VALUES (?1,?2,?3,?4,?5,?6,?7,'reserved_no_execution')",
            params![entry.event_id.as_slice(),plan.transfer_id().as_slice(),
                plan.attempt_id().as_slice(),plan.nonce().as_slice(),
                plan.plan_digest().as_slice(),entry_digest.as_slice(),next_version],
        )?;
        for (idx,p) in sorted.iter().enumerate() {
            tx.execute("INSERT INTO sim_postings(event_id,idx,account,debit,credit)
                VALUES (?1,?2,?3,?4,?5)",params![
                entry.event_id.as_slice(),idx as i64,p.account.as_slice(),
                i64_amount(p.debit_minor)?,i64_amount(p.credit_minor)?,
            ])?;
        }
        Ok(Outcome::RecordedSimulation(SimulatedRecord{
            transfer_id:plan.transfer_id(),attempt_id:plan.attempt_id(),
            event_id:entry.event_id,
            committed_state_version:u64::try_from(next_version).map_err(|_|Error::Invalid)?,
        }))
    }
}
