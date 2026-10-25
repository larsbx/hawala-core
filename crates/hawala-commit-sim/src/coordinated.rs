//! TEST-only common decision order inside the existing admission database.
//! Signed policy claims remain synthetic evidence, never proof of real custody.
use super::*;
use crate::coordination::{TestAction, TestDecision, TestState};
use hawala_authority::AuthorityEnvelope;
use hawala_custody_model::{Command, Request};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestReservationSide {
    Origin,
    Disburser,
}
impl TestReservationSide {
    fn code(self) -> u8 {
        match self {
            Self::Origin => 0,
            Self::Disburser => 1,
        }
    }
}
pub struct OriginReservation<'a> {
    pub request: &'a Request,
    pub authority: &'a dyn CustodyAuthority,
    pub expected_coordination_version: u64,
}

pub(super) fn schema(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS coordinated_profile (
            id INTEGER PRIMARY KEY CHECK(id=1));
        CREATE TABLE IF NOT EXISTS coordinated_operations (
            transfer BLOB PRIMARY KEY CHECK(length(transfer)=32),
            binding BLOB NOT NULL CHECK(length(binding)=32), epoch INTEGER NOT NULL CHECK(epoch>0),
            state BLOB NOT NULL CHECK(length(state)=13)) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS coordinated_allocations (
            transfer BLOB NOT NULL, side INTEGER NOT NULL CHECK(side IN (0,1)),
            request BLOB UNIQUE NOT NULL CHECK(length(request)=32),
            allocation BLOB UNIQUE NOT NULL CHECK(length(allocation)=32),
            digest BLOB NOT NULL CHECK(length(digest)=32),
            account BLOB NOT NULL CHECK(length(account)=32), epoch INTEGER NOT NULL, version INTEGER NOT NULL,
            PRIMARY KEY(transfer,side), UNIQUE(account,epoch,version)) WITHOUT ROWID;")?;
    Ok(())
}
pub(super) fn enabled(db: &Connection) -> Result<bool> {
    Ok(db
        .query_row("SELECT 1 FROM coordinated_profile WHERE id=1", [], |_| {
            Ok(())
        })
        .optional()?
        .is_some())
}
fn binding(a: &AuthorityEnvelope<'_>, origin: &Request, payout: &Request) -> [u8; 32] {
    let mut h = blake3::Hasher::new_derive_key("hawala/test-coordinated-admission/v1");
    h.update(&super::admission::admission_binding(a, payout));
    h.update(&origin.digest());
    *h.finalize().as_bytes()
}
fn load(db: &Connection, transfer: [u8; 32]) -> Result<([u8; 32], u64, TestState)> {
    let (digest, epoch, state): (Vec<u8>, i64, Vec<u8>) = db.query_row(
        "SELECT binding,epoch,state FROM coordinated_operations WHERE transfer=?1",
        params![transfer.as_slice()],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    Ok((
        fixed32(digest)?,
        u64::try_from(epoch).map_err(|_| Error::Invalid)?,
        TestState::decode(&state)?,
    ))
}
fn current(db: &Connection, epoch: u64) -> Result<bool> {
    Ok(
        db.query_row::<i64, _, _>("SELECT epoch FROM sim_state WHERE id=1", [], |r| r.get(0))?
            == i64_amount(epoch)?,
    )
}
fn checkpoint(db: &Connection, r: &Request) -> Result<bool> {
    let row: Option<(i64, i64)> = db
        .query_row(
            "SELECT epoch,version FROM admission_custody WHERE account=?1",
            params![r.custody_account.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(row == Some((i64_amount(r.epoch)?, i64_amount(r.expected_version)?)))
}
fn save(db: &Connection, transfer: [u8; 32], state: TestState) -> Result<()> {
    if db.execute(
        "UPDATE coordinated_operations SET state=?1 WHERE transfer=?2",
        params![state.bytes(), transfer.as_slice()],
    )? != 1
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
impl Simulator {
    /// Irreversibly selects the coordinated TEST profile before any legacy
    /// event exists. Subsequent preparations keep that profile. No funds created.
    pub fn prepare_test_coordination(
        &self,
        a: &AuthorityEnvelope<'_>,
        origin: &Request,
        payout: &Request,
    ) -> Result<[u8; 32]> {
        let t = a.accepted.terms();
        if origin.command != Command::Reserve
            || payout.command != Command::Reserve
            || origin.depositing_dar != t.origin
            || origin.currency != t.funding_currency
            || origin.amount_minor != t.funding_minor
            || origin.obligation_digest != t.digest()
            || origin.network_id != payout.network_id
            || origin.custody_account == payout.custody_account
            || origin.request_id == payout.request_id
            || origin.allocation_id == payout.allocation_id
            || [
                origin.request_id,
                origin.allocation_id,
                origin.custody_account,
                payout.request_id,
                payout.allocation_id,
                payout.custody_account,
            ]
            .contains(&[0; 32])
            || origin.epoch == 0
            || origin.expected_version == 0
            || payout.epoch == 0
            || payout.expected_version == 0
        {
            return Err(Error::Invalid);
        }
        let digest = binding(a, origin, payout);
        let mut db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        super::admission::schema(&tx)?;
        let old: Option<Vec<u8>> = tx
            .query_row(
                "SELECT binding FROM coordinated_operations WHERE transfer=?1",
                params![t.transfer_id.as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = old {
            if old != digest {
                return Err(Error::Conflict);
            }
            return Ok(digest); // permanent binding, including after final abort
        }
        if !current(&tx, a.versions.authority_epoch)? {
            return Err(Error::Stale);
        }
        if !enabled(&tx)?
            && tx.query_row::<i64, _, _>("SELECT count(*) FROM sim_events", [], |r| r.get(0))? != 0
        {
            return Err(Error::Invalid); // never retrofit coordination onto a legacy commit
        }
        tx.execute("INSERT OR IGNORE INTO coordinated_profile VALUES(1)", [])?;
        let initial = TestState {
            version: 1,
            decision: TestDecision::Undecided,
            left_locked: false,
            right_locked: false,
            left_released: false,
            right_released: false,
        };
        tx.execute(
            "INSERT INTO coordinated_operations VALUES(?1,?2,?3,?4)",
            params![
                t.transfer_id.as_slice(),
                digest.as_slice(),
                i64_amount(a.versions.authority_epoch)?,
                initial.bytes()
            ],
        )?;
        for (side, r) in [
            (TestReservationSide::Origin, origin),
            (TestReservationSide::Disburser, payout),
        ] {
            tx.execute(
                "INSERT OR IGNORE INTO admission_custody VALUES(?1,?2,?3)",
                params![
                    r.custody_account.as_slice(),
                    i64_amount(r.epoch)?,
                    i64_amount(r.expected_version)?
                ],
            )?;
            if !checkpoint(&tx, r)? {
                return Err(Error::Stale);
            }
            tx.execute(
                "INSERT INTO coordinated_allocations VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    t.transfer_id.as_slice(),
                    side.code(),
                    r.request_id.as_slice(),
                    r.allocation_id.as_slice(),
                    r.digest().as_slice(),
                    r.custody_account.as_slice(),
                    i64_amount(r.epoch)?,
                    i64_amount(r.expected_version)?
                ],
            )?;
        }
        tx.commit()?;
        Ok(digest)
    }
    pub fn test_coordination_state(&self, transfer: [u8; 32]) -> Result<TestState> {
        let db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        Ok(load(&db, transfer)?.2)
    }
    /// Observe independently pinned SIGNED TEST policy evidence, not actual
    /// reservation finality. Both sides are rechecked before new admission.
    pub fn confirm_test_reservation(
        &self,
        transfer: [u8; 32],
        digest: [u8; 32],
        side: TestReservationSide,
        r: &Request,
        authority: &dyn CustodyAuthority,
        expected_version: u64,
        now: u64,
    ) -> Result<TestState> {
        let mut db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (pinned, epoch, state) = load(&tx, transfer)?;
        if pinned != digest {
            return Err(Error::Conflict);
        }
        let expected: Vec<u8> = tx.query_row(
            "SELECT digest FROM coordinated_allocations WHERE transfer=?1 AND side=?2",
            params![transfer.as_slice(), side.code()],
            |row| row.get(0),
        )?;
        if expected != r.digest() {
            return Err(Error::Conflict);
        }
        let already = match side {
            TestReservationSide::Origin => state.left_locked,
            TestReservationSide::Disburser => state.right_locked,
        };
        if already || !current(&tx, epoch)? {
            return Ok(state);
        }
        if expected_version != state.version {
            return Err(Error::Stale);
        }
        if !checkpoint(&tx, r)? {
            return Ok(state);
        }
        match authority.verify(r, now) {
            Control::Unavailable => return Ok(state),
            Control::Denied => return Err(Error::Invalid),
            Control::VerifiedTestFixture => (),
        }
        let next = state.transition(match side {
            TestReservationSide::Origin => TestAction::ConfirmLeft,
            TestReservationSide::Disburser => TestAction::ConfirmRight,
        })?;
        save(&tx, transfer, next)?;
        tx.commit()?;
        Ok(next)
    }
    /// Permanent local TEST noncommit tombstone. It is not an external abort
    /// certificate and cannot release assets. No committed operation can abort.
    pub fn abort_test_coordination(
        &self,
        transfer: [u8; 32],
        digest: [u8; 32],
        expected_version: u64,
    ) -> Result<TestState> {
        let mut db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (pinned, epoch, state) = load(&tx, transfer)?;
        if pinned != digest {
            return Err(Error::Conflict);
        }
        if state.decision != TestDecision::Undecided || !current(&tx, epoch)? {
            return Ok(state);
        }
        if expected_version != state.version {
            return Err(Error::Stale);
        }
        let next = state.transition(TestAction::Abort)?;
        save(&tx, transfer, next)?;
        tx.commit()?;
        Ok(next)
    }
}

pub(super) fn before_admission(
    tx: &Connection,
    a: &AuthorityEnvelope<'_>,
    origin: Option<&OriginReservation<'_>>,
    payout: &Request,
    now: u64,
) -> Result<Option<Admission>> {
    if !enabled(tx)? {
        return Ok(origin.map(|_| Admission::Held));
    }
    let Some(origin) = origin else {
        return Ok(Some(Admission::Held));
    };
    let transfer = a.accepted.terms().transfer_id;
    let exists = tx
        .query_row(
            "SELECT 1 FROM coordinated_operations WHERE transfer=?1",
            params![transfer.as_slice()],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Ok(Some(Admission::Held));
    }
    let (pinned, epoch, state) = load(tx, transfer)?;
    if pinned != binding(a, origin.request, payout) {
        return Ok(Some(Admission::Rejected));
    }
    let committed = tx
        .query_row(
            "SELECT 1 FROM admission_receipts WHERE transfer=?1",
            params![transfer.as_slice()],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if (state.decision == TestDecision::Committed) != committed {
        return Err(Error::Invalid);
    }
    if state.decision == TestDecision::Committed {
        return Ok(None);
    } // historical non-executable replay
    if state.decision == TestDecision::Aborted
        || !state.left_locked
        || !state.right_locked
        || state.version != origin.expected_coordination_version
        || !current(tx, epoch)?
        || !checkpoint(tx, origin.request)?
    {
        return Ok(Some(Admission::Held));
    }
    Ok(match origin.authority.verify(origin.request, now) {
        Control::VerifiedTestFixture => None,
        Control::Unavailable => Some(Admission::Held),
        Control::Denied => Some(Admission::Rejected),
    })
}
pub(super) fn finish_admission(
    tx: &Connection,
    a: &AuthorityEnvelope<'_>,
    origin: &OriginReservation<'_>,
) -> Result<()> {
    let transfer = a.accepted.terms().transfer_id;
    let (_, _, state) = load(tx, transfer)?;
    if state.decision != TestDecision::Undecided
        || state.version != origin.expected_coordination_version
    {
        return Err(Error::Stale);
    }
    let next = state.transition(TestAction::Commit)?;
    if next.decision != TestDecision::Committed {
        return Err(Error::Invalid);
    }
    let r = origin.request;
    let version = i64_amount(r.expected_version)?;
    if tx.execute(
        "UPDATE admission_custody SET version=?1 WHERE account=?2 AND epoch=?3 AND version=?4",
        params![
            version.checked_add(1).ok_or(Error::Invalid)?,
            r.custody_account.as_slice(),
            i64_amount(r.epoch)?,
            version
        ],
    )? != 1
    {
        return Err(Error::Stale);
    }
    save(tx, transfer, next)
}

impl Simulator {
    /// Export one terminal-specific signed TEST precommitment. Requires the
    /// exact previously committed M6-C7 binding; preparation/abort/legacy plans
    /// cannot export. Uses the existing durable outbox claim, not a second
    /// dispatch rail. Failure/lost export after claim remains uncertain forever.
    /// The caller's signing key is independently pinned as TEST policy by the
    /// optical lab, never selected from a QR packet. No hardware capability.
    pub fn export_test_optical(
        &self,
        a: &AuthorityEnvelope<'_>,
        origin: &Request,
        payout: &Request,
        test_policy_key: &ed25519_dalek::SigningKey,
        now: u64,
        expires_at: u64,
    ) -> Result<Option<hawala_optical_test::Envelope>> {
        use hawala_optical_test::{Envelope, Scope};
        let t = a.accepted.terms();
        let qr = a.qr.request();
        let digest = binding(a, origin, payout);
        {
            let db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
            if !enabled(&db)? {
                return Ok(None);
            }
            let (stored, epoch, state) = load(&db, t.transfer_id)?;
            if state.decision != TestDecision::Committed
                || stored != digest
                || epoch != a.versions.authority_epoch
                || !current(&db, epoch)?
                || qr.verifier != a.expected_verifier
                || qr.holder == qr.verifier
                || now < qr.issued_at
                || expires_at > qr.expires_at
            {
                return Ok(None);
            }
            let (saved, dispatch_expiry): (Vec<u8>,u64) = db.query_row(
                "SELECT r.binding,o.expires_at FROM admission_receipts r JOIN test_outbox o ON o.transfer=r.transfer WHERE r.transfer=?1",
                [t.transfer_id.as_slice()], |r| Ok((r.get(0)?,r.get(1)?)))?;
            if saved != super::admission::admission_binding(a, payout)
                || expires_at > dispatch_expiry
            {
                return Ok(None);
            }
        }
        let scope = Scope {
            network: payout.network_id,
            transfer: t.transfer_id,
            terms: t.digest(),
            attempt: qr.attempt_id,
            holder: qr.holder,
            terminal: qr.verifier,
            origin: t.origin,
            disburser: t.disburser,
            coordinated_binding: digest,
            amount_minor: t.payout_minor,
            fee_minor: qr.disclosed_fee_minor,
            epoch: a.versions.authority_epoch,
            currency: t.payout_currency,
        };
        // Construct/sign before claiming so malformed export inputs cannot
        // consume an otherwise valid outbox. It is not exposed until commit.
        let message = Envelope::precommit(test_policy_key, scope, now, expires_at)
            .map_err(|_| Error::Invalid)?;
        match self.claim_test_dispatch(
            t.transfer_id,
            qr.attempt_id,
            qr.verifier,
            a.versions.authority_epoch,
            now,
        )? {
            TestDispatchDecision::Claimed(_) => Ok(Some(message)),
            TestDispatchDecision::Held | TestDispatchDecision::Rejected => Ok(None),
        }
    }
}
