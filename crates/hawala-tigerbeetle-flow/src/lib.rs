//! Financial commit in TigerBeetle, followed by independent durable consumers.
//!
//! This transport-independent core has no database/network implementation and
//! grants no financial or execution authority. Trusted ports must implement the
//! security, completeness and durability contracts below. In particular, a
//! supplied `Plan` is not evidence of consent or permission to submit it.
use std::collections::BTreeSet;

pub mod replay;

pub const MAX_LEGS: usize = 255;
pub const MAX_AMOUNT: u128 = i64::MAX as u128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidPlan,
    Conflict,
    InvalidPage,
    Port,
}
pub type Result<T> = std::result::Result<T, Error>;

/// The supported financial fast path is ordinary posted transfers only. Native
/// pending/expiry, balancing, imported and closing flags are deliberately absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transfer {
    pub id: u128,
    pub debit_account_id: u128,
    pub credit_account_id: u128,
    pub amount: u128,
    pub ledger: u32,
    pub code: u16,
    pub user_data_128: u128,
    pub user_data_64: u64,
    pub user_data_32: u32,
}

/// Registration maps the full business identities injectively to these native
/// IDs before use. No hash truncation or new mapping allocation happens here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    cluster: u128,
    generation: u64,
    operation: u128,
    epoch: u64,
    transfers: Vec<Transfer>,
}

impl Plan {
    pub fn new(
        cluster: u128,
        generation: u64,
        operation: u128,
        epoch: u64,
        transfers: Vec<Transfer>,
    ) -> Result<Self> {
        if generation == 0
            || !valid_id(operation)
            || epoch == 0
            || transfers.is_empty()
            || transfers.len() > MAX_LEGS
        {
            return Err(Error::InvalidPlan);
        }
        // A stable anchor is part of every chain. user_data is not unique in
        // TigerBeetle; by itself it cannot prevent two financial groups for one
        // operation. Reuse this first transfer ID for every retry of the group.
        if transfers[0].id != operation {
            return Err(Error::InvalidPlan);
        }
        let mut ids = BTreeSet::new();
        for (i, t) in transfers.iter().enumerate() {
            if !valid_id(t.id)
                || !ids.insert(t.id)
                || !valid_id(t.debit_account_id)
                || !valid_id(t.credit_account_id)
                || t.debit_account_id == t.credit_account_id
                || t.amount == 0
                || t.amount > MAX_AMOUNT
                || t.ledger == 0
                || t.code == 0
                || t.user_data_128 != operation
                || t.user_data_64 != epoch
                || t.user_data_32 != leg_tag(transfers.len(), i)
            {
                return Err(Error::InvalidPlan);
            }
        }
        Ok(Self {
            cluster,
            generation,
            operation,
            epoch,
            transfers,
        })
    }

    pub fn cluster(&self) -> u128 {
        self.cluster
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn operation(&self) -> u128 {
        self.operation
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn transfers(&self) -> &[Transfer] {
        &self.transfers
    }

    /// Exactly one complete chain. The adapter must not split it into requests.
    /// TigerBeetle does not retain `linked`; group/count/index survive in data.
    pub fn linked_transfers(&self) -> impl Iterator<Item = (&Transfer, bool)> {
        self.transfers
            .iter()
            .enumerate()
            .map(|(i, t)| (t, i + 1 < self.transfers.len()))
    }
}

fn valid_id(id: u128) -> bool {
    id != 0 && id != u128::MAX
}

/// High 16 bits = total transfer count; low 16 bits = zero-based index.
pub fn leg_tag(count: usize, index: usize) -> u32 {
    ((count as u32) << 16) | index as u32
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedTransfer {
    pub transfer: Transfer,
    pub timestamp: u64,
}

/// Constructible only after complete matching records from the trusted port.
/// This confirms accounting, not physical execution, consent or settlement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirmed {
    plan: Plan,
    records: Vec<RecordedTransfer>,
}
impl Confirmed {
    /// Structural verification for a trusted native-history adapter. This is
    /// NOT authentication: callers must establish trusted TigerBeetle provenance.
    pub fn from_records(plan: Plan, records: Vec<RecordedTransfer>) -> Result<Self> {
        confirm(&plan, records)
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }
    pub fn records(&self) -> &[RecordedTransfer] {
        &self.records
    }
    pub fn first_timestamp(&self) -> u64 {
        self.records[0].timestamp
    }
    pub fn last_timestamp(&self) -> u64 {
        self.records[self.records.len() - 1].timestamp
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Submission {
    /// Actual accepted records, normalized to input order by the trusted port.
    Accepted(Vec<RecordedTransfer>),
    /// Proven no financial effect for this original operation, including any
    /// earlier uncertain submission. Never a timeout or merely a failed local
    /// reauthorization of an operation that might already be committed.
    Rejected,
    /// Lost reply, interrupted session, or otherwise unresolved outcome.
    Unknown,
}

/// The adapter pins cluster/generation and enforces current authentication,
/// consent, policy and *effective* writer fencing before submitting. Compile any
/// spend-critical nonce/lot guards into the SAME linked chain, or refuse the
/// operation. Return no accepted record for an unchecked `exists` response.
/// Financial balance decisions belong to TigerBeetle, not a SQL projection.
pub trait FinancialPort {
    fn submit_linked(&mut self, plan: &Plan) -> Result<Submission>;
    /// A linearizable lookup of every original ID, normalized to plan order.
    /// Absence is not proof an interrupted request cannot still commit.
    fn lookup(&mut self, plan: &Plan) -> Result<Vec<Option<RecordedTransfer>>>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Committed(Confirmed),
    Rejected,
    Held,
}

/// The entire synchronous path: this function has NO EventStore/SQL/sink port.
/// Port errors are unresolved, never permission to claim rejection or retry with
/// another ID. The caller retains the original plan and invokes recovery.
pub fn record(port: &mut impl FinancialPort, plan: &Plan) -> Result<Decision> {
    match port.submit_linked(plan)? {
        Submission::Accepted(records) => Ok(Decision::Committed(confirm(plan, records)?)),
        Submission::Rejected => Ok(Decision::Rejected),
        Submission::Unknown => Ok(Decision::Held),
    }
}

/// Retry ONLY the same immutable plan. No replacement ID and no absent=>abort.
pub fn recover(port: &mut impl FinancialPort, plan: &Plan) -> Result<Decision> {
    let found = port.lookup(plan)?;
    if found.len() != plan.transfers.len() {
        return Err(Error::Conflict);
    }
    let present = found.iter().filter(|r| r.is_some()).count();
    if present == found.len() {
        return Ok(Decision::Committed(confirm(
            plan,
            found.into_iter().flatten().collect(),
        )?));
    }
    if present != 0 {
        return Err(Error::Conflict);
    }
    // A previously sent request may still be in flight; same-ID submission
    // resolves it without allowing a second financial operation.
    record(port, plan)
}

fn confirm(plan: &Plan, records: Vec<RecordedTransfer>) -> Result<Confirmed> {
    if records.len() != plan.transfers.len() {
        return Err(Error::Conflict);
    }
    let mut previous = 0;
    for (actual, expected) in records.iter().zip(&plan.transfers) {
        if actual.transfer != *expected || actual.timestamp <= previous {
            return Err(Error::Conflict);
        }
        previous = actual.timestamp;
    }
    Ok(Confirmed {
        plan: plan.clone(),
        records,
    })
}

/// Trusted replay adapter over TigerBeetle, optionally using CDC as transport.
/// Must recover *complete* groups from persisted group/count/index metadata and
/// native records, including account registry, cluster and epoch context. A CDC
/// message alone is not an authenticated, complete financial journal.
pub trait CommitSource {
    /// All complete groups after the exclusive timestamp, without omissions,
    /// in TigerBeetle order. Never advance past a partial linked group.
    fn read_after(&mut self, timestamp: u64, limit: usize) -> Result<Vec<Confirmed>>;
}

/// One instance per (destination, cluster, generation). `commit` MUST durably
/// and atomically bind full group identity/payload, apply the complete derived
/// bundle and advance its cursor from `expected` to last_timestamp. Exact
/// duplicate returns success; changed body or cursor race fails closed.
/// EventStore admission still verifies and signs its own business envelope.
pub trait DurableSink {
    fn watermark(&mut self, cluster: u128, generation: u64) -> Result<u64>;
    fn commit(&mut self, expected: u64, group: &Confirmed) -> Result<()>;
}

/// Recoverable persistence after financial commit. Each destination runs this
/// independently: a stopped destination never rolls back a TigerBeetle commit.
/// Lost sink replies are resolved by reading its durable cursor on the next run.
pub fn persist_once(
    source: &mut impl CommitSource,
    sink: &mut impl DurableSink,
    cluster: u128,
    generation: u64,
    limit: usize,
) -> Result<usize> {
    if generation == 0 || limit == 0 {
        return Err(Error::InvalidPage);
    }
    let mut cursor = sink.watermark(cluster, generation)?;
    let groups = source.read_after(cursor, limit)?;
    if groups.len() > limit {
        return Err(Error::InvalidPage);
    }
    // Validate the supplied page before changing this destination.
    let mut end = cursor;
    for group in &groups {
        if group.plan.cluster != cluster
            || group.plan.generation != generation
            || group.first_timestamp() <= end
        {
            return Err(Error::InvalidPage);
        }
        end = group.last_timestamp();
    }
    for group in &groups {
        sink.commit(cursor, group)?;
        cursor = group.last_timestamp();
    }
    Ok(groups.len())
}
