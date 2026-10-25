//! Local synthetic dispatch outbox. No real device or external command API.
use super::*;
use hawala_identity_core::IdentityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestOutboxState {
    Pending,
    ClaimedUncertain,
    HeldBeforeDispatch,
}
/// Produced only after durable claiming. Not a financial or hardware capability.
/// Intentionally not Clone/Copy: test code should hand one value to one mock.
#[derive(Debug, PartialEq, Eq)]
pub struct SyntheticDispatch {
    transfer: [u8; 32],
    attempt: [u8; 32],
    peer: IdentityId,
    epoch: u64,
    execution_scope: Option<test_execution::TestExecutionScope>,
    terminal_policy: Option<test_execution::TestRailPolicy>,
}
impl SyntheticDispatch {
    pub fn transfer_id(&self) -> [u8; 32] {
        self.transfer
    }
    pub fn attempt_id(&self) -> [u8; 32] {
        self.attempt
    }
    pub fn peer(&self) -> IdentityId {
        self.peer
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    /// Full coordinated TEST context; legacy dispatches have no rail scope.
    pub fn test_execution_scope(&self) -> Option<test_execution::TestExecutionScope> {
        self.execution_scope
    }
    pub fn test_terminal_policy(&self) -> Option<test_execution::TestRailPolicy> {
        self.terminal_policy
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum TestDispatchDecision {
    Claimed(SyntheticDispatch),
    Held,
    Rejected,
}

pub(super) fn schema(tx: &rusqlite::Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS test_outbox(
        transfer BLOB PRIMARY KEY CHECK(length(transfer)=32),
        attempt BLOB UNIQUE NOT NULL CHECK(length(attempt)=32),
        queued_at INTEGER NOT NULL CHECK(queued_at>0),
        expires_at INTEGER NOT NULL CHECK(expires_at>queued_at),
        state TEXT NOT NULL CHECK(state IN ('pending','claimed','held')));",
    )?;
    Ok(())
}
impl Simulator {
    /// Atomically claim a synthetic dispatch once, marking the intent uncertain
    /// BEFORE returning to a mock. A crash after claim never permits reclamation.
    /// The peer is the identity authenticated by the QR/Iroh session; all financial
    /// scope comes from the admission journal. Real execution remains unsupported.
    pub fn claim_test_dispatch(
        &self,
        transfer: [u8; 32],
        attempt: [u8; 32],
        peer: IdentityId,
        expected_epoch: u64,
        now: u64,
    ) -> Result<TestDispatchDecision> {
        let mut db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        schema(&tx)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM test_outbox WHERE transfer=?1)",
            params![transfer.as_slice()],
            |r| r.get(0),
        )?;
        if !exists {
            return Ok(TestDispatchDecision::Rejected);
        }
        let row: (Vec<u8>, Vec<u8>, i64, i64, i64, String, String, i64) = tx.query_row(
            "SELECT o.attempt,s.peer,s.epoch,o.queued_at,o.expires_at,o.state,r.state,a.epoch
            FROM test_outbox o JOIN recovery_subjects s ON s.transfer=o.transfer
            JOIN admission_receipts r ON r.transfer=o.transfer JOIN sim_state a ON a.id=1
            WHERE o.transfer=?1",
            params![transfer.as_slice()],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )?;
        let (stored_attempt, stored_peer, epoch, queued, expires, state, intent, current_epoch) =
            row;
        if stored_attempt != attempt || stored_peer != peer.as_bytes() {
            return Ok(TestDispatchDecision::Rejected);
        }
        if epoch != i64_amount(expected_epoch)? || epoch != current_epoch {
            return Ok(TestDispatchDecision::Held);
        }
        if state != "pending" || intent != "reserved" {
            return Ok(TestDispatchDecision::Held);
        }
        let clock = i64_amount(now)?;
        if clock < queued {
            return Ok(TestDispatchDecision::Held);
        }
        if clock >= expires {
            tx.execute(
                "UPDATE test_outbox SET state='held' WHERE transfer=?1",
                params![transfer.as_slice()],
            )?;
            tx.execute(
                "UPDATE admission_receipts SET state='uncertain' WHERE transfer=?1",
                params![transfer.as_slice()],
            )?;
            tx.commit()?;
            return Ok(TestDispatchDecision::Held);
        }
        let execution_scope = super::terminal_evidence::read_scope(&tx, transfer)?;
        let terminal_policy = if execution_scope.is_some() {
            super::terminal_evidence::read_policy(&tx, transfer)?
        } else {
            None
        };
        if tx.execute(
            "UPDATE test_outbox SET state='claimed' WHERE transfer=?1 AND state='pending'",
            params![transfer.as_slice()],
        )? != 1
        {
            return Err(Error::Conflict);
        }
        if tx.execute("UPDATE admission_receipts SET state='uncertain' WHERE transfer=?1 AND state='reserved'",params![transfer.as_slice()])?!=1 {return Err(Error::Conflict);}
        tx.commit()?;
        Ok(TestDispatchDecision::Claimed(SyntheticDispatch {
            transfer,
            attempt,
            peer,
            epoch: expected_epoch,
            execution_scope,
            terminal_policy,
        }))
    }
    /// Recovery inventory. Never returns dispatch values or retries.
    /// Missing entries include legacy intents, which are never auto-enqueued.
    pub fn test_outbox_inventory(&self) -> Result<Vec<([u8; 32], TestOutboxState)>> {
        let mut db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Deferred)?;
        schema(&tx)?;
        let any: bool =
            tx.query_row("SELECT EXISTS(SELECT 1 FROM test_outbox)", [], |r| r.get(0))?;
        if !any {
            return Ok(Vec::new());
        }
        let mut stmt=tx.prepare("SELECT o.transfer,o.state,r.state FROM test_outbox o JOIN admission_receipts r ON r.transfer=o.transfer ORDER BY o.transfer")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (transfer, state, intent) = row?;
            let state = match (state.as_str(), intent.as_str()) {
                ("claimed", _) => TestOutboxState::ClaimedUncertain,
                ("pending", "reserved") => TestOutboxState::Pending,
                _ => TestOutboxState::HeldBeforeDispatch,
            };
            result.push((fixed32(transfer)?, state));
        }
        Ok(result)
    }
}
