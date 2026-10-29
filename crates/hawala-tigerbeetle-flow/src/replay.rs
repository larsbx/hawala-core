//! Bounded reassembly of ordinary posted-transfer history into complete groups.
//! Input must come from an authenticated, complete native-history reader under
//! a pinned registry. This decoder verifies structure, not source authenticity
//! or the absence of records omitted by the reader. It never submits transfers.
use crate::{CommitSource, Confirmed, Error, Plan, RecordedTransfer, Result, MAX_LEGS};

/// Application-side memory bound, independent of any particular native limit.
pub const MAX_PAGE_ROWS: usize = 4096;
pub const MAX_GROUPS_PER_READ: usize = 64;

/// Preserve raw mode fields until validation; do not silently normalize pending,
/// balancing, closing or imported transfers into ordinary posted transfers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeRecord {
    pub record: RecordedTransfer,
    pub flags: u16,
    pub pending_id: u128,
    pub timeout: u32,
}

/// Trusted SDK/transport boundary. Return complete unfiltered coverage of the
/// configured profile's native history after the exclusive timestamp, ordered
/// by timestamp, with at most `limit` rows. Registry/scope and native provenance
/// must be checked here; a notification stream with undetected gaps is invalid.
pub trait NativeHistory {
    /// Pinned native cluster and externally governed recovery generation.
    fn context(&self) -> (u128, u64);
    fn read_after(&mut self, timestamp: u64, limit: usize) -> Result<Vec<NativeRecord>>;
}

/// Concrete CommitSource over paged native-history reads. Every call starts at
/// the destination's durable cursor, so lost replies or abandoned in-memory
/// suffixes cannot skip any group. No financial submission or SQL prewrite.
pub struct HistorySource<R> {
    reader: R,
    cluster: u128,
    generation: u64,
    page_rows: usize,
}

impl<R: NativeHistory> HistorySource<R> {
    pub fn new(reader: R, cluster: u128, generation: u64, page_rows: usize) -> Result<Self> {
        if generation == 0 || page_rows == 0 || page_rows > MAX_PAGE_ROWS {
            return Err(Error::InvalidPage);
        }
        if reader.context() != (cluster, generation) {
            return Err(Error::Conflict);
        }
        Ok(Self {
            reader,
            cluster,
            generation,
            page_rows,
        })
    }

    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R: NativeHistory> CommitSource for HistorySource<R> {
    fn read_after(&mut self, timestamp: u64, limit: usize) -> Result<Vec<Confirmed>> {
        if self.reader.context() != (self.cluster, self.generation) {
            return Err(Error::Conflict);
        }
        if limit == 0 || limit > MAX_GROUPS_PER_READ {
            return Err(Error::InvalidPage);
        }
        let mut decoder = Decoder::new(self.cluster, self.generation, timestamp)?;
        let mut groups = Vec::new();
        loop {
            let before = decoder.read_through();
            let rows = self.reader.read_after(before, self.page_rows)?;
            if rows.len() > self.page_rows {
                return Err(Error::InvalidPage);
            }
            if rows.is_empty() {
                decoder.finish()?;
                return Ok(groups);
            }
            groups.extend(decoder.push_page(&rows)?);
            if decoder.read_through() <= before {
                return Err(Error::InvalidPage);
            }
            if groups.len() >= limit {
                groups.truncate(limit);
                return Ok(groups);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoder {
    cluster: u128,
    generation: u64,
    complete: u64,
    read: u64,
    pending: Vec<RecordedTransfer>,
}

impl Decoder {
    /// `after` must be a verified complete-group boundary from the destination's
    /// durable cursor. A partial in-memory read watermark is not such a cursor.
    pub fn new(cluster: u128, generation: u64, after: u64) -> Result<Self> {
        if generation == 0 {
            return Err(Error::InvalidPage);
        }
        Ok(Self {
            cluster,
            generation,
            complete: after,
            read: after,
            pending: Vec::new(),
        })
    }

    /// Exclusive native read position. Only `completed_through` can identify a
    /// complete group; neither value advances a destination's durable cursor.
    pub fn read_through(&self) -> u64 {
        self.read
    }
    pub fn completed_through(&self) -> u64 {
        self.complete
    }
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// A page may end inside a group: retain its bounded suffix without emitting
    /// that group. Exact duplicates of currently buffered rows are harmless.
    /// Old completed rows require replay from the destination cursor instead.
    /// Any malformed row rolls back this entire call, including partial buffers.
    pub fn push_page(&mut self, rows: &[NativeRecord]) -> Result<Vec<Confirmed>> {
        if rows.len() > MAX_PAGE_ROWS {
            return Err(Error::InvalidPage);
        }
        let mut next = self.clone();
        let mut groups = Vec::new();
        for row in rows {
            if row.flags != 0 || row.pending_id != 0 || row.timeout != 0 {
                return Err(Error::InvalidPage);
            }
            if let Some(group) = next.push(&row.record)? {
                groups.push(group);
            }
        }
        *self = next;
        Ok(groups)
    }

    /// End-of-source/checkpoint may not certify a cursor inside a partial group.
    pub fn finish(&self) -> Result<u64> {
        if self.has_pending() {
            Err(Error::InvalidPage)
        } else {
            Ok(self.complete)
        }
    }

    fn push(&mut self, row: &RecordedTransfer) -> Result<Option<Confirmed>> {
        if row.timestamp <= self.read {
            return if self.pending.iter().any(|old| old == row) {
                Ok(None)
            } else {
                Err(Error::Conflict)
            };
        }
        let transfer = &row.transfer;
        let count = (transfer.user_data_32 >> 16) as usize;
        let index = (transfer.user_data_32 & 0xffff) as usize;
        if count == 0 || count > MAX_LEGS || index >= count || index != self.pending.len() {
            return Err(Error::InvalidPage);
        }
        if let Some(first) = self.pending.first() {
            if transfer.user_data_128 != first.transfer.user_data_128
                || transfer.user_data_64 != first.transfer.user_data_64
                || count != (first.transfer.user_data_32 >> 16) as usize
            {
                return Err(Error::Conflict);
            }
        }
        self.pending.push(row.clone());
        self.read = row.timestamp;
        if self.pending.len() != count {
            return Ok(None);
        }
        let first = &self.pending[0].transfer;
        let plan = Plan::new(
            self.cluster,
            self.generation,
            first.user_data_128,
            first.user_data_64,
            self.pending.iter().map(|r| r.transfer.clone()).collect(),
        )?;
        let group = Confirmed::from_records(plan, self.pending.clone())?;
        self.complete = group.last_timestamp();
        self.pending.clear();
        Ok(Some(group))
    }
}
