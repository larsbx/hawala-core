//! Durable *intent* journal for bilateral Hawala transfers.
//! This is not a double-entry ledger, authority for cash, or a dispenser API.
//! It never commands hardware or automatically retries a payout.
use std::{fs::{File, OpenOptions}, path::{Path, PathBuf}, sync::Mutex, time::Duration};
use fs2::FileExt;

use hawala_protocol::{ObligationEvidence, SignedTerms};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

#[derive(Debug)]
pub enum JournalError {
    Database(rusqlite::Error),
    Io(std::io::Error),
    StoragePoisoned,
    InvalidPath,
    InvalidInput,
    Conflict,
    UnknownTransfer,
    InvalidTransition,
}
impl From<std::io::Error> for JournalError {
    fn from(value: std::io::Error) -> Self { Self::Io(value) }
}
impl From<rusqlite::Error> for JournalError {
    fn from(value: rusqlite::Error) -> Self { Self::Database(value) }
}
pub type Result<T> = std::result::Result<T, JournalError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Registered,
    Dispensing,
    Uncertain,
    PayoutAttested,
    Cancelled,
}
impl Phase {
    fn parse(s: &str) -> Result<Self> {
        match s {
            "registered" => Ok(Self::Registered),
            "dispensing" => Ok(Self::Dispensing),
            "uncertain" => Ok(Self::Uncertain),
            "payout_attested" => Ok(Self::PayoutAttested),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(JournalError::InvalidInput),
        }
    }
}

/// An isolated local authority, NOT distributed consensus. Files must remain
/// under the exclusive operational control of the responsible payout node.
/// Sharing a journal via an unreliable network filesystem is forbidden.
pub struct Journal {
    // Connection must be dropped before releasing the operator lock.
    db: Mutex<Connection>,
    _lock: File,
    recovered_uncertain: usize,
}

fn append_event(tx: &Transaction<'_>, id: &[u8; 32], kind: &str) -> Result<()> {
    tx.execute("INSERT INTO events(transfer_id, kind) VALUES (?1, ?2)",
        params![id.as_slice(), kind])?;
    Ok(())
}

impl Journal {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() || path == Path::new(":memory:") {
            return Err(JournalError::InvalidPath);
        }
        // Use a sibling lock file, and hold its exclusive lock for the
        // entire lifetime. Another process must not race a live cash attempt.
        let mut lock_os = path.as_os_str().to_os_string();
        lock_os.push(".lock");
        let lock_path = PathBuf::from(lock_os);
        let lock = OpenOptions::new().create(true).write(true).open(lock_path)?;
        lock.try_lock_exclusive()?;
        let mut db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS obligations(
                 transfer_id BLOB PRIMARY KEY CHECK(length(transfer_id) = 32),
                 terms_digest BLOB NOT NULL CHECK(length(terms_digest) = 32),
                 payout_minor INTEGER NOT NULL CHECK(payout_minor > 0),
                 payout_currency BLOB NOT NULL CHECK(length(payout_currency) = 3),
                 phase TEXT NOT NULL CHECK(phase IN (
                   'registered', 'dispensing', 'uncertain', 'payout_attested', 'cancelled'
                 )),
                 attempt_id BLOB UNIQUE CHECK(attempt_id IS NULL OR length(attempt_id) = 32)
             ) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS events(
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 transfer_id BLOB NOT NULL,
                 kind TEXT NOT NULL
             );",
        )?;
        // A crash between hardware initiation and result persistence is
        // conservatively UNKNOWN. Never reissue the cash request on restart.
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO events(transfer_id, kind)
             SELECT transfer_id, 'crash_recovery_uncertain' FROM obligations
             WHERE phase = 'dispensing'", [],
        )?;
        let recovered_uncertain = tx.execute(
            "UPDATE obligations SET phase = 'uncertain' WHERE phase = 'dispensing'", [],
        )?;
        tx.commit()?;
        Ok(Self { db: Mutex::new(db), _lock: lock, recovered_uncertain })
    }

    pub fn recovered_uncertain(&self) -> usize { self.recovered_uncertain }

    /// Records a previously dual-signed AcceptedTerms, without reserving
    /// money or checking any current operating permissions. Terms are
    /// authenticated by their private-constructor SignedTerms type.
    pub fn register(&self, terms: &SignedTerms) -> Result<bool> {
        let t = terms.terms();
        if t.payout_minor > i64::MAX as u64 { return Err(JournalError::InvalidInput); }
        let mut db = self.db.lock().map_err(|_| JournalError::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "INSERT INTO obligations
             (transfer_id, terms_digest, payout_minor, payout_currency, phase)
             VALUES (?1, ?2, ?3, ?4, 'registered')
             ON CONFLICT(transfer_id) DO NOTHING",
            params![t.transfer_id.as_slice(), t.digest().as_slice(),
                    t.payout_minor as i64, t.payout_currency.as_slice()],
        )?;
        let existing: Vec<u8> = tx.query_row(
            "SELECT terms_digest FROM obligations WHERE transfer_id = ?1",
            params![t.transfer_id.as_slice()], |row| row.get(0),
        )?;
        if existing != t.digest().to_vec() {
            return Err(JournalError::Conflict);
        }
        if changed == 1 { append_event(&tx, &t.transfer_id, "registered")?; }
        tx.commit()?;
        Ok(changed == 1)
    }

    pub fn status(&self, transfer_id: [u8; 32]) -> Result<Option<Phase>> {
        let db = self.db.lock().map_err(|_| JournalError::StoragePoisoned)?;
        let state: Option<String> = db.query_row(
            "SELECT phase FROM obligations WHERE transfer_id = ?1",
            params![transfer_id.as_slice()], |row| row.get(0),
        ).optional()?;
        state.map(|s| Phase::parse(&s)).transpose()
    }

    /// Performs a ONE-WAY durable transition before contacting a dispenser.
    /// Only a caller that separately passed Hawala compliance, trust, and
    /// authorized custody checks may execute this. This API does NOT dispense.
    /// Repeated invocations MUST NOT produce another attempt token.
    pub fn begin_attempt(&self, transfer_id: [u8; 32], attempt_id: [u8; 32]) -> Result<()> {
        if transfer_id == [0; 32] || attempt_id == [0; 32] {
            return Err(JournalError::InvalidInput);
        }
        let mut db = self.db.lock().map_err(|_| JournalError::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE obligations SET phase = 'dispensing', attempt_id = ?2
             WHERE transfer_id = ?1 AND phase = 'registered'",
            params![transfer_id.as_slice(), attempt_id.as_slice()],
        )?;
        if changed != 1 { return Err(JournalError::InvalidTransition); }
        append_event(&tx, &transfer_id, "dispensing")?;
        tx.commit()?;
        Ok(())
    }

    /// A dual-signed payout receipt is only ATTESTED, never a proof of cash
    /// movement or unconditional finality. Does not initiate payment or
    /// release a collateral/custody claim.
    pub fn note_payout_attestation(
        &self,
        evidence: &ObligationEvidence,
    ) -> Result<()> {
        if !evidence.payout_attested() { return Err(JournalError::InvalidInput); }
        let terms = evidence.accepted().terms();
        let mut db = self.db.lock().map_err(|_| JournalError::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE obligations SET phase = 'payout_attested'
             WHERE transfer_id = ?1 AND terms_digest = ?2
               AND phase IN ('dispensing','uncertain')",
            params![terms.transfer_id.as_slice(), terms.digest().as_slice()],
        )?;
        if changed != 1 { return Err(JournalError::InvalidTransition); }
        append_event(&tx, &terms.transfer_id, "payout_attested")?;
        tx.commit()?;
        Ok(())
    }

    /// A reservation can be abandoned only if NO dispensing attempt was
    /// durably initiated; uncertain executions cannot be cancelled.
    pub fn cancel_unstarted(&self, transfer_id: [u8; 32]) -> Result<()> {
        let mut db = self.db.lock().map_err(|_| JournalError::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE obligations SET phase = 'cancelled'
             WHERE transfer_id = ?1 AND phase = 'registered'",
            params![transfer_id.as_slice()],
        )?;
        if changed != 1 { return Err(JournalError::InvalidTransition); }
        append_event(&tx, &transfer_id, "cancelled")?;
        tx.commit()?;
        Ok(())
    }

    pub fn event_count(&self, transfer_id: [u8; 32]) -> Result<u64> {
        let db = self.db.lock().map_err(|_| JournalError::StoragePoisoned)?;
        let n: i64 = db.query_row(
            "SELECT count(*) FROM events WHERE transfer_id = ?1",
            params![transfer_id.as_slice()], |row| row.get(0),
        )?;
        u64::try_from(n).map_err(|_| JournalError::InvalidInput)
    }
}
