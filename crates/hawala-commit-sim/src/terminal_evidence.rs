//! Signed synthetic terminal evidence remains held; no resolution or release.
use super::*;
use coordination::{TestDecision, TestState};
use hawala_identity_core::IdentityId;
use test_execution::{
    SignedTestTerminal, TestExecutionScope, TestRailPolicy, TestTerminalAuthority,
    TestTerminalCheck,
};

fn schema(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS terminal_test_policy(
            transfer BLOB PRIMARY KEY,policy BLOB NOT NULL CHECK(length(policy)=104)) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS terminal_test_evidence(
            transfer BLOB PRIMARY KEY,attempt BLOB UNIQUE NOT NULL,digest BLOB NOT NULL,
            frame BLOB NOT NULL CHECK(length(frame)=340)) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS terminal_test_conflicts(
            transfer BLOB NOT NULL,frame BLOB NOT NULL CHECK(length(frame)=340),
            PRIMARY KEY(transfer,frame)) WITHOUT ROWID;",
    )?;
    Ok(())
}
pub(super) fn read_policy(db: &Connection, transfer: [u8; 32]) -> Result<Option<TestRailPolicy>> {
    let exists = db
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='terminal_test_policy'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Ok(None);
    }
    let policy: Option<Vec<u8>> = db
        .query_row(
            "SELECT policy FROM terminal_test_policy WHERE transfer=?1",
            params![transfer.as_slice()],
            |r| r.get(0),
        )
        .optional()?;
    policy.map(|b| TestRailPolicy::decode(&b)).transpose()
}
/// Derive full scope from the existing committed admission and exact posting.
/// Legacy records deliberately cannot enroll a terminal execution rail.
pub(super) fn read_scope(
    db: &Connection,
    transfer: [u8; 32],
) -> Result<Option<TestExecutionScope>> {
    if !super::coordinated::enabled(db)? {
        return Ok(None);
    }
    let row: Option<(Vec<u8>, Vec<u8>)> = db
        .query_row(
            "SELECT binding,state FROM coordinated_operations WHERE transfer=?1",
            params![transfer.as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((binding, state)) = row else {
        return Ok(None);
    };
    if TestState::decode(&state)?.decision != TestDecision::Committed {
        return Ok(None);
    }
    let row: Option<(Vec<u8>, Vec<u8>, u64, Vec<u8>, u64)> = db
        .query_row(
            "SELECT e.attempt_id,s.peer,s.epoch,a.currency,p.debit FROM sim_events e
        JOIN admission_receipts r ON r.transfer=e.transfer_id
        JOIN recovery_subjects s ON s.transfer=e.transfer_id
        JOIN sim_postings p ON p.event_id=e.event_id
        JOIN sim_accounts a ON a.id=p.account WHERE e.transfer_id=?1 AND a.role=2 AND p.debit>0",
            params![transfer.as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((attempt, peer, epoch, currency, amount)) = row else {
        return Err(Error::Invalid);
    };
    Ok(Some(TestExecutionScope {
        transfer,
        attempt: fixed32(attempt)?,
        binding: fixed32(binding)?,
        peer: IdentityId::from_bytes(fixed32(peer)?).map_err(|_| Error::Invalid)?,
        financial_epoch: epoch,
        currency: currency.try_into().map_err(|_| Error::Invalid)?,
        amount,
    }))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestTerminalEvidenceDecision {
    RecordedHeld,
    ReplayHeld,
    ConflictHeld,
    Held,
    Rejected,
}
impl Simulator {
    pub fn test_execution_scope(&self, transfer: [u8; 32]) -> Result<TestExecutionScope> {
        let db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        read_scope(&db, transfer)?.ok_or(Error::Invalid)
    }
    /// Trusted local TEST enrollment; never selected from a peer frame. Policy
    /// and epoch are immutable, and enrollment is not production acceptance.
    pub fn pin_test_terminal_authority(
        &self,
        transfer: [u8; 32],
        policy: TestRailPolicy,
    ) -> Result<()> {
        policy.validate()?;
        let mut db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        schema(&tx)?;
        let scope = read_scope(&tx, transfer)?.ok_or(Error::Invalid)?;
        let old: Option<Vec<u8>> = tx
            .query_row(
                "SELECT policy FROM terminal_test_policy WHERE transfer=?1",
                params![transfer.as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = old {
            return if old == policy.bytes() {
                Ok(())
            } else {
                Err(Error::Conflict)
            };
        };
        let epoch: u64 =
            tx.query_row("SELECT epoch FROM sim_state WHERE id=1", [], |r| r.get(0))?;
        if epoch != scope.financial_epoch {
            return Err(Error::Stale);
        }
        tx.execute(
            "INSERT INTO terminal_test_policy VALUES(?1,?2)",
            params![transfer.as_slice(), policy.bytes()],
        )?;
        tx.commit()?;
        Ok(())
    }
    /// All authenticated terminal reports stay held, including no-effect.
    /// Physical delivery/finality and permission to reverse/release are absent.
    pub fn record_test_terminal_evidence(
        &self,
        receipt: &SignedTestTerminal,
        authority: &dyn TestTerminalAuthority,
    ) -> Result<TestTerminalEvidenceDecision> {
        let mut db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        schema(&tx)?;
        let transfer = receipt.scope().transfer_id();
        let Some(scope) = read_scope(&tx, transfer)? else {
            return Ok(TestTerminalEvidenceDecision::Rejected);
        };
        let pin: Option<Vec<u8>> = tx
            .query_row(
                "SELECT policy FROM terminal_test_policy WHERE transfer=?1",
                params![transfer.as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        let Some(pin) = pin else {
            return Ok(TestTerminalEvidenceDecision::Held);
        };
        let pinned = TestRailPolicy::decode(&pin)?;
        let verified = match authority.verify(receipt, scope, pinned) {
            TestTerminalCheck::Rejected => return Ok(TestTerminalEvidenceDecision::Rejected),
            TestTerminalCheck::Held => return Ok(TestTerminalEvidenceDecision::Held),
            TestTerminalCheck::Verified(v) => v,
        };
        if verified.scope != scope
            || verified.digest != receipt.digest()
            || verified.policy != pinned
        {
            return Err(Error::Invalid);
        }
        let frame = receipt.to_frame();
        let old: Option<Vec<u8>> = tx
            .query_row(
                "SELECT frame FROM terminal_test_evidence WHERE transfer=?1",
                params![transfer.as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        let decision = if let Some(old) = old {
            if old == frame {
                TestTerminalEvidenceDecision::ReplayHeld
            } else {
                tx.execute(
                    "INSERT OR IGNORE INTO terminal_test_conflicts VALUES(?1,?2)",
                    params![transfer.as_slice(), frame],
                )?;
                TestTerminalEvidenceDecision::ConflictHeld
            }
        } else {
            tx.execute(
                "INSERT INTO terminal_test_evidence VALUES(?1,?2,?3,?4)",
                params![
                    transfer.as_slice(),
                    scope.attempt.as_slice(),
                    receipt.digest().as_slice(),
                    frame
                ],
            )?;
            TestTerminalEvidenceDecision::RecordedHeld
        };
        if tx.execute(
            "UPDATE admission_receipts SET state='uncertain' WHERE transfer=?1",
            params![transfer.as_slice()],
        )? != 1
        {
            return Err(Error::Invalid);
        }
        // Never erase a historical claim. Pending attempts become held.
        tx.execute(
            "UPDATE test_outbox SET state='held' WHERE transfer=?1 AND state='pending'",
            params![transfer.as_slice()],
        )?;
        tx.commit()?;
        Ok(decision)
    }
}
