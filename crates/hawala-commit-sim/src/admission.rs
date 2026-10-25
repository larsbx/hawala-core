//! Integrated local TEST authority. A committed result is synthetic intent only.
use super::*;
use hawala_admission::Authorities;
use hawala_authority::{admit_preview, AuthorityEnvelope, Decision};
use hawala_custody_model::{examine, Approval, Command, CustodyPolicy, Request};
use hawala_ledger::{Book, Posting};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    VerifiedTestFixture,
    Unavailable,
    Denied,
}
/// Implementations are trusted local configuration, never selected by a peer.
/// No implementation in this crate establishes real asset control.
pub trait CustodyAuthority {
    fn verify(&self, request: &Request, now: u64) -> Control;
}
/// Independently pinned signed policy plus a bounded synthetic checkpoint.
/// Signatures are verified by the existing custody model, not reimplemented.
pub struct SignedTestCustody {
    pub policy: CustodyPolicy,
    pub approvals: Vec<Approval>,
    pub version: u64,
    pub valid_until: u64,
    pub capacity_minor: u64,
}
impl CustodyAuthority for SignedTestCustody {
    fn verify(&self, r: &Request, now: u64) -> Control {
        if r.command != Command::Reserve
            || r.expected_version != self.version
            || now > self.valid_until
            || r.amount_minor > self.capacity_minor
        {
            return Control::Unavailable;
        }
        match examine(&self.policy, r, &self.approvals) {
            Ok(hawala_custody_model::Decision::SufficientSignatures(_)) => {
                Control::VerifiedTestFixture
            }
            Ok(_) => Control::Unavailable,
            Err(_) => Control::Denied,
        }
    }
}
/// Explicit fail-closed placeholder for every unsupported real custodian.
pub struct UnsupportedRealCustody;
impl CustodyAuthority for UnsupportedRealCustody {
    fn verify(&self, _: &Request, _: u64) -> Control {
        Control::Unavailable
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    Rejected,
    Held,
    /// Not a cash capability; the durable state remains reserved_no_execution.
    Committed(SimulatedRecord),
}
/// Immutable allocation identity and financial provenance for a TEST credit lot.
pub struct TestLot {
    pub id: [u8; 32],
    pub terms_digest: [u8; 32],
    pub available_account: [u8; 32],
    pub reserved_account: [u8; 32],
    pub custody_account: [u8; 32],
    pub custody_version: u64,
    pub custody_epoch: u64,
}
fn schema(tx: &rusqlite::Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS admission_lots(
        id BLOB PRIMARY KEY, terms BLOB NOT NULL, available BLOB UNIQUE NOT NULL,
        reserved BLOB UNIQUE NOT NULL, custody BLOB NOT NULL, version INTEGER NOT NULL,
        epoch INTEGER NOT NULL, used INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE IF NOT EXISTS admission_receipts(
        transfer BLOB PRIMARY KEY, binding BLOB NOT NULL, lot BLOB UNIQUE NOT NULL,
        custody_request BLOB UNIQUE NOT NULL, custody_digest BLOB NOT NULL,
        state TEXT NOT NULL CHECK(state IN ('reserved','uncertain')));
        CREATE TABLE IF NOT EXISTS admission_custody(
        account BLOB PRIMARY KEY, epoch INTEGER NOT NULL, version INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS admission_conflicts(
        transfer BLOB NOT NULL, binding BLOB NOT NULL, PRIMARY KEY(transfer,binding));",
    )?;
    Ok(())
}
impl Simulator {
    /// Provisioning TEST provenance is separate from admission and never creates funds.
    pub fn register_test_lot(&self, l: &TestLot) -> Result<()> {
        if [
            l.id,
            l.terms_digest,
            l.available_account,
            l.reserved_account,
            l.custody_account,
        ]
        .contains(&[0; 32])
            || l.custody_version == 0
            || l.custody_epoch == 0
        {
            return Err(Error::Invalid);
        }
        let mut db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        schema(&tx)?;
        tx.execute(
            "INSERT INTO admission_lots(id,terms,available,reserved,custody,version,epoch)
            VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                l.id.as_slice(),
                l.terms_digest.as_slice(),
                l.available_account.as_slice(),
                l.reserved_account.as_slice(),
                l.custody_account.as_slice(),
                i64_amount(l.custody_version)?,
                i64_amount(l.custody_epoch)?
            ],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO admission_custody VALUES(?1,?2,?3)",
            params![
                l.custody_account.as_slice(),
                i64_amount(l.custody_epoch)?,
                i64_amount(l.custody_version)?
            ],
        )?;
        tx.commit()?;
        Ok(())
    }
    /// One local serializable commit: revalidate signed envelope against the
    /// persisted reference book, check a pinned test custody controller, reserve
    /// a provenance lot, consume nonce, journal accounting, and store receipt.
    /// ExternalReviews remain test reports; no deployment/payout API exists.
    pub fn admit(
        &self,
        a: &AuthorityEnvelope<'_>,
        pinned: &Authorities,
        custody: &dyn CustodyAuthority,
        r: &Request,
        now: u64,
    ) -> Result<Admission> {
        let mut db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        schema(&tx)?;
        let t = a.accepted.terms();
        let mut h = blake3::Hasher::new_derive_key("hawala/test-admit-binding/v1");
        h.update(&t.digest());
        h.update(&a.qr.digest());
        h.update(&r.digest());
        h.update(&a.reserve_entry.event_id);
        let mut postings = a.reserve_entry.postings.clone();
        postings.sort_by_key(|p| p.account);
        for p in &postings {
            h.update(&p.account);
            h.update(&p.debit_minor.to_be_bytes());
            h.update(&p.credit_minor.to_be_bytes());
        }
        for n in [
            a.exact_quote.rate_num,
            a.exact_quote.rate_den,
            a.exact_quote.source_fee_minor,
            a.exact_quote.destination_fee_minor,
            a.versions.authority_epoch,
            a.versions.account_version,
            a.versions.custody_account_version,
            a.versions.custody_authority_epoch,
        ] {
            h.update(&n.to_be_bytes());
        }
        h.update(&a.exact_quote.quote_digest);
        let binding = *h.finalize().as_bytes();
        let old: Option<Vec<u8>> = tx
            .query_row(
                "SELECT binding FROM admission_receipts WHERE transfer=?1",
                params![t.transfer_id.as_slice()],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(old) = old {
            if old != binding {
                tx.execute(
                    "INSERT OR IGNORE INTO admission_conflicts VALUES(?1,?2)",
                    params![t.transfer_id.as_slice(), binding.as_slice()],
                )?;
                tx.commit()?;
                return Ok(Admission::Rejected);
            }
            let (event, attempt, version): (Vec<u8>, Vec<u8>, i64) = tx.query_row(
                "SELECT event_id,attempt_id,committed_version FROM sim_events WHERE transfer_id=?1",
                params![t.transfer_id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            return Ok(Admission::Committed(SimulatedRecord {
                transfer_id: t.transfer_id,
                event_id: fixed32(event)?,
                attempt_id: fixed32(attempt)?,
                committed_state_version: version as u64,
            }));
        }
        // Authority configuration must be locally pinned, not supplied by a peer.
        if a.pinned_authorities != pinned || a.expected_verifier != a.qr.request().verifier {
            return Ok(Admission::Rejected);
        }
        if r.command != Command::Reserve
            || r.request_id != a.qr.request().attempt_id
            || r.obligation_digest != t.digest()
            || r.currency != t.payout_currency
            || r.amount_minor != t.payout_minor
            || r.depositing_dar != t.disburser
            || r.expected_version != a.versions.custody_account_version
            || r.epoch != a.versions.custody_authority_epoch
        {
            return Ok(Admission::Rejected);
        }
        let lot:Option<(Vec<u8>,Vec<u8>,Vec<u8>,Vec<u8>,i64,i64,i64)>=tx.query_row(
            "SELECT terms,available,reserved,custody,version,epoch,used FROM admission_lots WHERE id=?1",
            params![r.allocation_id.as_slice()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?))).optional()?;
        let Some((terms, available, reserved, account, version, epoch, used)) = lot else {
            return Ok(Admission::Held);
        };
        if terms != t.digest()
            || account != r.custody_account
            || used != 0
            || version != i64_amount(r.expected_version)?
            || epoch != i64_amount(r.epoch)?
        {
            return Ok(Admission::Held);
        }
        if postings.len() != 2
            || !postings.iter().any(|p| {
                p.account.as_slice() == available
                    && p.debit_minor == r.amount_minor
                    && p.credit_minor == 0
            })
            || !postings.iter().any(|p| {
                p.account.as_slice() == reserved
                    && p.credit_minor == r.amount_minor
                    && p.debit_minor == 0
            })
        {
            return Ok(Admission::Rejected);
        }
        let current: (i64, i64) = tx.query_row(
            "SELECT epoch,version FROM admission_custody WHERE account=?1",
            params![r.custody_account.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if current != (i64_amount(r.epoch)?, i64_amount(r.expected_version)?) {
            return Ok(Admission::Held);
        }
        match custody.verify(r, now) {
            Control::Unavailable => return Ok(Admission::Held),
            Control::Denied => return Ok(Admission::Rejected),
            Control::VerifiedTestFixture => {}
        }
        // Rehydrate the exact balanced snapshot using Book::post; all arithmetic
        // and currency validation stay in the existing reference ledger.
        let mut stmt =
            tx.prepare("SELECT id,currency,normal,balance FROM sim_accounts ORDER BY id")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        let mut accounts = Vec::new();
        let mut opening = Vec::new();
        for row in rows {
            let (id, currency, normal, balance) = row?;
            let id = fixed32(id)?;
            let currency = currency.try_into().map_err(|_| Error::Invalid)?;
            let normal = match normal {
                1 => NormalSide::Debit,
                2 => NormalSide::Credit,
                _ => return Err(Error::Invalid),
            };
            accounts.push(Account {
                id,
                currency,
                normal,
            });
            if balance > 0 {
                opening.push(Posting {
                    account: id,
                    debit_minor: if normal == NormalSide::Debit {
                        balance as u64
                    } else {
                        0
                    },
                    credit_minor: if normal == NormalSide::Credit {
                        balance as u64
                    } else {
                        0
                    },
                });
            }
        }
        drop(stmt);
        let mut book = Book::new(accounts).map_err(|_| Error::Invalid)?;
        if !opening.is_empty() {
            book.post(Entry {
                event_id: [255; 32],
                postings: opening,
            })
            .map_err(|_| Error::Invalid)?;
        }
        let plan = match admit_preview(a, &book, now) {
            Decision::Rejected(_) => return Ok(Admission::Rejected),
            Decision::Held(_) => return Ok(Admission::Held),
            Decision::Prepared(p) => p,
        };
        let result = match Self::record_in_transaction(&tx, &plan, a.reserve_entry) {
            Ok(Outcome::RecordedSimulation(v)) => v,
            Ok(Outcome::IdenticalReplay(_)) => return Ok(Admission::Rejected), // legacy commit cannot acquire provenance later
            Err(Error::Stale | Error::Insufficient) => return Ok(Admission::Held),
            Err(Error::Sql(rusqlite::Error::SqliteFailure(e, _)))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                return Ok(Admission::Rejected)
            }
            Err(e) => return Err(e),
        };
        let next = i64_amount(r.expected_version)?
            .checked_add(1)
            .ok_or(Error::Invalid)?;
        tx.execute(
            "UPDATE admission_custody SET version=?1 WHERE account=?2",
            params![next, r.custody_account.as_slice()],
        )?;
        tx.execute(
            "UPDATE admission_lots SET used=1 WHERE id=?1",
            params![r.allocation_id.as_slice()],
        )?;
        tx.execute(
            "INSERT INTO admission_receipts VALUES(?1,?2,?3,?4,?5,'reserved')",
            params![
                t.transfer_id.as_slice(),
                binding.as_slice(),
                r.allocation_id.as_slice(),
                r.request_id.as_slice(),
                r.digest().as_slice()
            ],
        )?;
        tx.commit()?;
        Ok(Admission::Committed(result))
    }
    /// Persist loss of external certainty. No automatic retry, release, payout,
    /// success acknowledgement or collateral-unlock transition is implemented.
    pub fn mark_uncertain(&self, transfer: [u8; 32]) -> Result<()> {
        let mut db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        schema(&tx)?;
        if tx.execute(
            "UPDATE admission_receipts SET state='uncertain' WHERE transfer=?1",
            params![transfer.as_slice()],
        )? != 1
        {
            return Err(Error::Invalid);
        }
        tx.commit()?;
        Ok(())
    }
    pub fn is_uncertain(&self, transfer: [u8; 32]) -> Result<bool> {
        let db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        Ok(db.query_row(
            "SELECT state='uncertain' FROM admission_receipts WHERE transfer=?1",
            params![transfer.as_slice()],
            |r| r.get(0),
        )?)
    }
}
