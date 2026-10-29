//! Atomic local durable evidence and independently keyed signed-outcome sinks.
//! SQLite is downstream of TB. Retaining a signed intent is not financial commit.
use crate::{
    accounting::Registry,
    evidence::{outcome_bytes, plan_bytes, Admission},
};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hawala_tigerbeetle_flow::{Confirmed, DurableSink, Error, Plan, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::{path::Path, time::Duration};

fn database(path: &Path) -> Result<Connection> {
    if path.as_os_str() == ":memory:" {
        return Err(Error::Port);
    }
    let db = Connection::open(path).map_err(|_| Error::Port)?;
    db.busy_timeout(Duration::from_millis(50))
        .map_err(|_| Error::Port)?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")
        .map_err(|_| Error::Port)?;
    Ok(db)
}
fn context(
    registry: &Registry,
    cluster: u128,
    generation: u64,
    key: &VerifyingKey,
) -> Result<Vec<u8>> {
    if generation == 0 {
        return Err(Error::InvalidPlan);
    }
    let mut bytes = registry.digest()?.to_vec();
    bytes.extend(cluster.to_be_bytes());
    bytes.extend(generation.to_be_bytes());
    bytes.extend(key.to_bytes());
    Ok(bytes)
}
fn pin(db: &Connection, expected: &[u8]) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS profile(id INTEGER PRIMARY KEY CHECK(id=1), bytes BLOB NOT NULL);").map_err(|_|Error::Port)?;
    db.execute("INSERT OR IGNORE INTO profile VALUES(1,?1)", [expected])
        .map_err(|_| Error::Port)?;
    let actual: Vec<u8> = db
        .query_row("SELECT bytes FROM profile WHERE id=1", [], |r| r.get(0))
        .map_err(|_| Error::Port)?;
    if actual != expected {
        return Err(Error::Conflict);
    }
    Ok(())
}

pub struct EvidenceStore {
    db: Connection,
    registry: Registry,
    key: VerifyingKey,
    cluster: u128,
    generation: u64,
}
impl EvidenceStore {
    pub fn open(
        path: &Path,
        registry: Registry,
        cluster: u128,
        generation: u64,
        key: VerifyingKey,
    ) -> Result<Self> {
        let db = database(path)?;
        pin(&db, &context(&registry, cluster, generation, &key)?)?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS admissions(operation BLOB PRIMARY KEY,bytes BLOB NOT NULL,signature BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS reservations(transfer BLOB PRIMARY KEY,operation BLOB NOT NULL REFERENCES admissions(operation));")
            .map_err(|_|Error::Port)?;
        Ok(Self {
            db,
            registry,
            key,
            cluster,
            generation,
        })
    }
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
    pub fn context(&self) -> (u128, u64) {
        (self.cluster, self.generation)
    }
    pub fn retain(&mut self, admission: &Admission) -> Result<Plan> {
        let plan = admission.verify(&self.registry, &self.key)?;
        if (plan.cluster(), plan.generation()) != self.context() {
            return Err(Error::Conflict);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| Error::Port)?;
        let id = plan.operation().to_be_bytes();
        let old: Option<(Vec<u8>, Vec<u8>)> = tx
            .query_row(
                "SELECT bytes,signature FROM admissions WHERE operation=?1",
                [id.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|_| Error::Port)?;
        if let Some((bytes, sig)) = old {
            if bytes != admission.bytes || sig != admission.signature {
                return Err(Error::Conflict);
            }
        } else {
            tx.execute(
                "INSERT INTO admissions VALUES(?1,?2,?3)",
                params![
                    id.as_slice(),
                    admission.bytes,
                    admission.signature.as_slice()
                ],
            )
            .map_err(|_| Error::Port)?;
            for t in plan.transfers() {
                tx.execute(
                    "INSERT INTO reservations VALUES(?1,?2)",
                    params![t.id.to_be_bytes().as_slice(), id.as_slice()],
                )
                .map_err(|_| Error::Conflict)?;
            }
        }
        tx.commit().map_err(|_| Error::Port)?;
        Ok(plan)
    }
    pub fn load(&self, operation: u128) -> Result<(Plan, Admission)> {
        let (bytes, sig): (Vec<u8>, Vec<u8>) = self
            .db
            .query_row(
                "SELECT bytes,signature FROM admissions WHERE operation=?1",
                [operation.to_be_bytes().as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| Error::Conflict)?;
        let admission = Admission {
            bytes,
            signature: sig.try_into().map_err(|_| Error::Conflict)?,
        };
        let plan = admission.verify(&self.registry, &self.key)?;
        if plan.operation() != operation || (plan.cluster(), plan.generation()) != self.context() {
            return Err(Error::Conflict);
        }
        Ok((plan, admission))
    }
    pub fn require(&self, plan: &Plan) -> Result<Admission> {
        let (retained, a) = self.load(plan.operation())?;
        if retained != *plan {
            return Err(Error::Conflict);
        }
        Ok(a)
    }
}

pub struct SignedSink {
    db: Connection,
    evidence: EvidenceStore,
    key: SigningKey,
    destination: [u8; 32],
}
impl SignedSink {
    pub fn open(
        path: &Path,
        evidence: EvidenceStore,
        destination: [u8; 32],
        key: SigningKey,
    ) -> Result<Self> {
        if destination == [0; 32] {
            return Err(Error::InvalidPlan);
        }
        let db = database(path)?;
        let (cluster, generation) = evidence.context();
        let mut expected = context(
            evidence.registry(),
            cluster,
            generation,
            &key.verifying_key(),
        )?;
        expected.extend(destination);
        pin(&db, &expected)?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS outcomes(operation BLOB PRIMARY KEY,previous BLOB NOT NULL,last BLOB UNIQUE NOT NULL,bytes BLOB NOT NULL,signature BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS cursor(id INTEGER PRIMARY KEY CHECK(id=1),last BLOB NOT NULL);
            INSERT OR IGNORE INTO cursor VALUES(1,x'0000000000000000');").map_err(|_|Error::Port)?;
        let sink = Self {
            db,
            evidence,
            key,
            destination,
        };
        sink.audit()?;
        Ok(sink)
    }
    fn cursor(&self) -> Result<u64> {
        let b: Vec<u8> = self
            .db
            .query_row("SELECT last FROM cursor WHERE id=1", [], |r| r.get(0))
            .map_err(|_| Error::Port)?;
        Ok(u64::from_be_bytes(
            b.try_into().map_err(|_| Error::Conflict)?,
        ))
    }
    /// Verify every retained signed outcome and prefix link against the durable
    /// cursor. A rollback of the entire DB to an older valid prefix needs an
    /// external checkpoint and is outside this local conformance destination.
    pub fn audit(&self) -> Result<usize> {
        let mut q = self
            .db
            .prepare("SELECT operation,previous,last,bytes,signature FROM outcomes ORDER BY last")
            .map_err(|_| Error::Port)?;
        let mut rows = q.query([]).map_err(|_| Error::Port)?;
        let mut cursor = 0u64;
        let mut count = 0;
        while let Some(row) = rows.next().map_err(|_| Error::Port)? {
            let id: Vec<u8> = row.get(0).map_err(|_| Error::Port)?;
            let previous: Vec<u8> = row.get(1).map_err(|_| Error::Port)?;
            let last: Vec<u8> = row.get(2).map_err(|_| Error::Port)?;
            let bytes: Vec<u8> = row.get(3).map_err(|_| Error::Port)?;
            let sig: Vec<u8> = row.get(4).map_err(|_| Error::Port)?;
            let previous = u64::from_be_bytes(previous.try_into().map_err(|_| Error::Conflict)?);
            let last = u64::from_be_bytes(last.try_into().map_err(|_| Error::Conflict)?);
            let operation = u128::from_be_bytes(id.try_into().map_err(|_| Error::Conflict)?);
            let (_, admission) = self.evidence.load(operation)?;
            self.key
                .verifying_key()
                .verify_strict(
                    &bytes,
                    &Signature::from_slice(&sig).map_err(|_| Error::Conflict)?,
                )
                .map_err(|_| Error::Conflict)?;
            // Signatures also bind SQL identity, cursor columns and admission.
            let group = decode_outcome(&bytes, &admission, self.destination, previous)?;
            if group.plan().operation() != operation
                || group.last_timestamp() != last
                || previous != cursor
                || group.first_timestamp() <= cursor
            {
                return Err(Error::Conflict);
            }
            cursor = last;
            count += 1;
        }
        if cursor != self.cursor()? {
            return Err(Error::Conflict);
        }
        Ok(count)
    }
}
impl DurableSink for SignedSink {
    fn watermark(&mut self, cluster: u128, generation: u64) -> Result<u64> {
        if (cluster, generation) != self.evidence.context() {
            return Err(Error::Conflict);
        }
        self.audit()?;
        self.cursor()
    }
    fn commit(&mut self, expected: u64, group: &Confirmed) -> Result<()> {
        let admission = self.evidence.require(group.plan())?;
        let bytes = outcome_bytes(group, &admission, &self.destination, expected);
        let signature = self.key.sign(&bytes).to_bytes();
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| Error::Port)?;
        let op = group.plan().operation().to_be_bytes();
        let old: Option<(Vec<u8>, Vec<u8>)> = tx
            .query_row(
                "SELECT bytes,signature FROM outcomes WHERE operation=?1",
                [op.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|_| Error::Port)?;
        if let Some((b, s)) = old {
            if b != bytes || s != signature {
                return Err(Error::Conflict);
            }
            return Ok(());
        }
        let cursor: Vec<u8> = tx
            .query_row("SELECT last FROM cursor WHERE id=1", [], |r| r.get(0))
            .map_err(|_| Error::Port)?;
        if cursor != expected.to_be_bytes() || group.first_timestamp() <= expected {
            return Err(Error::Conflict);
        }
        tx.execute(
            "INSERT INTO outcomes VALUES(?1,?2,?3,?4,?5)",
            params![
                op.as_slice(),
                expected.to_be_bytes().as_slice(),
                group.last_timestamp().to_be_bytes().as_slice(),
                bytes,
                signature.as_slice()
            ],
        )
        .map_err(|_| Error::Port)?;
        tx.execute(
            "UPDATE cursor SET last=?1 WHERE id=1",
            [group.last_timestamp().to_be_bytes().as_slice()],
        )
        .map_err(|_| Error::Port)?;
        tx.commit().map_err(|_| Error::Port)
    }
}
fn decode_outcome(
    bytes: &[u8],
    a: &Admission,
    destination: [u8; 32],
    expected: u64,
) -> Result<Confirmed> {
    use crate::evidence::{parse_plan, Reader};
    use hawala_tigerbeetle_flow::RecordedTransfer;
    let mut r = Reader::new(bytes, b"hawala-tb-first-test-outcome/v1\0")?;
    if r.take(32)? != destination || r.u64()? != expected {
        return Err(Error::Conflict);
    }
    let n = r.u32()? as usize;
    if r.take(n)? != a.bytes || r.take(64)? != a.signature {
        return Err(Error::Conflict);
    }
    let n = r.u32()? as usize;
    let plan = parse_plan(r.take(n)?)?;
    // Retained signed plan must be exactly what this signed outcome records.
    let mut ar = Reader::new(&a.bytes, b"hawala-tb-first-test-admission/v1\0")?;
    ar.take(32)?;
    let n = ar.u32()? as usize;
    if ar.take(n)? != plan_bytes(&plan) {
        return Err(Error::Conflict);
    }
    let mut records = Vec::new();
    for t in plan.transfers() {
        records.push(RecordedTransfer {
            transfer: t.clone(),
            timestamp: r.u64()?,
        });
    }
    r.finish()?;
    Confirmed::from_records(plan, records)
}
