//! Signed external observations for TEST intents. Reports never prove cash delivery.
use super::*;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hawala_identity_core::IdentityId;

const DOMAIN: &[u8] = b"hawala/test-execution-observation/v1\0";
/// Fixed bounded frame, including magic and signature; no permissive suffixes.
pub const OBSERVATION_FRAME_LEN: usize = 8 + 32 * 4 + 8 * 3 + 1 + 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationKind {
    Timeout,
    ReportedDelivery,
    ReportedNoDelivery,
}
impl ObservationKind {
    fn tag(self) -> u8 {
        match self {
            Self::Timeout => 1,
            Self::ReportedDelivery => 2,
            Self::ReportedNoDelivery => 3,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionObservation {
    pub transfer_id: [u8; 32],
    pub attempt_id: [u8; 32],
    pub observation_id: [u8; 32],
    pub epoch: u64,
    pub observed_at: u64,
    pub expires_at: u64,
    pub kind: ObservationKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedObservation {
    observation: ExecutionObservation,
    reporter: IdentityId,
    signature: [u8; 64],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationOutcome {
    RecordedHeld,
    IdenticalReplayHeld,
    ConflictHeld,
    Rejected,
}
impl SignedObservation {
    pub fn sign(key: &SigningKey, observation: ExecutionObservation) -> Result<Self> {
        Self::validate_shape(&observation)?;
        let mut s = Self {
            observation,
            reporter: IdentityId::from_signing_key(key),
            signature: [0; 64],
        };
        s.signature = key.sign(&s.statement()).to_bytes();
        Ok(s)
    }
    fn validate_shape(o: &ExecutionObservation) -> Result<()> {
        if [o.transfer_id, o.attempt_id, o.observation_id].contains(&[0; 32])
            || o.epoch == 0
            || o.observed_at == 0
            || o.expires_at < o.observed_at
            || o.expires_at - o.observed_at > 300
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
    pub fn observation(&self) -> ExecutionObservation {
        self.observation
    }
    pub fn reporter(&self) -> IdentityId {
        self.reporter
    }
    pub fn encode(&self) -> Vec<u8> {
        let o = &self.observation;
        let mut v = b"HWTREC01".to_vec();
        for p in [
            &o.transfer_id,
            &o.attempt_id,
            &o.observation_id,
            self.reporter.as_bytes(),
        ] {
            v.extend_from_slice(p);
        }
        for n in [o.epoch, o.observed_at, o.expires_at] {
            v.extend_from_slice(&n.to_be_bytes());
        }
        v.push(o.kind.tag());
        v.extend_from_slice(&self.signature);
        v
    }
    fn statement(&self) -> Vec<u8> {
        let mut v = DOMAIN.to_vec();
        let wire = self.encode();
        v.extend_from_slice(&wire[..wire.len() - 64]);
        v
    }
    pub fn decode(wire: &[u8]) -> Result<Self> {
        if wire.len() != OBSERVATION_FRAME_LEN || &wire[..8] != b"HWTREC01" {
            return Err(Error::Invalid);
        }
        let array = |start: usize| -> Result<[u8; 32]> {
            wire[start..start + 32]
                .try_into()
                .map_err(|_| Error::Invalid)
        };
        let number = |start: usize| -> Result<u64> {
            Ok(u64::from_be_bytes(
                wire[start..start + 8]
                    .try_into()
                    .map_err(|_| Error::Invalid)?,
            ))
        };
        let observation = ExecutionObservation {
            transfer_id: array(8)?,
            attempt_id: array(40)?,
            observation_id: array(72)?,
            epoch: number(136)?,
            observed_at: number(144)?,
            expires_at: number(152)?,
            kind: match wire[160] {
                1 => ObservationKind::Timeout,
                2 => ObservationKind::ReportedDelivery,
                3 => ObservationKind::ReportedNoDelivery,
                _ => return Err(Error::Invalid),
            },
        };
        Self::validate_shape(&observation)?;
        let result = Self {
            observation,
            reporter: IdentityId::from_bytes(array(104)?).map_err(|_| Error::Invalid)?,
            signature: wire[161..].try_into().map_err(|_| Error::Invalid)?,
        };
        result.verify()?;
        Ok(result)
    }
    fn verify(&self) -> Result<()> {
        Self::validate_shape(&self.observation)?;
        VerifyingKey::from_bytes(self.reporter.as_bytes())
            .map_err(|_| Error::Invalid)?
            .verify_strict(&self.statement(), &Signature::from_bytes(&self.signature))
            .map_err(|_| Error::Invalid)
    }
}
pub(super) fn schema(tx: &rusqlite::Transaction<'_>) -> Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS recovery_subjects(
        transfer BLOB PRIMARY KEY, attempt BLOB UNIQUE NOT NULL, peer BLOB NOT NULL, epoch INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS recovery_observations(
        observation BLOB PRIMARY KEY, transfer BLOB NOT NULL, frame BLOB NOT NULL);
        CREATE TABLE IF NOT EXISTS recovery_conflicts(
        observation BLOB NOT NULL, frame BLOB NOT NULL, PRIMARY KEY(observation,frame));")?;
    Ok(())
}
impl Simulator {
    /// The authenticated transport identity must come from the QR/Iroh handshake.
    /// Financial scope is pinned by the admission commit, never by the report.
    /// Every accepted report leaves the intent uncertain; none permits retry,
    /// release, settlement, or a delivered-cash assertion.
    pub fn record_observation(
        &self,
        signed: &SignedObservation,
        authenticated_peer: IdentityId,
        now: u64,
    ) -> Result<ObservationOutcome> {
        if signed.verify().is_err() || signed.reporter != authenticated_peer {
            return Ok(ObservationOutcome::Rejected);
        }
        let o = signed.observation;
        let frame = signed.encode();
        let mut db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        schema(&tx)?;
        let subject: Option<(Vec<u8>, Vec<u8>, i64)> = tx
            .query_row(
                "SELECT attempt,peer,epoch FROM recovery_subjects WHERE transfer=?1",
                params![o.transfer_id.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((attempt, peer, epoch)) = subject else {
            return Ok(ObservationOutcome::Rejected);
        };
        if attempt != o.attempt_id
            || peer != signed.reporter.as_bytes()
            || epoch != i64_amount(o.epoch)?
        {
            return Ok(ObservationOutcome::Rejected);
        }
        let prior: Option<Vec<u8>> = tx
            .query_row(
                "SELECT frame FROM recovery_observations WHERE observation=?1",
                params![o.observation_id.as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        // Only a byte-for-byte retrieval of already accepted evidence may
        // bypass freshness, including a previously retained conflicting frame.
        let known_primary = prior.as_ref().is_some_and(|old| old == &frame);
        let known_conflict: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM recovery_conflicts WHERE observation=?1 AND frame=?2)",
            params![o.observation_id.as_slice(), frame.as_slice()],
            |r| r.get(0),
        )?;
        if !known_primary && !known_conflict && (now < o.observed_at || now > o.expires_at) {
            return Ok(ObservationOutcome::Rejected);
        }
        let outcome = match prior {
            Some(old) if old == frame => ObservationOutcome::IdenticalReplayHeld,
            Some(_) => {
                tx.execute(
                    "INSERT OR IGNORE INTO recovery_conflicts VALUES(?1,?2)",
                    params![o.observation_id.as_slice(), frame],
                )?;
                ObservationOutcome::ConflictHeld
            }
            None => {
                tx.execute(
                    "INSERT INTO recovery_observations VALUES(?1,?2,?3)",
                    params![o.observation_id.as_slice(), o.transfer_id.as_slice(), frame],
                )?;
                ObservationOutcome::RecordedHeld
            }
        };
        if tx.execute(
            "UPDATE admission_receipts SET state='uncertain' WHERE transfer=?1",
            params![o.transfer_id.as_slice()],
        )? != 1
        {
            return Err(Error::Invalid);
        }
        tx.commit()?;
        Ok(outcome)
    }
}
