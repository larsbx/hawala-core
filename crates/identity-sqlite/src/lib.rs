//! Reusable crash-durable ReplayGuard for an identity verifier.
//!
//! A local SQLite file provides atomic nonce consumption across processes
//! using the SAME database. It is not a distributed quorum, cross-device
//! consensus or an application financial execution authority.
//! DB failures MUST fail closed instead of treating nonces as unused.
use std::{path::Path, sync::Mutex, time::Duration};

use hawala_identity_core::{Error as IdentityError, IdentityId, ReplayGuard};
use rusqlite::{params, Connection, OpenFlags};

#[derive(Debug)]
pub enum OpenError {
    EmptyPath,
    InMemoryNotDurable,
    Database(rusqlite::Error),
}
impl From<rusqlite::Error> for OpenError {
    fn from(e: rusqlite::Error) -> Self { Self::Database(e) }
}

pub struct SqliteReplayGuard {
    db: Mutex<Connection>,
}

impl SqliteReplayGuard {
    /// Opens a FILE-backed SQLite DB. Use a path restricted to the verifier
    /// service, never a URI or memory DB. Protect, back up and encrypt disk as
    /// appropriate; do not ever copy the DB to create parallel authorities.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, OpenError> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() { return Err(OpenError::EmptyPath); }
        if path == Path::new(":memory:") {
            return Err(OpenError::InMemoryNotDurable);
        }
        let conn = Connection::open_with_flags(
            path, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS consumed_nonces (
                verifier BLOB NOT NULL CHECK(length(verifier) = 32),
                claimant BLOB NOT NULL CHECK(length(claimant) = 32),
                nonce BLOB NOT NULL CHECK(length(nonce) = 32),
                PRIMARY KEY (verifier, claimant, nonce)
             ) WITHOUT ROWID;",
        )?;
        Ok(Self { db: Mutex::new(conn) })
    }

    pub fn count(&self) -> Result<u64, IdentityError> {
        let db = self.db.lock().map_err(|_| IdentityError::Storage)?;
        let count: i64 = db.query_row("SELECT count(*) FROM consumed_nonces", [], |r| r.get(0))
            .map_err(|_| IdentityError::Storage)?;
        u64::try_from(count).map_err(|_| IdentityError::Storage)
    }
}

impl ReplayGuard for SqliteReplayGuard {
    /// SQLite's unique primary key plus a fully committed single INSERT
    /// makes the single authority's decision atomic and crash durable.
    /// Tombstones are deliberately NOT pruned without an explicit verified
    /// retention policy: replays stay rejected after process restarts.
    fn consume_once(
        &self,
        verifier: IdentityId,
        claimant: IdentityId,
        nonce: [u8; 32],
    ) -> hawala_identity_core::Result<bool> {
        if nonce == [0; 32] { return Err(IdentityError::InvalidInput); }
        let db = self.db.lock().map_err(|_| IdentityError::Storage)?;
        let inserted = db.execute(
            "INSERT INTO consumed_nonces(verifier, claimant, nonce)
             VALUES (?1, ?2, ?3) ON CONFLICT(verifier, claimant, nonce) DO NOTHING",
            params![verifier.as_bytes().as_slice(),
                    claimant.as_bytes().as_slice(),
                    nonce.as_slice()],
        ).map_err(|_| IdentityError::Storage)?;
        Ok(inserted == 1)
    }
}
