//! Isolated, synthetic bilateral reservation/decision journal. NOT custody.
//!
//! No method calls admission, changes ledger balances, issues certificates or
//! releases actual assets. Inputs are local TEST events, not signed evidence.
//! A production coordinator needs independently verified custody, a durable
//! common ordering authority and externally enforced abort/commit fences.
use crate::{Error, Result};
use hawala_identity_core::IdentityId;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use std::{path::Path, sync::Mutex, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TestBinding {
    pub transfer: [u8; 32],
    pub terms_digest: [u8; 32],
    pub origin: IdentityId,
    pub disburser: IdentityId,
    pub left_allocation: [u8; 32],
    pub right_allocation: [u8; 32],
    pub epoch: u64,
}
impl TestBinding {
    fn bytes(self) -> Result<Vec<u8>> {
        if self.transfer == [0; 32]
            || self.terms_digest == [0; 32]
            || self.left_allocation == [0; 32]
            || self.right_allocation == [0; 32]
            || self.left_allocation == self.right_allocation
            || self.origin == self.disburser
            || self.epoch == 0
            || self.epoch > i64::MAX as u64
        {
            return Err(Error::Invalid);
        }
        let mut b = b"hawala/TEST-coordination-binding/v1\0".to_vec();
        for p in [
            self.transfer,
            self.terms_digest,
            *self.origin.as_bytes(),
            *self.disburser.as_bytes(),
            self.left_allocation,
            self.right_allocation,
        ] {
            b.extend_from_slice(&p);
        }
        b.extend_from_slice(&self.epoch.to_be_bytes());
        Ok(b)
    }
    pub fn digest(self) -> Result<[u8; 32]> {
        Ok(*blake3::hash(&self.bytes()?).as_bytes())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TestAction {
    ConfirmLeft = 1,
    ConfirmRight = 2,
    Commit = 3,
    Abort = 4,
    ReleaseLeft = 5,
    ReleaseRight = 6,
    Timeout = 7,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TestDecision {
    Undecided = 0,
    Committed = 1,
    Aborted = 2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TestState {
    pub version: u64,
    pub decision: TestDecision,
    /// Historical confirmation; remains true after a synthetic release.
    pub left_locked: bool,
    pub right_locked: bool,
    pub left_released: bool,
    pub right_released: bool,
}
impl TestState {
    pub(super) fn transition(self, action: TestAction) -> Result<Self> {
        let mut next = self;
        match action {
            TestAction::ConfirmLeft => next.left_locked = true,
            TestAction::ConfirmRight => next.right_locked = true,
            TestAction::Commit
                if self.decision == TestDecision::Undecided
                    && self.left_locked
                    && self.right_locked =>
            {
                next.decision = TestDecision::Committed
            }
            TestAction::Abort if self.decision == TestDecision::Undecided => {
                next.decision = TestDecision::Aborted
            }
            TestAction::ReleaseLeft
                if self.decision == TestDecision::Aborted && self.left_locked =>
            {
                next.left_released = true
            }
            TestAction::ReleaseRight
                if self.decision == TestDecision::Aborted && self.right_locked =>
            {
                next.right_released = true
            }
            _ => (),
        }
        if next != self {
            next.version = self
                .version
                .checked_add(1)
                .filter(|v| *v <= i64::MAX as u64)
                .ok_or(Error::Invalid)?;
        }
        Ok(next)
    }
    pub(super) fn bytes(self) -> Vec<u8> {
        let mut b = self.version.to_be_bytes().to_vec();
        b.extend_from_slice(&[
            self.decision as u8,
            self.left_locked as u8,
            self.right_locked as u8,
            self.left_released as u8,
            self.right_released as u8,
        ]);
        b
    }
    pub(super) fn decode(b: &[u8]) -> Result<Self> {
        if b.len() != 13 || b[9..].iter().any(|x| *x > 1) {
            return Err(Error::Invalid);
        }
        let decision = match b[8] {
            0 => TestDecision::Undecided,
            1 => TestDecision::Committed,
            2 => TestDecision::Aborted,
            _ => return Err(Error::Invalid),
        };
        let s = Self {
            version: u64::from_be_bytes(b[..8].try_into().unwrap()),
            decision,
            left_locked: b[9] != 0,
            right_locked: b[10] != 0,
            left_released: b[11] != 0,
            right_released: b[12] != 0,
        };
        if s.version == 0
            || s.version > i64::MAX as u64
            || (s.decision == TestDecision::Committed
                && (!s.left_locked || !s.right_locked || s.left_released || s.right_released))
            || (s.left_released && (s.decision != TestDecision::Aborted || !s.left_locked))
            || (s.right_released && (s.decision != TestDecision::Aborted || !s.right_locked))
        {
            return Err(Error::Invalid);
        }
        Ok(s)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TestCommand {
    pub request_id: [u8; 32],
    pub binding_digest: [u8; 32],
    pub epoch: u64,
    pub expected_version: u64,
    pub action: TestAction,
}
impl TestCommand {
    fn bytes(self) -> Vec<u8> {
        let mut b = b"hawala/TEST-coordination-command/v1\0".to_vec();
        b.extend_from_slice(&self.request_id);
        b.extend_from_slice(&self.binding_digest);
        b.extend_from_slice(&self.epoch.to_be_bytes());
        b.extend_from_slice(&self.expected_version.to_be_bytes());
        b.push(self.action as u8);
        b
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TestResult {
    Changed = 1,
    Held = 2,
    Stale = 3,
    WrongScope = 4,
}
/// A local test replay result, never an external abort/commit certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TestReceipt {
    pub result: TestResult,
    /// State at the original request; query `state` for the current state.
    pub state: TestState,
    pub replayed: bool,
}

pub struct TestCoordinator {
    db: Mutex<Connection>,
    binding: TestBinding,
}
impl TestCoordinator {
    /// Uses a separate file from the admission simulator. There is one binding
    /// per file; reopening with changed terms, dars, allocation or epoch fails.
    pub fn open(path: impl AsRef<Path>, binding: TestBinding) -> Result<Self> {
        let path = path.as_ref();
        if path.as_os_str().is_empty()
            || path == Path::new(":memory:")
            || path.as_os_str().to_string_lossy().starts_with("file:")
        {
            return Err(Error::Invalid);
        }
        let bytes = binding.bytes()?;
        // URI mode can silently turn a path into an in-memory database or
        // select an unsafe VFS. Only ordinary filesystem paths are supported.
        let mut db =
            Connection::open_with_flags(path, OpenFlags::default() & !OpenFlags::SQLITE_OPEN_URI)?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS test_coordination (
                id INTEGER PRIMARY KEY CHECK(id=1), binding BLOB NOT NULL,
                state BLOB NOT NULL CHECK(length(state)=13));
            CREATE TABLE IF NOT EXISTS test_coordination_requests (
                request_id BLOB PRIMARY KEY CHECK(length(request_id)=32),
                command BLOB NOT NULL, result INTEGER NOT NULL CHECK(result BETWEEN 1 AND 4),
                state BLOB NOT NULL CHECK(length(state)=13)) WITHOUT ROWID;",
        )?;
        let mode: String = db.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        if mode != "wal" {
            return Err(Error::Invalid);
        }
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let initial = TestState {
            version: 1,
            decision: TestDecision::Undecided,
            left_locked: false,
            right_locked: false,
            left_released: false,
            right_released: false,
        };
        tx.execute(
            "INSERT OR IGNORE INTO test_coordination VALUES (1,?1,?2)",
            params![bytes, initial.bytes()],
        )?;
        let pinned: Vec<u8> = tx.query_row(
            "SELECT binding FROM test_coordination WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        if pinned != bytes {
            return Err(Error::Conflict);
        }
        tx.commit()?;
        Ok(Self {
            db: Mutex::new(db),
            binding,
        })
    }
    pub fn state(&self) -> Result<TestState> {
        let db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        TestState::decode(&db.query_row::<Vec<u8>, _, _>(
            "SELECT state FROM test_coordination WHERE id=1",
            [],
            |r| r.get(0),
        )?)
    }
    pub fn apply(&self, cmd: TestCommand) -> Result<TestReceipt> {
        if cmd.request_id == [0; 32] {
            return Err(Error::Invalid);
        }
        let mut db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old: Option<(Vec<u8>, u8, Vec<u8>)> = tx
            .query_row(
                "SELECT command,result,state FROM test_coordination_requests WHERE request_id=?1",
                params![cmd.request_id.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let bytes = cmd.bytes();
        if let Some((prior, result, state)) = old {
            if prior != bytes {
                return Err(Error::Conflict);
            }
            let result = match result {
                1 => TestResult::Changed,
                2 => TestResult::Held,
                3 => TestResult::Stale,
                4 => TestResult::WrongScope,
                _ => return Err(Error::Invalid),
            };
            return Ok(TestReceipt {
                result,
                state: TestState::decode(&state)?,
                replayed: true,
            });
        }
        let state = TestState::decode(&tx.query_row::<Vec<u8>, _, _>(
            "SELECT state FROM test_coordination WHERE id=1",
            [],
            |r| r.get(0),
        )?)?;
        let mut next = state;
        let result =
            if cmd.binding_digest != self.binding.digest()? || cmd.epoch != self.binding.epoch {
                TestResult::WrongScope
            } else if cmd.expected_version != state.version {
                TestResult::Stale
            } else {
                next = state.transition(cmd.action)?;
                if next == state {
                    TestResult::Held
                } else {
                    TestResult::Changed
                }
            };
        // Decision/lock state and its replay record have one commit point.
        tx.execute(
            "UPDATE test_coordination SET state=?1 WHERE id=1",
            params![next.bytes()],
        )?;
        tx.execute(
            "INSERT INTO test_coordination_requests VALUES (?1,?2,?3,?4)",
            params![cmd.request_id.as_slice(), bytes, result as u8, next.bytes()],
        )?;
        tx.commit()?;
        Ok(TestReceipt {
            result,
            state: next,
            replayed: false,
        })
    }
}
