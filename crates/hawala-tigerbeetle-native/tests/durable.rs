mod support;
use hawala_tigerbeetle_flow::{persist_once, CommitSource, DurableSink, Error, Plan};
use hawala_tigerbeetle_native::{
    accounting::{compile, Posting},
    durable::{EvidenceStore, SignedSink},
    evidence::{parse_plan, plan_bytes, Admission},
};
use rusqlite::Connection;
use support::*;
use tempfile::tempdir;

#[test]
fn compiler_exact_pairings_registry_and_bounds() {
    let p = plan(100);
    let pairs: Vec<_> = p
        .transfers()
        .iter()
        .map(|t| (t.id, t.debit_account_id, t.credit_account_id, t.amount))
        .collect();
    assert_eq!(pairs, vec![(100, 1, 3, 4), (101, 1, 4, 3), (102, 2, 4, 3)]);
    assert_eq!(parse_plan(&plan_bytes(&p)).unwrap(), p);
    for end in 0..plan_bytes(&p).len() {
        assert!(parse_plan(&plan_bytes(&p)[..end]).is_err());
    }
    let mut extra = plan_bytes(&p);
    extra.push(0);
    assert!(parse_plan(&extra).is_err());
    let mut r = registry();
    r.accounts[1].native_id = 1;
    assert!(r.bytes().is_err());
    let mut r = registry();
    r.revision = 2;
    r.previous = registry().digest().unwrap();
    assert!(registry().successor(&r).is_ok());
    r.accounts[0].native_id = 9;
    assert!(registry().successor(&r).is_err());
    assert!(compile(
        &registry(),
        CLUSTER,
        1,
        100,
        &[
            Posting {
                account: [1; 32],
                credit: false,
                amount: 1
            },
            Posting {
                account: [3; 32],
                credit: true,
                amount: 2
            }
        ]
    )
    .is_err());
    assert!(compile(
        &registry(),
        CLUSTER,
        1,
        u128::MAX - 1,
        &[
            Posting {
                account: [1; 32],
                credit: false,
                amount: 7
            },
            Posting {
                account: [3; 32],
                credit: true,
                amount: 4
            },
            Posting {
                account: [4; 32],
                credit: true,
                amount: 3
            }
        ]
    )
    .is_err());
}
#[test]
fn evidence_survives_reopen_rejects_changed_retry_and_reserved_range_overlap() {
    let d = tempdir().unwrap();
    let path = d.path().join("evidence.sqlite");
    let p = plan(100);
    let mut e = evidence(&path);
    retain(&mut e, &p);
    retain(&mut e, &p);
    drop(e);
    let mut e = evidence(&path);
    assert_eq!(e.load(100).unwrap().0, p);
    let mut ts = p.transfers().to_vec();
    ts[0].amount += 1;
    let changed = Plan::new(CLUSTER, 1, 100, 9, ts).unwrap();
    let a = Admission::sign(&registry(), &changed, b"different body", &key()).unwrap();
    assert_eq!(e.retain(&a), Err(Error::Conflict));
    let overlapping = plan(102);
    let a = Admission::sign(&registry(), &overlapping, b"overlap", &key()).unwrap();
    assert_eq!(e.retain(&a), Err(Error::Conflict));
    assert!(e.load(102).is_err());
    let mut a = Admission::sign(&registry(), &plan(200), b"original", &key()).unwrap();
    a.bytes[10] ^= 1;
    assert_eq!(e.retain(&a), Err(Error::Conflict));
    assert!(EvidenceStore::open(&path, registry(), CLUSTER, 2, key().verifying_key()).is_err());
}
#[test]
fn signed_sink_reopens_with_exact_bytes_and_rejects_changed_body_cursor_and_signature() {
    let d = tempdir().unwrap();
    let ev = d.path().join("evidence.sqlite");
    let db = d.path().join("sink.sqlite");
    let p = plan(100);
    retain(&mut evidence(&ev), &p);
    let g = group(&p, 1000);
    let mut s = sink(&db, &ev, 10);
    assert_eq!(s.watermark(CLUSTER, 1).unwrap(), 0);
    s.commit(0, &g).unwrap();
    s.commit(0, &g).unwrap();
    assert_eq!(s.audit().unwrap(), 1);
    assert_eq!(s.commit(1, &g), Err(Error::Conflict));
    drop(s);
    let mut s = sink(&db, &ev, 10);
    assert_eq!(s.watermark(CLUSTER, 1).unwrap(), 1002);
    assert!(s.watermark(CLUSTER, 2).is_err());
    assert!(s.commit(0, &group(&p, 2000)).is_err());
    drop(s);
    let db = Connection::open(&db).unwrap();
    db.execute("UPDATE outcomes SET signature=zeroblob(64)", [])
        .unwrap();
    assert!(SignedSink::open(
        d.path().join("sink.sqlite").as_path(),
        evidence(&ev),
        [10; 32],
        ed25519_dalek::SigningKey::from_bytes(&[10; 32])
    )
    .is_err());
}
#[test]
fn sql_failure_and_lost_reply_preserve_independent_atomic_cursors() {
    struct Source(Vec<hawala_tigerbeetle_flow::Confirmed>);
    impl CommitSource for Source {
        fn read_after(
            &mut self,
            t: u64,
            _: usize,
        ) -> hawala_tigerbeetle_flow::Result<Vec<hawala_tigerbeetle_flow::Confirmed>> {
            Ok(self
                .0
                .iter()
                .filter(|g| g.first_timestamp() > t)
                .cloned()
                .collect())
        }
    }
    struct Lost(SignedSink);
    impl DurableSink for Lost {
        fn watermark(&mut self, c: u128, g: u64) -> hawala_tigerbeetle_flow::Result<u64> {
            self.0.watermark(c, g)
        }
        fn commit(
            &mut self,
            t: u64,
            g: &hawala_tigerbeetle_flow::Confirmed,
        ) -> hawala_tigerbeetle_flow::Result<()> {
            self.0.commit(t, g)?;
            Err(Error::Port)
        }
    }
    let d = tempdir().unwrap();
    let ev = d.path().join("evidence.sqlite");
    let a = d.path().join("a.sqlite");
    let b = d.path().join("b.sqlite");
    let p = plan(100);
    let p2 = plan(200);
    let mut e = evidence(&ev);
    retain(&mut e, &p);
    retain(&mut e, &p2);
    let mut source = Source(vec![group(&p, 1000), group(&p2, 2000)]);
    let mut sa = Lost(sink(&a, &ev, 10));
    let mut sb = sink(&b, &ev, 11);
    assert!(persist_once(&mut source, &mut sa, CLUSTER, 1, 2).is_err());
    drop(sa);
    let mut sa = sink(&a, &ev, 10);
    assert_eq!(sa.watermark(CLUSTER, 1).unwrap(), 1002);
    assert_eq!(sb.watermark(CLUSTER, 1).unwrap(), 0);
    let lock = Connection::open(&b).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(persist_once(&mut source, &mut sb, CLUSTER, 1, 2).is_err());
    assert_eq!(sb.watermark(CLUSTER, 1).unwrap(), 0);
    lock.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        persist_once(&mut source, &mut sa, CLUSTER, 1, 2).unwrap(),
        1
    );
    assert_eq!(
        persist_once(&mut source, &mut sb, CLUSTER, 1, 2).unwrap(),
        2
    );
    assert_eq!(sa.audit().unwrap(), 2);
    assert_eq!(sb.audit().unwrap(), 2);
}
#[test]
fn restart_rolls_back_uncommitted_bundle_and_cursor_and_detects_missing_rows() {
    let d = tempdir().unwrap();
    let ev = d.path().join("e.sqlite");
    let db = d.path().join("s.sqlite");
    let p = plan(100);
    retain(&mut evidence(&ev), &p);
    drop(sink(&db, &ev, 10));
    let c = Connection::open(&db).unwrap();
    c.execute_batch("BEGIN IMMEDIATE; UPDATE cursor SET last=x'0000000000000001';")
        .unwrap();
    drop(c);
    let mut s = sink(&db, &ev, 10);
    assert_eq!(s.watermark(CLUSTER, 1).unwrap(), 0);
    s.commit(0, &group(&p, 1000)).unwrap();
    drop(s);
    let c = Connection::open(&db).unwrap();
    c.execute("DELETE FROM outcomes", []).unwrap();
    assert!(SignedSink::open(
        &db,
        evidence(&ev),
        [10; 32],
        ed25519_dalek::SigningKey::from_bytes(&[10; 32])
    )
    .is_err());
}

#[test]
fn registry_bytes_and_compiler_pairings_conform_to_independent_pr50_fixture() {
    use hawala_tigerbeetle_native::accounting::{Account, Ledger, Registry};
    let bytes = include_bytes!("fixtures/node-books-50-registry.bin");
    let start = b"hawala-node-books-registry/v1\0".len();
    let mut context = [[0u8; 32]; 5];
    for (i, c) in context.iter_mut().enumerate() {
        c.copy_from_slice(&bytes[start + 32 * i..start + 32 * (i + 1)]);
    }
    let r = Registry {
        context,
        revision: 1,
        previous: [0; 32],
        epoch: 1,
        ledgers: vec![
            Ledger {
                id: 3,
                currency: *b"EUR",
                scale: 2,
            },
            Ledger {
                id: 9,
                currency: *b"USD",
                scale: 2,
            },
        ],
        accounts: (1u8..=8)
            .map(|i| {
                let mut id = [0u8; 32];
                id[31] = i;
                Account {
                    identity: id,
                    native_id: 500 + i as u128,
                    ledger: if i <= 4 { 9 } else { 3 },
                    normal_credit: i % 2 == 0,
                }
            })
            .collect(),
    };
    assert_eq!(r.bytes().unwrap(), bytes);
    let amounts = [40, 60, 70, 30, 70, 30, 20, 80];
    let postings: Vec<_> = (1u8..=8)
        .map(|i| {
            let mut account = [0; 32];
            account[31] = i;
            Posting {
                account,
                credit: [3, 4, 7, 8].contains(&i),
                amount: amounts[i as usize - 1],
            }
        })
        .collect();
    let plan = compile(&r, CLUSTER, 1, 1000, &postings).unwrap();
    let expected = include_str!("fixtures/node-books-50-first-pairs.tsv");
    for (t, line) in plan.transfers().iter().zip(expected.lines()) {
        let row: Vec<u128> = line.split('\t').map(|v| v.parse().unwrap()).collect();
        assert_eq!(
            [
                t.id,
                t.ledger as u128,
                t.debit_account_id,
                t.credit_account_id,
                t.amount
            ],
            row.as_slice()
        );
        assert_eq!(t.user_data_128, 1000);
        assert_eq!(t.user_data_64, 1);
    }
    assert_eq!(plan.transfers().len(), expected.lines().count());
}

#[test]
fn conflicting_concurrent_sink_cursor_and_process_crash_fail_closed() {
    let d = tempdir().unwrap();
    let ev = d.path().join("e.sqlite");
    let db = d.path().join("s.sqlite");
    let p = plan(100);
    let p2 = plan(200);
    let mut e = evidence(&ev);
    retain(&mut e, &p);
    retain(&mut e, &p2);
    let mut a = sink(&db, &ev, 10);
    let mut b = sink(&db, &ev, 10);
    let g = group(&p, 1000);
    a.commit(0, &g).unwrap();
    assert_eq!(b.commit(0, &group(&p2, 2000)), Err(Error::Conflict));
    b.commit(0, &g).unwrap();
    drop(a);
    drop(b);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "sql_crash_worker", "--ignored"])
        .env("HAWALA_SQL_CRASH_PATH", &db)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(23));
    let mut s = sink(&db, &ev, 10);
    assert_eq!(s.audit().unwrap(), 1);
    assert_eq!(s.watermark(CLUSTER, 1).unwrap(), 1002);
}
#[test]
#[ignore = "helper invoked with explicit temporary database by process-crash test"]
fn sql_crash_worker() {
    let Some(path) = std::env::var_os("HAWALA_SQL_CRASH_PATH") else {
        return;
    };
    let c = Connection::open(path).unwrap();
    c.execute_batch(
        "BEGIN IMMEDIATE; DELETE FROM outcomes; UPDATE cursor SET last=x'0000000000000000';",
    )
    .unwrap();
    std::process::exit(23);
}


#[test]
fn durable_ports_reject_temporary_memory_and_uri_profiles() {
    use std::path::Path;
    let d = tempdir().unwrap();
    let ev = d.path().join("evidence.sqlite");
    for name in [
        "",
        ":memory:",
        "file::memory:?cache=shared",
        "file:hawala-volatile?mode=memory&cache=shared",
        "file:hawala-unsafe.sqlite?vfs=unix-none",
    ] {
        assert!(matches!(
            EvidenceStore::open(Path::new(name), registry(), CLUSTER, 1, key().verifying_key()),
            Err(Error::Port)
        ));
        assert!(matches!(
            SignedSink::open(
                Path::new(name),
                evidence(&ev),
                [10; 32],
                ed25519_dalek::SigningKey::from_bytes(&[10; 32])
            ),
            Err(Error::Port)
        ));
    }
    let p = plan(100);
    retain(&mut evidence(&ev), &p);
    let archive = d.path().join("archive.sqlite");
    let mut destination = sink(&archive, &ev, 10);
    destination.commit(0, &group(&p, 1000)).unwrap();
    drop(destination);
    assert_eq!(sink(&archive, &ev, 10).audit().unwrap(), 1);
}
