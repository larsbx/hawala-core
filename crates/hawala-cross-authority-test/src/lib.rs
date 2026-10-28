//! TEST-ONLY host adapter for the canonical finite protocol. No wire signatures,
//! assets, financial append, execution, custody, ports or production entry point.
//! Three independent SQLite journals; no cross-authority database transaction.
#![cfg(feature = "conformance")]
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const PROFILE: &str = "hawala/TEST-cross-authority-host/v1";
pub const FIELDS: [&str; 24] = [
    "requested",
    "requestEpoch",
    "voteL",
    "voteR",
    "seenL",
    "seenR",
    "decision",
    "wire",
    "wireEpoch",
    "localL",
    "localR",
    "upC",
    "upL",
    "upR",
    "netL",
    "netR",
    "epoch",
    "installedL",
    "installedR",
    "recoveryWire",
    "fenceC",
    "fenceL",
    "fenceR",
    "recovered",
];
const C: &[usize] = &[0, 1, 4, 5, 6, 7, 8, 16, 19, 20, 23];
const L: &[usize] = &[2, 9, 17, 21];
const R: &[usize] = &[3, 10, 18, 22];
const ENV: &[usize] = &[11, 12, 13, 14, 15];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Projection(pub [u64; 24]);
impl Default for Projection {
    fn default() -> Self {
        let mut s = Self([0; 24]);
        for i in ENV {
            s.0[*i] = 1;
        }
        s
    }
}
impl Projection {
    pub fn fields(&self) -> BTreeMap<&'static str, u64> {
        FIELDS
            .iter()
            .enumerate()
            .map(|(i, k)| (*k, self.0[i]))
            .collect()
    }
    pub fn safe(&self) -> bool {
        let s = &self.0;
        [2, 3, 6, 7, 9, 10].iter().all(|i| s[*i] <= 2)
            && (s[4] == 0 || s[2] == 1)
            && (s[5] == 0 || s[3] == 1)
            && (s[6] != 1 || (s[4] == 1 && s[5] == 1))
            && [7, 9, 10].iter().all(|i| s[*i] == 0 || s[*i] == s[6])
            && [9, 10].iter().all(|i| s[*i] == 0 || s[*i] == s[7])
    }
    pub fn held(&self, left: bool) -> bool {
        let (v, d) = if left { (2, 9) } else { (3, 10) };
        self.0[v] == 1 && self.0[d] == 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    name: String,
    args: Vec<u64>,
}
impl Event {
    pub fn parse(text: &str) -> Result<Self> {
        let (name, args) = if let Some((name, rest)) = text.split_once('(') {
            let body = rest.strip_suffix(')').ok_or("bad event")?;
            let args = body
                .split(',')
                .map(|v| {
                    let n = v.parse::<u64>().map_err(|_| "bad integer")?;
                    if n.to_string() != v {
                        return Err("noncanonical integer");
                    }
                    Ok(n)
                })
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if !matches!(name, "prepareL" | "prepareR") || args.len() != 3 {
                return Err("unsupported parameterized event".into());
            }
            (name, args)
        } else {
            (text, Vec::new())
        };
        if !matches!(
            name,
            "prepareL"
                | "prepareR"
                | "sendPrepare"
                | "refuseL"
                | "refuseR"
                | "receiveL"
                | "receiveR"
                | "commit"
                | "abort"
                | "publish"
                | "deliverL"
                | "deliverR"
                | "foreignAttempt"
                | "foreignAuthority"
                | "crashC"
                | "restartC"
                | "crashL"
                | "restartL"
                | "crashR"
                | "restartR"
                | "partitionL"
                | "healL"
                | "partitionR"
                | "healR"
                | "timeout"
                | "fenceCoordinator"
                | "fenceLeft"
                | "fenceRight"
                | "recoverLog"
                | "publishRecovery"
                | "installL"
                | "installR"
                | "activateL"
                | "activateR"
                | "recover"
        ) || (matches!(name, "prepareL" | "prepareR") && args.len() != 3)
        {
            return Err("unknown event".into());
        }
        Ok(Self {
            name: name.into(),
            args,
        })
    }
    fn role(&self) -> Option<&'static str> {
        match self.name.as_str() {
            "prepareL" | "refuseL" | "deliverL" | "foreignAttempt" | "foreignAuthority"
            | "fenceLeft" | "installL" | "activateL" => Some("left"),
            "prepareR" | "refuseR" | "deliverR" | "fenceRight" | "installR" | "activateR" => {
                Some("right")
            }
            "crashC" | "restartC" | "crashL" | "restartL" | "crashR" | "restartR"
            | "partitionL" | "healL" | "partitionR" | "healR" => None,
            _ => Some("coordinator"),
        }
    }
}

// Local fields only. Never serialize the other authority's durable state here.
fn indices(role: &str) -> &'static [usize] {
    match role {
        "coordinator" => C,
        "left" => L,
        "right" => R,
        _ => unreachable!(),
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub replayed: bool,
    pub version: u64,
    pub local_state: Vec<u64>,
}
struct Journal {
    path: PathBuf,
    role: &'static str,
}
impl Journal {
    fn open(path: PathBuf, role: &'static str, create: bool) -> Result<Self> {
        let j = Self { path, role };
        let db = j.connect(create)?;
        if create {
            db.execute_batch("CREATE TABLE state (id INTEGER PRIMARY KEY CHECK(id=1), profile TEXT NOT NULL, role TEXT NOT NULL, version INTEGER NOT NULL, body TEXT NOT NULL); CREATE TABLE receipts (id TEXT PRIMARY KEY, command TEXT NOT NULL, version INTEGER NOT NULL, body TEXT NOT NULL);")?;
            db.execute(
                "INSERT INTO state VALUES (1,?1,?2,0,?3)",
                params![
                    PROFILE,
                    role,
                    serde_json::to_string(&vec![0u64; indices(role).len()])?
                ],
            )?;
        }
        let found: (String, String) =
            db.query_row("SELECT profile,role FROM state WHERE id=1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        if found != (PROFILE.into(), role.into()) {
            return Err("HOLD: wrong journal enrollment".into());
        }
        let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err("HOLD: corrupt journal".into());
        }
        j.snapshot()?;
        Ok(j)
    }
    fn connect(&self, create: bool) -> Result<Connection> {
        let name = self.path.to_str().ok_or("invalid path")?;
        if name.is_empty() || name.contains(":memory:") || name.starts_with("file:") {
            return Err("ordinary durable path required".into());
        }
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | if create {
                OpenFlags::SQLITE_OPEN_CREATE
            } else {
                OpenFlags::empty()
            };
        let db = Connection::open_with_flags(&self.path, flags)?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "FULL")?;
        Ok(db)
    }
    fn snapshot(&self) -> Result<(u64, Vec<u64>)> {
        let db = self.connect(false)?;
        let (v, text): (u64, String) =
            db.query_row("SELECT version,body FROM state WHERE id=1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        let values: Vec<u64> = serde_json::from_str(&text)?;
        if values.len() != indices(self.role).len() {
            return Err("HOLD: wrong local schema".into());
        }
        for (&field, &value) in indices(self.role).iter().zip(&values) {
            let max = if [2, 3, 6, 7, 9, 10].contains(&field) {
                2
            } else {
                1
            };
            if value > max {
                return Err("HOLD: invalid local state".into());
            }
        }
        Ok((v, values))
    }
    fn apply(&self, id: &str, body: &str, expected: u64, values: Vec<u64>) -> Result<Receipt> {
        if id.is_empty() || id.len() > 128 || !id.is_ascii() {
            return Err("invalid request ID".into());
        }
        let mut db = self.connect(false)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<(String, u64, String)> = tx
            .query_row(
                "SELECT command,version,body FROM receipts WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((prior, v, text)) = previous {
            if prior != body {
                return Err("request ID payload conflict".into());
            }
            return Ok(Receipt {
                replayed: true,
                version: v,
                local_state: serde_json::from_str(&text)?,
            });
        }
        let version: u64 =
            tx.query_row("SELECT version FROM state WHERE id=1", [], |r| r.get(0))?;
        if version != expected {
            return Err("HOLD: stale local version".into());
        }
        let next = version
            .checked_add(1)
            .filter(|v| *v <= i64::MAX as u64)
            .ok_or("version overflow")?;
        let encoded = serde_json::to_string(&values)?;
        tx.execute(
            "UPDATE state SET version=?1,body=?2 WHERE id=1",
            params![next, encoded],
        )?;
        tx.execute(
            "INSERT INTO receipts VALUES (?1,?2,?3,?4)",
            params![id, body, next, encoded],
        )?;
        tx.commit()?;
        Ok(Receipt {
            replayed: false,
            version: next,
            local_state: values,
        })
    }
}

pub struct TestHost {
    root: PathBuf,
    c: Journal,
    l: Journal,
    r: Journal,
    environment: [u64; 5],
}
impl TestHost {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        std::fs::create_dir(path.as_ref())?;
        Self::open(path.as_ref(), true)
    }
    pub fn open_existing(path: impl AsRef<Path>) -> Result<Self> {
        Self::open(path.as_ref(), false)
    }
    fn open(root: &Path, create: bool) -> Result<Self> {
        let root = root.to_path_buf();
        let host = Self {
            c: Journal::open(root.join("coordinator.db"), "coordinator", create)?,
            l: Journal::open(root.join("left.db"), "left", create)?,
            r: Journal::open(root.join("right.db"), "right", create)?,
            root,
            environment: [1; 5],
        };
        host.projection()?;
        Ok(host)
    }
    pub fn reopen(&mut self) -> Result<()> {
        // Restore durable actors; preserve the test driver's volatile fault schedule.
        self.c = Journal::open(self.root.join("coordinator.db"), "coordinator", false)?;
        self.l = Journal::open(self.root.join("left.db"), "left", false)?;
        self.r = Journal::open(self.root.join("right.db"), "right", false)?;
        Ok(())
    }
    pub fn projection(&self) -> Result<Projection> {
        Ok(self.snapshot_all()?.0)
    }
    fn snapshot_all(&self) -> Result<(Projection, BTreeMap<&'static str, u64>)> {
        let mut s = Projection::default();
        let mut versions = BTreeMap::new();
        for j in [&self.c, &self.l, &self.r] {
            let (version, local) = j.snapshot()?;
            versions.insert(j.role, version);
            for (&i, &v) in indices(j.role).iter().zip(local.iter()) {
                s.0[i] = v;
            }
        }
        for (&i, &v) in ENV.iter().zip(&self.environment) {
            s.0[i] = v;
        }
        if !s.safe() {
            return Err("HOLD: conflicting durable history".into());
        }
        Ok((s, versions))
    }
    pub fn apply(&mut self, id: &str, text: &str) -> Result<Option<Receipt>> {
        let e = Event::parse(text)?;
        let (before, versions) = self.snapshot_all()?;
        let mut after = before.clone();
        reduce(&mut after, &e);
        let receipt = if let Some(role) = e.role() {
            let j = match role {
                "coordinator" => &self.c,
                "left" => &self.l,
                _ => &self.r,
            };
            let version = versions[role];
            Some(j.apply(
                id,
                text,
                version,
                indices(role).iter().map(|i| after.0[*i]).collect(),
            )?)
        } else {
            None
        };
        if !receipt.as_ref().is_some_and(|r| r.replayed) {
            for (index, &i) in ENV.iter().enumerate() {
                self.environment[index] = after.0[i];
            }
        }
        if matches!(e.name.as_str(), "restartC" | "restartL" | "restartR") {
            self.reopen()?;
        }
        self.projection()?;
        Ok(receipt)
    }
}
fn reduce(p: &mut Projection, e: &Event) {
    let s = &mut p.0;
    match e.name.as_str() {
        "prepareL" | "prepareR" => {
            let (v, d, u, n, f) = if e.name == "prepareL" {
                (2, 9, 12, 14, 21)
            } else {
                (3, 10, 13, 15, 22)
            };
            if e.args[0] == 0
                && e.args[1] == 0
                && e.args[2] == s[16]
                && s[0] == 1
                && e.args[2] == s[1]
                && s[u] == 1
                && s[n] == 1
                && s[f] == 0
                && s[d] == 0
                && s[v] == 0
            {
                s[v] = 1;
            }
        }
        "sendPrepare" => {
            if s[11] == 1 && s[20] == 0 && s[6] == 0 {
                s[0] = 1;
                s[1] = s[16];
            }
        }
        "refuseL" => {
            if s[12] == 1 && s[2] == 0 {
                s[2] = 2;
            }
        }
        "refuseR" => {
            if s[13] == 1 && s[3] == 0 {
                s[3] = 2;
            }
        }
        "receiveL" => {
            if s[11] == 1 && s[14] == 1 && s[2] == 1 {
                s[4] = 1;
            }
        }
        "receiveR" => {
            if s[11] == 1 && s[15] == 1 && s[3] == 1 {
                s[5] = 1;
            }
        }
        "commit" => {
            if s[11] == 1 && s[20] == 0 && s[6] == 0 && s[4] == 1 && s[5] == 1 {
                s[6] = 1;
            }
        }
        "abort" => {
            if s[11] == 1 && s[20] == 0 && s[6] == 0 {
                s[6] = 2;
            }
        }
        "publish" => {
            if s[11] == 1 && s[20] == 0 && s[7] == 0 && s[6] != 0 {
                s[7] = s[6];
                s[8] = s[16];
            }
        }
        "deliverL" => {
            if s[12] == 1 && s[14] == 1 && s[7] != 0 && s[8] <= s[16] {
                s[9] = s[7];
            }
        }
        "deliverR" => {
            if s[13] == 1 && s[15] == 1 && s[7] != 0 && s[8] <= s[16] {
                s[10] = s[7];
            }
        }
        "foreignAttempt" | "foreignAuthority" | "timeout" => {}
        "crashC" => s[11] = 0,
        "restartC" => {
            if s[20] == 0 {
                s[11] = 1;
            }
        }
        "crashL" => s[12] = 0,
        "restartL" => s[12] = 1,
        "crashR" => s[13] = 0,
        "restartR" => s[13] = 1,
        "partitionL" => s[14] = 0,
        "healL" => s[14] = 1,
        "partitionR" => s[15] = 0,
        "healR" => s[15] = 1,
        "fenceCoordinator" => {
            s[20] = 1;
            s[11] = 0;
            s[23] = 0;
        }
        "fenceLeft" => {
            if s[20] == 1 && s[12] == 1 && s[14] == 1 {
                s[21] = 1;
            }
        }
        "fenceRight" => {
            if s[20] == 1 && s[13] == 1 && s[15] == 1 {
                s[22] = 1;
            }
        }
        "recoverLog" => {
            if s[20] == 1 {
                s[23] = 1;
            }
        }
        "publishRecovery" => {
            if s[20] == 1 && s[21] == 1 && s[22] == 1 && s[23] == 1 {
                s[19] = 1;
            }
        }
        "installL" => {
            if s[19] == 1 && s[12] == 1 && s[14] == 1 {
                s[17] = 1;
            }
        }
        "installR" => {
            if s[19] == 1 && s[13] == 1 && s[15] == 1 {
                s[18] = 1;
            }
        }
        "recover" => {
            if s[20] == 1
                && s[21] == 1
                && s[22] == 1
                && s[23] == 1
                && s[17] == 1
                && s[18] == 1
                && s[16] == 0
            {
                s[16] = 1;
                s[11] = 1;
                s[20] = 0;
            }
        }
        "activateL" => {
            if s[16] == 1 && s[17] == 1 && s[12] == 1 && s[14] == 1 {
                s[21] = 0;
            }
        }
        "activateR" => {
            if s[16] == 1 && s[18] == 1 && s[13] == 1 && s[15] == 1 {
                s[22] = 0;
            }
        }
        _ => unreachable!("validated event alphabet"),
    }
}

/// Evidence SHAPE gate only: verification is an external assumption in this lab.
pub fn test_workflow_gate(verified: bool, stage: &str, action: &str) -> Result<bool> {
    if ![
        "proposed",
        "committed",
        "executed",
        "settled",
        "held",
        "aborted",
    ]
    .contains(&stage)
        || ![
            "recordProposal",
            "reserveCancelableOrder",
            "checkoutFulfill",
            "dispatchPaid",
            "groupBuyRelease",
            "remittanceDelivered",
        ]
        .contains(&action)
    {
        return Err("unknown workflow input".into());
    }
    Ok(verified
        && !matches!(stage, "held" | "aborted")
        && (action == "recordProposal"
            || stage == "settled"
            || (action == "reserveCancelableOrder" && matches!(stage, "committed" | "executed"))))
}
