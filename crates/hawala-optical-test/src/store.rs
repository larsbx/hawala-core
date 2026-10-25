use crate::{Envelope, Error, Kind, Result, MAX_SESSION_SECS};
use ed25519_dalek::SigningKey;
use hawala_identity_core::IdentityId;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::{path::Path, time::Duration};

/// Independently provisioned TEST policy; not supplied by QR or camera.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TestPolicy {
    pub network: [u8; 32],
    pub terminal: IdentityId,
    pub authority: IdentityId,
    pub epoch: u64,
}
impl TestPolicy {
    fn bytes(self) -> Result<Vec<u8>> {
        if self.network == [0; 32] || self.epoch == 0 || self.terminal == self.authority {
            return Err(Error::Invalid);
        }
        let mut b = self.network.to_vec();
        b.extend_from_slice(self.terminal.as_bytes());
        b.extend_from_slice(self.authority.as_bytes());
        b.extend_from_slice(&self.epoch.to_be_bytes());
        Ok(b)
    }
}
/// Every successful response is permanently HELD. No executable ticket or
/// at-most-once hardware claim is exposed, even for a first-time acceptance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evidence {
    AuthenticatedHeld,
    IdenticalReplayHeld,
    ObservationHeld,
    ConflictHeld,
}

type StoredIntent = (Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>, String);

pub struct TerminalLab {
    db: Connection,
    policy: TestPolicy,
}
fn validate_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path == Path::new(":memory:")
        || path.to_string_lossy().starts_with("file:")
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn connect(path: &Path) -> Result<Connection> {
    validate_path(path)?;
    let flags =
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let db = Connection::open_with_flags(path, flags)?;
    db.busy_timeout(Duration::from_secs(5))?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
    let mode: String = db.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
    let sync: i64 = db.query_row("PRAGMA synchronous", [], |r| r.get(0))?;
    if mode != "wal" || sync != 2 {
        return Err(Error::Invalid);
    }
    Ok(db)
}
fn clock(db: &Connection, now: u64) -> Result<()> {
    let now = i64::try_from(now).map_err(|_| Error::Time)?;
    let last: i64 = db.query_row("SELECT clock FROM optical_policy WHERE id=1", [], |r| {
        r.get(0)
    })?;
    if now == 0 || now < last {
        return Err(Error::Time);
    }
    if db.execute("UPDATE optical_policy SET clock=?1 WHERE id=1", [now])? != 1 {
        return Err(Error::Invalid);
    }
    Ok(())
}
impl TerminalLab {
    /// Enrollment fails if a policy already exists. Offline backups/cloned
    /// stores and signing-host compromise are outside this TEST durability model.
    pub fn create(path: impl AsRef<Path>, policy: TestPolicy) -> Result<Self> {
        let p = policy.bytes()?;
        validate_path(path.as_ref())?;
        // Exclusive file creation prevents re-enrollment of even a damaged or
        // empty previous store. A failed enrollment leaves an unusable file.
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.as_ref())
            .map_err(Error::Io)?;
        drop(file);
        let mut db = connect(path.as_ref())?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS optical_policy(id INTEGER PRIMARY KEY CHECK(id=1),policy BLOB NOT NULL,clock INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS optical_intents(
                transfer BLOB PRIMARY KEY, attempt BLOB UNIQUE NOT NULL,
                commitment BLOB UNIQUE NOT NULL, commit_wire BLOB NOT NULL,
                session BLOB UNIQUE, nonce BLOB UNIQUE, challenge BLOB, response BLOB,
                state TEXT NOT NULL CHECK(state IN ('registered','challenged','consumed_held')));
            CREATE TABLE IF NOT EXISTS optical_observations(
                transfer BLOB NOT NULL,digest BLOB NOT NULL,wire BLOB NOT NULL,
                PRIMARY KEY(transfer,digest));")?;
        tx.execute("INSERT INTO optical_policy VALUES(1,?1,0)", [p])?;
        tx.commit()?;
        Ok(Self { db, policy })
    }
    /// Reopen never re-creates schema or policy. Provisioning mismatch fails.
    pub fn open(path: impl AsRef<Path>, policy: TestPolicy) -> Result<Self> {
        let db = connect(path.as_ref())?;
        let old: Vec<u8> =
            db.query_row("SELECT policy FROM optical_policy WHERE id=1", [], |r| {
                r.get(0)
            })?;
        if old != policy.bytes()? {
            return Err(Error::Binding);
        }
        for table in ["optical_intents", "optical_observations"] {
            db.prepare(&format!("SELECT * FROM {table} LIMIT 0"))?;
        }
        Ok(Self { db, policy })
    }
    fn advance_time(&mut self, now: u64) -> Result<()> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        clock(&tx, now)?;
        tx.commit()?;
        Ok(())
    }
    fn check_commit(&self, c: &Envelope, now: u64) -> Result<()> {
        c.verify(self.policy.authority, now)?;
        if c.kind != Kind::Precommit
            || c.scope.network != self.policy.network
            || c.scope.terminal != self.policy.terminal
            || c.scope.epoch != self.policy.epoch
        {
            return Err(Error::Binding);
        }
        Ok(())
    }
    /// Import one previously committed, terminal-specific synthetic allocation.
    /// Never infers custody availability from the signature or camera.
    pub fn import(&mut self, c: &Envelope, now: u64) -> Result<()> {
        self.advance_time(now)?;
        self.check_commit(c, now)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        clock(&tx, now)?;
        let existing: Option<Vec<u8>> = tx
            .query_row(
                "SELECT commit_wire FROM optical_intents WHERE transfer=?1",
                [c.scope.transfer.as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = existing {
            if old != c.encode() {
                return Err(Error::Binding);
            }
        } else {
            if tx.execute("INSERT INTO optical_intents(transfer,attempt,commitment,commit_wire,state) VALUES(?1,?2,?3,?4,'registered')",
                params![c.scope.transfer.as_slice(),c.scope.attempt.as_slice(),c.digest().as_slice(),c.encode()])?!=1 {
                return Err(Error::Invalid);
            }
        }
        tx.commit()?;
        Ok(())
    }
    /// Session/challenge values come from independent cryptographic randomness
    /// in a future adapter. This deterministic lab takes explicit test values.
    /// One challenge only: interruption/expiry cannot authorize a fresh attempt.
    pub fn challenge(
        &mut self,
        c: &Envelope,
        key: &SigningKey,
        session: [u8; 32],
        nonce: [u8; 32],
        now: u64,
        expires: u64,
    ) -> Result<Envelope> {
        self.advance_time(now)?;
        self.check_commit(c, now)?;
        if IdentityId::from_signing_key(key) != self.policy.terminal
            || expires > c.expires_at
            || expires.saturating_sub(now) > MAX_SESSION_SECS
        {
            return Err(Error::Binding);
        }
        let mut value = c.derive(Kind::Challenge, key);
        value.session = session;
        value.nonce = nonce;
        value.issued_at = now;
        value.expires_at = expires;
        let challenge = Envelope::signed(key, value)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        clock(&tx, now)?;
        if tx.execute(
            "UPDATE optical_intents SET session=?1,nonce=?2,challenge=?3,state='challenged'
            WHERE transfer=?4 AND commitment=?5 AND state='registered'",
            params![
                session.as_slice(),
                nonce.as_slice(),
                challenge.encode(),
                c.scope.transfer.as_slice(),
                c.digest().as_slice()
            ],
        )? != 1
        {
            return Err(Error::Replay);
        }
        tx.commit()?;
        Ok(challenge)
    }
    /// Entire response verification and permanent consumption share one SQLite
    /// transaction. Commit precedes return; lost ACK/restart cannot reopen it.
    pub fn accept(
        &mut self,
        response: &Envelope,
        scanned_holder: IdentityId,
        now: u64,
    ) -> Result<Evidence> {
        self.advance_time(now)?;
        response.verify(scanned_holder, now)?;
        if response.kind != Kind::Response || response.scope.holder != scanned_holder {
            return Err(Error::Binding);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        clock(&tx, now)?;
        let (commit, challenge, old, state): StoredIntent = tx.query_row(
            "SELECT commit_wire,challenge,response,state FROM optical_intents WHERE transfer=?1",
            [response.scope.transfer.as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let c = Envelope::decode(&commit)?;
        c.verify(self.policy.authority, now)?;
        let h = Envelope::decode(&challenge.ok_or(Error::Binding)?)?;
        h.verify(self.policy.terminal, now)?;
        if c.scope != response.scope
            || c.scope.terminal != self.policy.terminal
            || c.scope.network != self.policy.network
            || c.scope.epoch != self.policy.epoch
            || h.kind != Kind::Challenge
            || h.scope != c.scope
            || h.parent != c.digest()
            || response.parent != h.digest()
            || response.session != h.session
            || response.nonce != h.nonce
            || response.issued_at != h.issued_at
            || response.expires_at != h.expires_at
        {
            return Err(Error::Binding);
        }
        if let Some(old) = old {
            if state != "consumed_held" || old != response.encode() {
                return Err(Error::Replay);
            }
            tx.commit()?;
            return Ok(Evidence::IdenticalReplayHeld);
        }
        if tx.execute("UPDATE optical_intents SET response=?1,state='consumed_held' WHERE transfer=?2 AND state='challenged'",
            params![response.encode(),response.scope.transfer.as_slice()])?!=1 {return Err(Error::Replay);}
        tx.commit()?;
        Ok(Evidence::AuthenticatedHeld)
    }
    /// Complete signed observations retained independently. No class releases a
    /// reservation, changes a ledger balance, or returns another authorization.
    pub fn observe(&mut self, o: &Envelope, now: u64) -> Result<Evidence> {
        self.advance_time(now)?;
        o.verify(self.policy.terminal, now)?;
        if o.kind != Kind::Observation {
            return Err(Error::Binding);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        clock(&tx, now)?;
        let raw: Vec<u8> = tx.query_row(
            "SELECT response FROM optical_intents WHERE transfer=?1 AND state='consumed_held'",
            [o.scope.transfer.as_slice()],
            |r| r.get(0),
        )?;
        let response = Envelope::decode(&raw)?;
        if o.scope != response.scope
            || o.parent != response.digest()
            || o.session != response.session
            || o.nonce != response.nonce
            || o.issued_at != response.issued_at
            || o.expires_at != response.expires_at
        {
            return Err(Error::Binding);
        }
        let mut stmt = tx.prepare("SELECT wire FROM optical_observations WHERE transfer=?1")?;
        let mut conflict = false;
        for raw in stmt.query_map([o.scope.transfer.as_slice()], |r| r.get::<_, Vec<u8>>(0))? {
            if Envelope::decode(&raw?)?.report != o.report {
                conflict = true;
            }
        }
        drop(stmt);
        tx.execute(
            "INSERT OR IGNORE INTO optical_observations VALUES(?1,?2,?3)",
            params![
                o.scope.transfer.as_slice(),
                o.digest().as_slice(),
                o.encode()
            ],
        )?;
        // Check an ignored write did not silently erase the promised evidence.
        let saved: Vec<u8> = tx.query_row(
            "SELECT wire FROM optical_observations WHERE transfer=?1 AND digest=?2",
            params![o.scope.transfer.as_slice(), o.digest().as_slice()],
            |r| r.get(0),
        )?;
        if saved != o.encode() {
            return Err(Error::Invalid);
        }
        tx.commit()?;
        Ok(if conflict {
            Evidence::ConflictHeld
        } else {
            Evidence::ObservationHeld
        })
    }
    pub fn state(&self, transfer: [u8; 32]) -> Result<String> {
        Ok(self.db.query_row(
            "SELECT state FROM optical_intents WHERE transfer=?1",
            [transfer.as_slice()],
            |r| r.get(0),
        )?)
    }
    pub fn observations(&self, transfer: [u8; 32]) -> Result<Vec<Envelope>> {
        let mut s = self
            .db
            .prepare("SELECT wire FROM optical_observations WHERE transfer=?1 ORDER BY digest")?;
        let result = s
            .query_map([transfer.as_slice()], |r| r.get::<_, Vec<u8>>(0))?
            .map(|r| Envelope::decode(&r?))
            .collect();
        result
    }
}
