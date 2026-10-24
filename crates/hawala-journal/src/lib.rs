//! Durable *intent* journal for bilateral Hawala transfers.
//! This is not a double-entry ledger, authority for cash, or a dispenser API.
//! It never commands hardware or automatically retries a payout.
use std::{fs::{File, OpenOptions}, path::{Path, PathBuf}, sync::Mutex, time::Duration};
use fs2::FileExt;

use hawala_protocol::{ObligationEvidence, SignedTerms};
use hawala_atm::{Capabilities, ReportClass, SignedTerminalObservation};
use hawala_human::SignedHandoff;
use hawala_flow::{bind, FlowKind, Role};
use hawala_interaction::{AgreedInteraction, ParticipantKind};
use hawala_qr_auth::{verify_signed, SignedAuthorization};
use hawala_identity_core::IdentityId;
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
    UntrustedTerminal,
    InvalidTerminalEvidence,
    InvalidHandoffEvidence,
    InvalidQrAuthorization,
    AuthorizationReplay,
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
                 attempt_id BLOB UNIQUE CHECK(attempt_id IS NULL OR length(attempt_id) = 32),
                 terminal_id BLOB CHECK(terminal_id IS NULL OR length(terminal_id) = 32)
             ) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS events(
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 transfer_id BLOB NOT NULL,
                 kind TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS terminal_reports(
                 transfer_id BLOB PRIMARY KEY CHECK(length(transfer_id) = 32),
                 terminal_id BLOB NOT NULL CHECK(length(terminal_id) = 32),
                 attempt_id BLOB NOT NULL CHECK(length(attempt_id) = 32),
                 observation_id BLOB NOT NULL CHECK(length(observation_id) = 32),
                 evidence_digest BLOB NOT NULL CHECK(length(evidence_digest) = 32),
                 report_class TEXT NOT NULL,
                 recorded_at INTEGER NOT NULL
             ) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS human_reports(
                 transfer_id BLOB PRIMARY KEY CHECK(length(transfer_id) = 32),
                 attempt_id BLOB NOT NULL CHECK(length(attempt_id) = 32),
                 receipt_id BLOB NOT NULL CHECK(length(receipt_id) = 32),
                 evidence_digest BLOB NOT NULL CHECK(length(evidence_digest) = 32),
                 asserted_at INTEGER NOT NULL
             ) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS qr_attempt_authorizations(
                 transfer_id BLOB PRIMARY KEY CHECK(length(transfer_id) = 32),
                 holder BLOB NOT NULL CHECK(length(holder) = 32),
                 verifier BLOB NOT NULL CHECK(length(verifier) = 32),
                 qr_nonce BLOB NOT NULL CHECK(length(qr_nonce) = 32),
                 attempt_id BLOB NOT NULL CHECK(length(attempt_id) = 32),
                 proof_digest BLOB NOT NULL CHECK(length(proof_digest) = 32),
                 UNIQUE(holder, verifier, qr_nonce)
             ) WITHOUT ROWID;",
        )?;
        // Forward-compatible migration of the M5-A single-authority database.
        // A previous journal has no terminal_id column. Never discard it.
        let has_terminal_id = {
            let mut stmt = db.prepare("PRAGMA table_info(obligations)")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            let mut found = false;
            for name in rows { if name? == "terminal_id" { found = true; } }
            found
        };
        if !has_terminal_id {
            db.execute_batch(
                "ALTER TABLE obligations ADD COLUMN terminal_id BLOB
                 CHECK(terminal_id IS NULL OR length(terminal_id) = 32);",
            )?;
        }
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
        self.begin_attempt_bound(transfer_id, attempt_id, None)
    }

    /// Bind exactly one registered cash terminal to this attempt BEFORE any
    /// hardware command is dispatched. Only an independently authorized
    /// Hawala/ATM policy engine may call this; it does not itself authorize cash.
    pub fn begin_atm_attempt(
        &self,
        transfer_id: [u8; 32],
        attempt_id: [u8; 32],
        expected_terminal: IdentityId,
    ) -> Result<()> {
        self.begin_attempt_bound(transfer_id, attempt_id, Some(expected_terminal))
    }

    fn begin_attempt_bound(
        &self,
        transfer_id: [u8; 32],
        attempt_id: [u8; 32],
        terminal: Option<IdentityId>,
    ) -> Result<()> {
        if transfer_id == [0; 32] || attempt_id == [0; 32] {
            return Err(JournalError::InvalidInput);
        }
        let mut db = self.db.lock().map_err(|_| JournalError::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let terminal_bytes: Option<Vec<u8>> = terminal.map(|id| id.as_bytes().to_vec());
        let changed = tx.execute(
            "UPDATE obligations
             SET phase = 'dispensing', attempt_id = ?2, terminal_id = ?3
             WHERE transfer_id = ?1 AND phase = 'registered'",
            params![transfer_id.as_slice(), attempt_id.as_slice(), terminal_bytes],
        )?;
        if changed != 1 { return Err(JournalError::InvalidTransition); }
        append_event(&tx, &transfer_id, "dispensing")?;
        tx.commit()?;
        Ok(())
    }

    /// Preserve a signed ATM report without treating it as cash finality.
    /// Every report must match the terminal locked into the original durable
    /// attempt and exact transfer terms. The report keeps the cash claim
    /// frozen as UNCERTAIN; it never issues a second hardware instruction.
    /// TrustedCapabilities MUST come from operator policy, not peer messages.
    pub fn record_terminal_report(
        &self,
        signed: &SignedTerminalObservation,
        trusted_capabilities: Capabilities,
    ) -> Result<bool> {
        if !trusted_capabilities.valid() {
            return Err(JournalError::InvalidTerminalEvidence);
        }
        let observation = &signed.observation;
        let mut db = self.db.lock().map_err(|_| JournalError::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let snapshot: Option<(Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>, i64, Vec<u8>, String)> =
            tx.query_row(
                "SELECT terms_digest, attempt_id, terminal_id,
                        payout_minor, payout_currency, phase
                 FROM obligations WHERE transfer_id = ?1",
                params![observation.transfer_id.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?,
                        r.get(3)?, r.get(4)?, r.get(5)?)),
            ).optional()?;
        let (terms_digest, attempt_id, terminal_id, minor, currency, phase) =
            snapshot.ok_or(JournalError::UnknownTransfer)?;
        if phase != "dispensing" && phase != "uncertain" {
            return Err(JournalError::InvalidTransition);
        }
        let expected_terminal: [u8; 32] = terminal_id
            .ok_or(JournalError::UntrustedTerminal)?
            .try_into().map_err(|_| JournalError::InvalidTerminalEvidence)?;
        let terminal_id = IdentityId::from_bytes(expected_terminal)
            .map_err(|_| JournalError::InvalidTerminalEvidence)?;
        signed.verify(terminal_id).map_err(|_| JournalError::InvalidTerminalEvidence)?;
        if attempt_id.as_deref() != Some(observation.attempt_id.as_slice())
            || terms_digest != observation.terms_digest.to_vec()
            || minor < 0 || minor as u64 != observation.payout_minor
            || currency != observation.currency.to_vec()
            || observation.recorded_at > i64::MAX as u64
        {
            return Err(JournalError::InvalidTerminalEvidence);
        }
        let digest = signed.digest();
        let prior: Option<Vec<u8>> = tx.query_row(
            "SELECT evidence_digest FROM terminal_reports WHERE transfer_id = ?1",
            params![observation.transfer_id.as_slice()], |r| r.get(0),
        ).optional()?;
        if let Some(prior) = prior {
            return if prior == digest.to_vec() { Ok(false) }
                else { Err(JournalError::Conflict) };
        }
        let disposition = signed.classify(trusted_capabilities);
        tx.execute(
            "INSERT INTO terminal_reports(
                 transfer_id, terminal_id, attempt_id, observation_id,
                 evidence_digest, report_class, recorded_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                observation.transfer_id.as_slice(),
                terminal_id.as_bytes().as_slice(),
                observation.attempt_id.as_slice(),
                observation.observation_id.as_slice(),
                digest.as_slice(),
                disposition.as_str(), observation.recorded_at as i64,
            ],
        )?;
        tx.execute(
            "UPDATE obligations SET phase = 'uncertain'
             WHERE transfer_id = ?1 AND phase = 'dispensing'",
            params![observation.transfer_id.as_slice()],
        )?;
        append_event(&tx, &observation.transfer_id, "terminal_report_recorded")?;
        tx.commit()?;
        Ok(true)
    }

    pub fn terminal_report(&self, transfer_id: [u8; 32]) -> Result<Option<ReportClass>> {
        let db = self.db.lock().map_err(|_| JournalError::StoragePoisoned)?;
        let report: Option<String> = db.query_row(
            "SELECT report_class FROM terminal_reports WHERE transfer_id = ?1",
            params![transfer_id.as_slice()], |r| r.get(0),
        ).optional()?;
        report.map(|s| match s.as_str() {
            "reported_delivered" => Ok(ReportClass::ReportedDelivered),
            "reported_not_delivered" => Ok(ReportClass::ReportedNotDelivered),
            "uncertain" => Ok(ReportClass::Uncertain),
            _ => Err(JournalError::InvalidTerminalEvidence),
        }).transpose()
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


    /// Record mutually signed HUMAN cash-handoff assertions against exactly
    /// the ONE previously committed local payout attempt. No device terminal
    /// may be bound to this attempt, and this method never authorizes cash
    /// delivery or declares customer funds settled.
    pub fn record_human_handoff(
        &self,
        accepted: &SignedTerms,
        receipt: &SignedHandoff,
    ) -> Result<bool> {
        receipt.verify(accepted).map_err(|_| JournalError::InvalidHandoffEvidence)?;
        let claim = receipt.claim();
        if claim.asserted_handoff_at > i64::MAX as u64 {
            return Err(JournalError::InvalidHandoffEvidence);
        }
        let mut db = self.db.lock().map_err(|_| JournalError::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let snapshot: Option<(Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>, i64, Vec<u8>, String)> =
            tx.query_row(
                "SELECT terms_digest, attempt_id, terminal_id, payout_minor,
                        payout_currency, phase FROM obligations WHERE transfer_id = ?1",
                params![claim.transfer_id.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            ).optional()?;
        let (digest, attempt, terminal, payout, currency, phase) =
            snapshot.ok_or(JournalError::UnknownTransfer)?;
        if !matches!(phase.as_str(), "dispensing" | "uncertain") {
            return Err(JournalError::InvalidTransition);
        }
        if terminal.is_some() || attempt.as_deref() != Some(claim.attempt_id.as_slice())
            || digest != claim.terms_digest.to_vec()
            || payout < 0 || payout as u64 != claim.payout_minor
            || currency != claim.currency.to_vec()
        {
            return Err(JournalError::InvalidHandoffEvidence);
        }
        let evidence = receipt.digest();
        let prior: Option<Vec<u8>> = tx.query_row(
            "SELECT evidence_digest FROM human_reports WHERE transfer_id = ?1",
            params![claim.transfer_id.as_slice()], |r| r.get(0),
        ).optional()?;
        if let Some(prior) = prior {
            return if prior == evidence.to_vec() { Ok(false) }
                else { Err(JournalError::Conflict) };
        }
        tx.execute(
            "INSERT INTO human_reports(
                transfer_id, attempt_id, receipt_id, evidence_digest, asserted_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                claim.transfer_id.as_slice(),
                claim.attempt_id.as_slice(),
                claim.receipt_id.as_slice(),
                evidence.as_slice(),
                claim.asserted_handoff_at as i64
            ],
        )?;
        tx.execute(
            "UPDATE obligations SET phase = 'uncertain'
             WHERE transfer_id = ?1 AND phase = 'dispensing'",
            params![claim.transfer_id.as_slice()],
        )?;
        append_event(&tx, &claim.transfer_id, "human_handoff_attested")?;
        tx.commit()?;
        Ok(true)
    }

    pub fn human_handoff_recorded(&self, transfer_id: [u8; 32]) -> Result<bool> {
        let db = self.db.lock().map_err(|_| JournalError::StoragePoisoned)?;
        let present: Option<i64> = db.query_row(
            "SELECT 1 FROM human_reports WHERE transfer_id = ?1",
            params![transfer_id.as_slice()], |r| r.get(0),
        ).optional()?;
        Ok(present.is_some())
    }


    /// Record a verified dynamic QR proof and commit exactly ONE intended
    /// cash-out attempt in the SAME local SQLite transaction. No external cash
    /// command or balance/collateral mutation is executed here. In particular
    /// this is NOT a financial authorization or regulated funding ledger.
    ///
    /// A future production entrypoint must require independently established
    /// operator delegation, funding/collateral capacity, sender/recipient
    /// eligibility, fees and quote acceptance, all atomically reconciled
    /// with the actual financial authority. Current single-node SQLite does
    /// not solve P2P double spending or cross-device identity-key reuse.
    pub fn begin_qr_cashout_intent(
        &self,
        accepted: &SignedTerms,
        agreed: &AgreedInteraction,
        authorization: &SignedAuthorization,
        expected_qr_nonce: [u8; 32],
        expected_challenge: [u8; 32],
        expected_session: [u8; 32],
        expected_verifier: IdentityId,
        now: u64,
    ) -> Result<()> {
        let flow = bind(accepted, agreed, now)
            .map_err(|_| JournalError::InvalidQrAuthorization)?;
        if flow.kind() != FlowKind::CashOut {
            return Err(JournalError::InvalidQrAuthorization);
        }
        let proof = verify_signed(
            authorization, accepted, agreed,
            expected_qr_nonce, expected_challenge, expected_session,
            expected_verifier, now,
        ).map_err(|_| JournalError::InvalidQrAuthorization)?;
        let transfer_id = proof.transfer_id();
        let attempt_id = proof.attempt_id();
        let qr_nonce = proof.qr_nonce();
        let proof_digest = proof.proof_digest();
        let terms_digest = proof.terms_digest();
        let intent = agreed.intent();
        let cash_peer = match flow.initiator_role() {
            Role::Left => intent.responder,
            Role::Right => intent.initiator,
        };
        let terminal: Option<Vec<u8>> = match cash_peer.kind {
            ParticipantKind::Machine => Some(cash_peer.identity.as_bytes().to_vec()),
            ParticipantKind::Human => None,
        };
        let mut db = self.db.lock().map_err(|_| JournalError::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Unique nonce is permanent (no TTL pruning), scoped by both full
        // identities. On ANY failure the nonce and state change roll back.
        let inserted = tx.execute(
            "INSERT INTO qr_attempt_authorizations(
                transfer_id, holder, verifier, qr_nonce, attempt_id, proof_digest
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT DO NOTHING",
            params![
                transfer_id.as_slice(),
                proof.holder().as_bytes().as_slice(),
                proof.verifier().as_bytes().as_slice(),
                qr_nonce.as_slice(),
                attempt_id.as_slice(),
                proof_digest.as_slice(),
            ],
        )?;
        if inserted != 1 { return Err(JournalError::AuthorizationReplay); }
        let changed = tx.execute(
            "UPDATE obligations
             SET phase = 'dispensing', attempt_id = ?2, terminal_id = ?3
             WHERE transfer_id = ?1 AND terms_digest = ?4
               AND phase = 'registered'",
            params![
                transfer_id.as_slice(),
                attempt_id.as_slice(),
                terminal,
                terms_digest.as_slice()
            ],
        )?;
        if changed != 1 { return Err(JournalError::InvalidTransition); }
        append_event(&tx, &transfer_id, "qr_authorized_intent")?;
        tx.commit()?;
        Ok(())
    }

    pub fn qr_authorization_recorded(&self, transfer_id: [u8; 32]) -> Result<bool> {
        let db = self.db.lock().map_err(|_| JournalError::StoragePoisoned)?;
        let present: Option<i64> = db.query_row(
            "SELECT 1 FROM qr_attempt_authorizations WHERE transfer_id = ?1",
            params![transfer_id.as_slice()], |r| r.get(0),
        ).optional()?;
        Ok(present.is_some())
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
