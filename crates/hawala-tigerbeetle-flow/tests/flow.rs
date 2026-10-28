use hawala_tigerbeetle_flow::*;
use std::collections::BTreeMap;

fn plan(operation: u128) -> Plan {
    let transfers = (0..2)
        .map(|i| Transfer {
            id: operation + i as u128,
            debit_account_id: 10,
            credit_account_id: 20 + i as u128,
            amount: 5,
            ledger: 1,
            code: 1,
            user_data_128: operation,
            user_data_64: 3,
            user_data_32: leg_tag(2, i),
        })
        .collect();
    Plan::new(0, 1, operation, 3, transfers).unwrap()
}

/// Synthetic native boundary only. Shared maps stand for surviving port state;
/// these tests do not establish a real database's durability or authentication.
#[derive(Default)]
struct Native {
    records: BTreeMap<u128, RecordedTransfer>,
    clock: u64,
    calls: usize,
    lose_reply: bool,
    reject: bool,
    hide_lookup: bool,
}
impl FinancialPort for Native {
    fn submit_linked(&mut self, plan: &Plan) -> Result<Submission> {
        self.calls += 1;
        if self.reject {
            return Ok(Submission::Rejected);
        }
        let mut records = Vec::new();
        for t in plan.transfers() {
            if let Some(old) = self.records.get(&t.id) {
                if old.transfer != *t {
                    return Err(Error::Conflict);
                }
                records.push(old.clone());
            } else {
                self.clock += 1;
                records.push(RecordedTransfer {
                    transfer: t.clone(),
                    timestamp: self.clock,
                });
            }
        }
        for r in &records {
            self.records.insert(r.transfer.id, r.clone());
        }
        if self.lose_reply {
            Ok(Submission::Unknown)
        } else {
            Ok(Submission::Accepted(records))
        }
    }
    fn lookup(&mut self, plan: &Plan) -> Result<Vec<Option<RecordedTransfer>>> {
        Ok(plan
            .transfers()
            .iter()
            .map(|t| {
                if self.hide_lookup {
                    None
                } else {
                    self.records.get(&t.id).cloned()
                }
            })
            .collect())
    }
}

fn committed(native: &mut Native, plan: &Plan) -> Confirmed {
    match record(native, plan).unwrap() {
        Decision::Committed(g) => g,
        other => panic!("expected commit, got {other:?}"),
    }
}

struct Source {
    groups: Vec<Confirmed>,
    malformed: bool,
}
impl CommitSource for Source {
    fn read_after(&mut self, timestamp: u64, limit: usize) -> Result<Vec<Confirmed>> {
        Ok(if self.malformed {
            self.groups.clone()
        } else {
            self.groups
                .iter()
                .filter(|g| g.last_timestamp() > timestamp)
                .take(limit)
                .cloned()
                .collect()
        })
    }
}

#[derive(Default)]
struct Sink {
    cursor: u64,
    groups: BTreeMap<u128, Confirmed>,
    fail_before: bool,
    fail_after: bool,
}
impl DurableSink for Sink {
    fn watermark(&mut self, _: u128, _: u64) -> Result<u64> {
        Ok(self.cursor)
    }
    fn commit(&mut self, expected: u64, group: &Confirmed) -> Result<()> {
        if self.fail_before {
            return Err(Error::Port);
        }
        if let Some(old) = self.groups.get(&group.plan().operation()) {
            return if old == group {
                Ok(())
            } else {
                Err(Error::Conflict)
            };
        }
        if self.cursor != expected {
            return Err(Error::Conflict);
        }
        self.groups.insert(group.plan().operation(), group.clone());
        self.cursor = group.last_timestamp();
        if self.fail_after {
            Err(Error::Port)
        } else {
            Ok(())
        }
    }
}

#[test]
fn financial_commit_has_no_downstream_dependency() {
    let mut native = Native::default();
    let group = committed(&mut native, &plan(100));
    assert_eq!(native.calls, 1);
    assert_eq!(group.last_timestamp(), 2);
    // No sink exists until AFTER the confirmed financial decision.
    let mut sink = Sink {
        fail_before: true,
        ..Sink::default()
    };
    let mut source = Source {
        groups: vec![group],
        malformed: false,
    };
    assert_eq!(
        persist_once(&mut source, &mut sink, 0, 1, 10),
        Err(Error::Port)
    );
    assert_eq!(native.records.len(), 2);
    assert_eq!(sink.cursor, 0);
}

#[test]
fn lost_native_reply_recovers_original_complete_group_without_submission() {
    let p = plan(100);
    let mut native = Native {
        lose_reply: true,
        ..Native::default()
    };
    assert_eq!(record(&mut native, &p), Ok(Decision::Held));
    native.lose_reply = false;
    assert!(matches!(
        recover(&mut native, &p),
        Ok(Decision::Committed(_))
    ));
    assert_eq!(native.calls, 1);
    assert_eq!(native.records.len(), 2);
}

#[test]
fn absent_lookup_retries_same_ids_instead_of_aborting_or_replacing() {
    let p = plan(100);
    let mut native = Native {
        lose_reply: true,
        ..Native::default()
    };
    assert_eq!(record(&mut native, &p), Ok(Decision::Held));
    native.hide_lookup = true;
    native.lose_reply = false;
    assert!(matches!(
        recover(&mut native, &p),
        Ok(Decision::Committed(_))
    ));
    assert_eq!(native.calls, 2);
    assert_eq!(native.records.len(), 2);
    assert_eq!(native.clock, 2);
}

#[test]
fn partial_group_is_conflict_and_never_retried() {
    let p = plan(100);
    let mut native = Native::default();
    committed(&mut native, &p);
    native.records.remove(&101);
    assert_eq!(recover(&mut native, &p), Err(Error::Conflict));
    assert_eq!(native.calls, 1);
}

#[test]
fn changed_body_on_existing_id_fails_closed() {
    let p = plan(100);
    let mut native = Native::default();
    committed(&mut native, &p);
    let mut altered = p.transfers().to_vec();
    altered[0].amount = 6;
    let altered = Plan::new(0, 1, 100, 3, altered).unwrap();
    assert_eq!(recover(&mut native, &altered), Err(Error::Conflict));
    assert_eq!(native.calls, 1);
}

#[test]
fn rejection_creates_no_success_or_downstream_work() {
    let mut native = Native {
        reject: true,
        ..Native::default()
    };
    assert_eq!(record(&mut native, &plan(100)), Ok(Decision::Rejected));
    assert!(native.records.is_empty());
}

#[test]
fn lost_sink_reply_resumes_from_durable_destination_cursor() {
    let mut native = Native::default();
    let g = committed(&mut native, &plan(100));
    let mut source = Source {
        groups: vec![g],
        malformed: false,
    };
    let mut sink = Sink {
        fail_after: true,
        ..Sink::default()
    };
    assert_eq!(
        persist_once(&mut source, &mut sink, 0, 1, 10),
        Err(Error::Port)
    );
    sink.fail_after = false;
    assert_eq!(persist_once(&mut source, &mut sink, 0, 1, 10), Ok(0));
    assert_eq!(sink.groups.len(), 1);
    assert_eq!(sink.cursor, 2);
}

#[test]
fn destinations_catch_up_independently_and_rebuild_from_native_source() {
    let mut native = Native::default();
    let groups = vec![
        committed(&mut native, &plan(100)),
        committed(&mut native, &plan(200)),
    ];
    let mut source = Source {
        groups,
        malformed: false,
    };
    let mut slow = Sink {
        fail_before: true,
        ..Sink::default()
    };
    let mut fast = Sink::default();
    assert_eq!(
        persist_once(&mut source, &mut slow, 0, 1, 10),
        Err(Error::Port)
    );
    assert_eq!(persist_once(&mut source, &mut fast, 0, 1, 10), Ok(2));
    slow.fail_before = false;
    assert_eq!(persist_once(&mut source, &mut slow, 0, 1, 1), Ok(1));
    assert_eq!(persist_once(&mut source, &mut slow, 0, 1, 10), Ok(1));
    assert_eq!(fast.groups, slow.groups);
    let mut rebuilt = Sink::default();
    assert_eq!(persist_once(&mut source, &mut rebuilt, 0, 1, 10), Ok(2));
    assert_eq!(fast.groups, rebuilt.groups);
}

#[test]
fn reordered_or_wrong_generation_page_is_rejected_before_any_write() {
    let mut native = Native::default();
    let a = committed(&mut native, &plan(100));
    let b = committed(&mut native, &plan(200));
    let mut source = Source {
        groups: vec![b, a],
        malformed: true,
    };
    let mut sink = Sink::default();
    assert_eq!(
        persist_once(&mut source, &mut sink, 0, 1, 10),
        Err(Error::InvalidPage)
    );
    source.groups.reverse();
    assert_eq!(
        persist_once(&mut source, &mut sink, 0, 2, 10),
        Err(Error::InvalidPage)
    );
    assert!(sink.groups.is_empty());
}

#[test]
fn native_records_must_match_complete_ordered_plan() {
    let mut native = Native::default();
    let p = plan(100);
    let g = committed(&mut native, &p);
    let mut records = g.records().to_vec();
    records.swap(0, 1);
    assert_eq!(
        Confirmed::from_records(p.clone(), records),
        Err(Error::Conflict)
    );
    let mut records = g.records().to_vec();
    records[1].timestamp = records[0].timestamp;
    assert_eq!(Confirmed::from_records(p, records), Err(Error::Conflict));
}

#[test]
fn plan_retains_group_membership_and_emits_one_closed_linked_chain() {
    let p = plan(100);
    assert_eq!(
        p.linked_transfers()
            .map(|(_, linked)| linked)
            .collect::<Vec<_>>(),
        vec![true, false]
    );
    for (i, t) in p.transfers().iter().enumerate() {
        assert_eq!(t.user_data_128, p.operation());
        assert_eq!(t.user_data_64, p.epoch());
        assert_eq!(t.user_data_32, leg_tag(2, i));
    }
}

#[test]
fn malformed_ids_amounts_and_group_tags_are_refused_before_submission() {
    let p = plan(100);
    for mutate in 0..7 {
        let mut t = p.transfers().to_vec();
        match mutate {
            0 => t[0].id = 0,
            1 => t[1].id = t[0].id,
            2 => t[0].amount = 0,
            3 => t[0].amount = MAX_AMOUNT + 1,
            4 => t[0].user_data_128 = 99,
            5 => t[0].user_data_32 = leg_tag(3, 0),
            _ => t[0].debit_account_id = t[0].credit_account_id,
        }
        assert_eq!(Plan::new(0, 1, 100, 3, t), Err(Error::InvalidPlan));
    }
}

#[test]
fn metadata_cannot_replace_the_unique_first_transfer_anchor() {
    let p = plan(100);
    let mut transfers = p.transfers().to_vec();
    transfers[0].id = 900;
    assert_eq!(Plan::new(0, 1, 100, 3, transfers), Err(Error::InvalidPlan));
}

#[test]
fn destination_cursor_does_not_advance_over_a_replayed_page() {
    let mut native = Native::default();
    let group = committed(&mut native, &plan(100));
    let mut source = Source {
        groups: vec![group],
        malformed: false,
    };
    let mut sink = Sink::default();
    assert_eq!(persist_once(&mut source, &mut sink, 0, 1, 10), Ok(1));
    source.malformed = true; // Simulate an upstream redelivery before this cursor.
    assert_eq!(
        persist_once(&mut source, &mut sink, 0, 1, 10),
        Err(Error::InvalidPage)
    );
    assert_eq!(sink.cursor, 2);
    assert_eq!(sink.groups.len(), 1);
}
