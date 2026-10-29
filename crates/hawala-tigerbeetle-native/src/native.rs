//! Official SDK, one unsplit linked request, then full original-ID readback.
//! Deliberately only loopback test clusters. TB has no record signatures; these
//! reads trust that local process/OS boundary. Production authenticated transport,
//! effective writer fencing and native policy guards are NOT implemented here.
use crate::durable::EvidenceStore;
use futures::executor::block_on;
use hawala_tigerbeetle_flow::{
    replay::{NativeHistory, NativeRecord},
    Confirmed, Error, FinancialPort, Plan, RecordedTransfer, Result, Submission, Transfer,
};
use std::{collections::BTreeMap, net::SocketAddr};
use tigerbeetle as tb;

pub struct NativeTestAdapter {
    client: tb::Client,
    evidence: EvidenceStore,
}
impl NativeTestAdapter {
    pub fn connect_loopback_test(address: SocketAddr, evidence: EvidenceStore) -> Result<Self> {
        if !address.ip().is_loopback() || address.port() == 0 {
            return Err(Error::Port);
        }
        let client =
            tb::Client::new(evidence.context().0, &address.to_string()).map_err(|_| Error::Port)?;
        Ok(Self { client, evidence })
    }
    pub fn evidence_mut(&mut self) -> &mut EvidenceStore {
        &mut self.evidence
    }
    fn authorized(&self, plan: &Plan) -> Result<()> {
        self.evidence.require(plan).map(|_| ())
    }
    fn lookup_checked(&self, plan: &Plan) -> Result<Vec<Option<RecordedTransfer>>> {
        self.authorized(plan)?;
        let ids: Vec<_> = plan.transfers().iter().map(|t| t.id).collect();
        let found = block_on(self.client.lookup_transfers(&ids)).map_err(|_| Error::Port)?;
        let mut by_id = BTreeMap::new();
        for row in found {
            if !ids.contains(&row.id) || by_id.insert(row.id, normalize(row)?).is_some() {
                return Err(Error::Conflict);
            }
        }
        let records: Vec<_> = ids
            .iter()
            .map(|id| by_id.remove(id).map(|n| n.record))
            .collect();
        for (r, t) in records.iter().zip(plan.transfers()) {
            if let Some(r) = r {
                if r.transfer != *t {
                    return Err(Error::Conflict);
                }
            }
        }
        if records.iter().any(Option::is_some) {
            if records.iter().any(Option::is_none) {
                return Err(Error::Conflict);
            }
            let present: Vec<_> = records.iter().flatten().collect();
            if present
                .windows(2)
                .any(|w| w[0].timestamp.checked_add(1) != Some(w[1].timestamp))
            {
                return Err(Error::Conflict);
            }
            Confirmed::from_records(plan.clone(), records.iter().flatten().cloned().collect())?;
        }
        Ok(records)
    }
}
impl FinancialPort for NativeTestAdapter {
    fn submit_linked(&mut self, plan: &Plan) -> Result<Submission> {
        self.authorized(plan)?;
        // Detect changed-body retries/partial foreign groups BEFORE any write.
        let old = self.lookup_checked(plan)?;
        if old.iter().all(Option::is_some) {
            return Ok(Submission::Accepted(old.into_iter().flatten().collect()));
        }
        let request: Vec<_> = plan
            .linked_transfers()
            .map(|(t, linked)| tb::Transfer {
                id: t.id,
                debit_account_id: t.debit_account_id,
                credit_account_id: t.credit_account_id,
                amount: t.amount,
                ledger: t.ledger,
                code: t.code,
                user_data_128: t.user_data_128,
                user_data_64: t.user_data_64,
                user_data_32: t.user_data_32,
                flags: if linked {
                    tb::TransferFlags::Linked
                } else {
                    tb::TransferFlags::empty()
                },
                ..Default::default()
            })
            .collect();
        // Any SDK error is uncertain. Never interpret absence as an abort, nor
        // return Rejected on a retry which may have an earlier in-flight commit.
        if block_on(self.client.create_transfers(&request)).is_err() {
            return Ok(Submission::Unknown);
        }
        match self.lookup_checked(plan) {
            Ok(rows) if rows.iter().all(Option::is_some) => {
                Ok(Submission::Accepted(rows.into_iter().flatten().collect()))
            }
            Ok(_) => Ok(Submission::Unknown),
            Err(Error::Port) => Ok(Submission::Unknown),
            Err(e) => Err(e),
        }
    }
    fn lookup(&mut self, plan: &Plan) -> Result<Vec<Option<RecordedTransfer>>> {
        self.lookup_checked(plan)
    }
}
impl NativeHistory for NativeTestAdapter {
    fn context(&self) -> (u128, u64) {
        self.evidence.context()
    }
    fn read_after(&mut self, timestamp: u64, limit: usize) -> Result<Vec<NativeRecord>> {
        if limit == 0 || limit > 4096 {
            return Err(Error::InvalidPage);
        }
        let Some(min) = timestamp.checked_add(1) else {
            return Ok(Vec::new());
        };
        // No ledger/code/user-data filters: unknown or unregistered history
        // fails closed rather than disappearing behind a filtered query.
        let rows = block_on(self.client.query_transfers(tb::QueryFilter {
            timestamp_min: min,
            limit: limit as u32,
            ..Default::default()
        }))
        .map_err(|_| Error::Port)?;
        let mut result = Vec::new();
        let mut previous = timestamp;
        for row in rows {
            let native = normalize(row)?;
            let t = &native.record.transfer;
            let (plan, _) = self.evidence.load(t.user_data_128)?;
            let index = (t.user_data_32 & 65535) as usize;
            if plan.transfers().get(index) != Some(t) || native.record.timestamp <= previous {
                return Err(Error::Conflict);
            }
            // Independent original-ID readback authenticates body against the
            // retained signed admission, including all other legs of this group.
            let found = self.lookup_checked(&plan)?;
            if found.get(index) != Some(&Some(native.record.clone())) {
                return Err(Error::Conflict);
            }
            previous = native.record.timestamp;
            result.push(native);
        }
        Ok(result)
    }
}
fn normalize(t: tb::Transfer) -> Result<NativeRecord> {
    let count = (t.user_data_32 >> 16) as usize;
    let index = (t.user_data_32 & 65535) as usize;
    let linked = if index + 1 < count {
        tb::TransferFlags::Linked.0
    } else {
        0
    };
    // Pinned 0.17.9 stores flags verbatim. Verify linked against the explicit
    // count/index BEFORE normalizing to the flow decoder's posted profile.
    if count == 0
        || count > 255
        || index >= count
        || t.flags.0 != linked
        || t.pending_id != 0
        || t.timeout != 0
        || t.timestamp == 0
    {
        return Err(Error::Conflict);
    }
    Ok(NativeRecord {
        record: RecordedTransfer {
            transfer: Transfer {
                id: t.id,
                debit_account_id: t.debit_account_id,
                credit_account_id: t.credit_account_id,
                amount: t.amount,
                ledger: t.ledger,
                code: t.code,
                user_data_128: t.user_data_128,
                user_data_64: t.user_data_64,
                user_data_32: t.user_data_32,
            },
            timestamp: t.timestamp,
        },
        flags: 0,
        pending_id: t.pending_id,
        timeout: t.timeout,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_native_link_flags_are_checked_before_normalization() {
        let t = tb::Transfer {
            id: 100,
            debit_account_id: 1,
            credit_account_id: 2,
            amount: 1,
            ledger: 1,
            code: 1,
            user_data_128: 100,
            user_data_64: 1,
            user_data_32: hawala_tigerbeetle_flow::leg_tag(2, 0),
            flags: tb::TransferFlags::Linked,
            timestamp: 10,
            ..Default::default()
        };
        assert_eq!(normalize(t).unwrap().flags, 0);
        for flags in [0, 2, 3, 4, 8, 16, 32, 64, 128, 256, 512] {
            assert!(normalize(tb::Transfer {
                flags: tb::TransferFlags(flags),
                ..t
            })
            .is_err());
        }
        assert!(normalize(tb::Transfer {
            user_data_32: hawala_tigerbeetle_flow::leg_tag(2, 1),
            ..t
        })
        .is_err());
        assert!(normalize(tb::Transfer { pending_id: 1, ..t }).is_err());
        assert!(normalize(tb::Transfer { timeout: 1, ..t }).is_err());
    }
}
