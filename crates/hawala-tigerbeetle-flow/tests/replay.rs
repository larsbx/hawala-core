use hawala_tigerbeetle_flow::{replay::*, *};

fn rows(operation: u128, timestamp: u64, count: usize) -> Vec<NativeRecord> {
    (0..count)
        .map(|i| NativeRecord {
            record: RecordedTransfer {
                transfer: Transfer {
                    id: operation + i as u128,
                    debit_account_id: 10,
                    credit_account_id: 20,
                    amount: 5,
                    ledger: 1,
                    code: 1,
                    user_data_128: operation,
                    user_data_64: 3,
                    user_data_32: leg_tag(count, i),
                },
                timestamp: timestamp + i as u64,
            },
            flags: 0,
            pending_id: 0,
            timeout: 0,
        })
        .collect()
}

#[test]
fn page_splits_never_emit_or_checkpoint_a_partial_group() {
    let raw = rows(100, 10, 3);
    for split in 1..3 {
        let mut decoder = Decoder::new(0, 1, 0).unwrap();
        assert!(decoder.push_page(&raw[..split]).unwrap().is_empty());
        assert_eq!(decoder.completed_through(), 0);
        assert_eq!(decoder.read_through(), raw[split - 1].record.timestamp);
        assert_eq!(decoder.finish(), Err(Error::InvalidPage));
        let groups = decoder.push_page(&raw[split..]).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].records().len(), 3);
        assert_eq!(decoder.finish(), Ok(12));
    }
}

#[test]
fn currently_buffered_redelivery_is_exactly_idempotent_but_changed_body_conflicts() {
    let raw = rows(100, 10, 2);
    let mut decoder = Decoder::new(0, 1, 0).unwrap();
    decoder.push_page(&raw[..1]).unwrap();
    assert!(decoder.push_page(&raw[..1]).unwrap().is_empty());
    let before = decoder.clone();
    let mut changed = raw[0].clone();
    changed.record.transfer.amount += 1;
    assert_eq!(decoder.push_page(&[changed]), Err(Error::Conflict));
    assert_eq!(decoder, before);
    assert_eq!(decoder.push_page(&raw).unwrap().len(), 1);
}

#[test]
fn invalid_later_row_rolls_back_whole_page_including_earlier_complete_group() {
    let mut raw = rows(100, 10, 2);
    let mut next = rows(200, 20, 2);
    next[1].record.transfer.user_data_128 = 999;
    raw.extend(next);
    let mut decoder = Decoder::new(0, 1, 0).unwrap();
    let before = decoder.clone();
    assert_eq!(decoder.push_page(&raw), Err(Error::Conflict));
    assert_eq!(decoder, before);
}

#[test]
fn restart_from_destination_cursor_reconstructs_same_unpersisted_groups() {
    let mut raw = rows(100, 10, 2);
    raw.extend(rows(200, 20, 2));
    let mut decoder = Decoder::new(0, 1, 0).unwrap();
    let groups = decoder.push_page(&raw).unwrap();
    // Destination persisted only the first group before the source process died.
    let mut restarted = Decoder::new(0, 1, groups[0].last_timestamp()).unwrap();
    assert_eq!(restarted.push_page(&raw[2..]).unwrap(), groups[1..]);
}

#[test]
fn missing_reordered_interleaved_or_wrong_count_legs_are_refused() {
    let raw = rows(100, 10, 3);
    for mutation in 0..5 {
        let mut changed = raw.clone();
        match mutation {
            0 => {
                changed.remove(0);
            }
            1 => {
                changed.remove(1);
            }
            2 => changed.swap(0, 1),
            3 => changed[1].record.transfer.user_data_64 += 1,
            _ => changed[1].record.transfer.user_data_32 = leg_tag(4, 1),
        }
        let mut decoder = Decoder::new(0, 1, 0).unwrap();
        assert!(decoder.push_page(&changed).is_err());
        assert_eq!(decoder.completed_through(), 0);
        assert!(!decoder.has_pending());
    }
}

#[test]
fn unsupported_native_modes_must_not_be_normalized_into_posted_history() {
    for mutation in 0..3 {
        let mut raw = rows(100, 10, 2);
        match mutation {
            0 => raw[0].flags = 2,
            1 => raw[0].pending_id = 99,
            _ => raw[0].timeout = 60,
        }
        let mut decoder = Decoder::new(0, 1, 0).unwrap();
        assert_eq!(decoder.push_page(&raw), Err(Error::InvalidPage));
        assert_eq!(decoder.completed_through(), 0);
    }
}

#[test]
fn maximum_group_survives_one_row_pages_without_unbounded_buffering() {
    let raw = rows(1000, 10, MAX_LEGS);
    let mut decoder = Decoder::new(0, 1, 0).unwrap();
    let mut groups = Vec::new();
    for row in &raw {
        groups.extend(decoder.push_page(std::slice::from_ref(row)).unwrap());
    }
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].records().len(), MAX_LEGS);
    assert_eq!(decoder.finish(), Ok(10 + MAX_LEGS as u64 - 1));
    let mut invalid = raw[0].clone();
    invalid.record.transfer.user_data_32 = leg_tag(MAX_LEGS + 1, 0);
    assert_eq!(
        Decoder::new(0, 1, 0).unwrap().push_page(&[invalid]),
        Err(Error::InvalidPage)
    );
    assert!(Decoder::new(0, 0, 0).is_err());
}

#[test]
fn invalid_complete_body_and_nonforward_timestamps_do_not_escape() {
    for mutation in 0..4 {
        let mut raw = rows(100, 10, 2);
        match mutation {
            0 => raw[0].record.transfer.id = 999,
            1 => raw[1].record.transfer.amount = 0,
            2 => raw[1].record.timestamp = raw[0].record.timestamp,
            _ => raw[1].record.transfer.id = raw[0].record.transfer.id,
        }
        let mut decoder = Decoder::new(0, 1, 0).unwrap();
        assert!(decoder.push_page(&raw).is_err());
        assert_eq!(decoder.completed_through(), 0);
    }
}

struct Reader {
    rows: Vec<NativeRecord>,
    oversized: bool,
}
impl NativeHistory for Reader {
    fn context(&self) -> (u128, u64) {
        (0, 1)
    }
    fn read_after(&mut self, timestamp: u64, limit: usize) -> Result<Vec<NativeRecord>> {
        Ok(self
            .rows
            .iter()
            .filter(|r| r.record.timestamp > timestamp)
            .take(if self.oversized { limit + 1 } else { limit })
            .cloned()
            .collect())
    }
}

#[test]
fn actual_commit_source_collects_split_pages_and_resumes_from_destination_cursor() {
    let mut raw = rows(100, 10, 3);
    raw.extend(rows(200, 20, 2));
    let reader = Reader {
        rows: raw,
        oversized: false,
    };
    let mut source = HistorySource::new(reader, 0, 1, 1).unwrap();
    let first = source.read_after(0, 1).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].last_timestamp(), 12);
    // The destination never acknowledged the first result: safely read it again.
    assert_eq!(source.read_after(0, 1).unwrap(), first);
    let second = source.read_after(12, 1).unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].plan().operation(), 200);
    assert!(source.read_after(21, 1).unwrap().is_empty());
}

#[test]
fn source_read_ahead_never_skips_groups_on_next_destination_call() {
    let mut raw = rows(100, 10, 2);
    raw.extend(rows(200, 20, 2));
    raw.extend(rows(300, 30, 2));
    let reader = Reader {
        rows: raw,
        oversized: false,
    };
    let mut source = HistorySource::new(reader, 0, 1, MAX_PAGE_ROWS).unwrap();
    let first = source.read_after(0, 1).unwrap();
    assert_eq!(first.len(), 1);
    let rest = source.read_after(first[0].last_timestamp(), 2).unwrap();
    assert_eq!(
        rest.iter()
            .map(|g| g.plan().operation())
            .collect::<Vec<_>>(),
        vec![200, 300]
    );
}

#[test]
fn end_of_native_history_inside_group_cannot_certify_any_prefix() {
    let mut raw = rows(100, 10, 2);
    raw.extend(rows(200, 20, 3)[..2].iter().cloned());
    let reader = Reader {
        rows: raw,
        oversized: false,
    };
    let mut source = HistorySource::new(reader, 0, 1, 2).unwrap();
    assert_eq!(source.read_after(0, 2), Err(Error::InvalidPage));
}

#[test]
fn native_reader_cannot_ignore_configured_memory_bounds() {
    let reader = Reader {
        rows: rows(100, 10, 3),
        oversized: true,
    };
    let mut source = HistorySource::new(reader, 0, 1, 1).unwrap();
    assert_eq!(source.read_after(0, 1), Err(Error::InvalidPage));
    assert_eq!(
        source.read_after(0, MAX_GROUPS_PER_READ + 1),
        Err(Error::InvalidPage)
    );
}

#[test]
fn native_source_must_match_the_destination_cluster_and_generation() {
    for (cluster, generation) in [(99, 1), (0, 2)] {
        let reader = Reader {
            rows: rows(100, 10, 2),
            oversized: false,
        };
        assert!(matches!(
            HistorySource::new(reader, cluster, generation, 1),
            Err(Error::Conflict)
        ));
    }
}
