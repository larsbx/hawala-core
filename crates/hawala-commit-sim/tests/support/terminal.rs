use super::coordinated::Lab;
use super::*;
use hawala_commit_sim::test_execution::*;
use hawala_commit_sim::{
    SyntheticDispatch, TestDispatchDecision, TestTerminalEvidenceDecision as Evidence,
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Barrier},
};

fn policy() -> TestRailPolicy {
    TestRailPolicy {
        network_id: [60; 32],
        rail_id: [99; 32],
        controller: id(90),
        epoch: 7,
    }
}
fn committed(path: &Path, hm: bool, cm: bool) -> Lab {
    let l = Lab::new(path, hm, cm);
    l.both();
    assert!(matches!(l.admit().unwrap(), Admission::Committed(_)));
    l.sim
        .pin_test_terminal_authority([10; 32], policy())
        .unwrap();
    l
}
fn ticket(l: &Lab) -> SyntheticDispatch {
    match l
        .sim
        .claim_test_dispatch([10; 32], [22; 32], l.f.qr.request().verifier, 3, 150)
        .unwrap()
    {
        TestDispatchDecision::Claimed(t) => t,
        other => panic!("{other:?}"),
    }
}
fn receipt(d: TestRailDecision) -> SignedTestTerminal {
    match d {
        TestRailDecision::Terminal(r) => r,
        TestRailDecision::Held => panic!("not terminal"),
    }
}
fn snapshot(path: &Path) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
    let db = rusqlite::Connection::open(path).unwrap();
    ["SELECT * FROM sim_state ORDER BY id","SELECT * FROM sim_accounts ORDER BY id",
     "SELECT * FROM sim_events ORDER BY event_id","SELECT * FROM sim_postings ORDER BY event_id,idx",
     "SELECT * FROM admission_lots ORDER BY id","SELECT * FROM admission_custody ORDER BY account",
     "SELECT * FROM coordinated_operations ORDER BY transfer","SELECT * FROM coordinated_allocations ORDER BY transfer,side",
     "SELECT transfer,binding,lot,custody_request,custody_digest FROM admission_receipts ORDER BY transfer",
     "SELECT * FROM recovery_subjects ORDER BY transfer",
     "SELECT transfer,attempt,queued_at,expires_at FROM test_outbox ORDER BY transfer"].into_iter().map(|sql| {
        let mut stmt=db.prepare(sql).unwrap();let n=stmt.column_count();
        stmt.query_map([],|r|(0..n).map(|i|r.get(i)).collect::<rusqlite::Result<Vec<_>>>()).unwrap()
            .collect::<rusqlite::Result<Vec<_>>>().unwrap()
    }).collect()
}

#[test]
fn lean_execution_paths_match_durable_rail_for_all_peer_modes() {
    let traces: BTreeMap<String, [u64; 3]> =
        include_str!("../../../../formal/traces/testExecution.tsv")
            .lines()
            .filter(|l| !l.starts_with('#'))
            .map(|line| {
                let mut fields = line.split('\t');
                let path = fields.next().unwrap().to_owned();
                let values: Vec<u64> = fields.map(|v| v.parse().unwrap()).collect();
                (path, values.try_into().unwrap())
            })
            .collect();
    assert_eq!(traces.len(), 35);
    for (hm, cm) in [(false, false), (false, true), (true, false), (true, true)] {
        let other = tempfile::tempdir().unwrap();
        let other = committed(&other.path().join("other.db"), hm, !cm);
        let wrong = ticket(&other);
        for path in traces.keys() {
            let dir = tempfile::tempdir().unwrap();
            let finance = dir.path().join("finance.db");
            let file = dir.path().join("rail.db");
            let l = committed(&finance, hm, cm);
            let scope = l.sim.test_execution_scope([10; 32]).unwrap();
            let before = snapshot(&finance);
            let mut rail = TestExecutionRail::create(&file, policy(), scope, key(90)).unwrap();
            let mut held = None;
            let mut prefix = Vec::new();
            for (i, event) in path.split(',').enumerate() {
                match event {
                    "ticket" => {
                        if held.is_none() {
                            held = Some(ticket(&l));
                        }
                    }
                    "invalidTicket" => assert_eq!(
                        l.sim
                            .claim_test_dispatch([10; 32], [22; 32], id(98), 3, 150)
                            .unwrap(),
                        TestDispatchDecision::Rejected
                    ),
                    "execute" => {
                        if let Some(t) = &held {
                            rail.execute(t, [(i + 1) as u8; 32], 1).unwrap();
                        }
                    }
                    "wrongScope" => assert!(matches!(
                        rail.execute(&wrong, [(i + 1) as u8; 32], 1),
                        Err(SimError::Invalid)
                    )),
                    "fence" => {
                        rail.fence_no_effect([(i + 1) as u8; 32], 1).unwrap();
                    }
                    "partition" => {
                        let _lost_ack = rail.terminal_receipt().unwrap();
                    }
                    "restart" => {
                        drop(rail);
                        rail = TestExecutionRail::open(&file, policy(), scope, key(90)).unwrap();
                    }
                    _ => panic!("{event}"),
                }
                prefix.push(event);
                let terminal = rail.terminal_receipt().unwrap();
                let actual = [
                    held.is_some() as u64,
                    terminal.as_ref().map(|r| r.outcome() as u64).unwrap_or(0),
                    rail.mock_effect_count().unwrap(),
                ];
                assert_eq!(
                    actual,
                    traces[&prefix.join(",")],
                    "{path} prefix={prefix:?}"
                );
                assert_eq!(snapshot(&finance), before);
            }
        }
    }
}

#[test]
fn independent_workers_cannot_both_execute_and_certify_no_effect() {
    let dir = tempfile::tempdir().unwrap();
    let l = committed(&dir.path().join("finance.db"), false, false);
    let scope = l.sim.test_execution_scope([10; 32]).unwrap();
    let t = ticket(&l);
    let file = dir.path().join("rail.db");
    let rail = TestExecutionRail::create(&file, policy(), scope, key(90)).unwrap();
    let other = TestExecutionRail::open(&file, policy(), scope, key(90)).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let (a, b) = std::thread::scope(|s| {
        let b = barrier.clone();
        let rail = &rail;
        let t = &t;
        let a = s.spawn(move || {
            b.wait();
            receipt(rail.execute(t, [1; 32], 1).unwrap())
        });
        let b = barrier.clone();
        let b = s.spawn(move || {
            b.wait();
            receipt(other.fence_no_effect([2; 32], 1).unwrap())
        });
        (a.join().unwrap(), b.join().unwrap())
    });
    assert_eq!(a, b);
    assert_eq!(
        rail.mock_effect_count().unwrap(),
        u64::from(a.outcome() == TestTerminalOutcome::MockEffect)
    );
    assert_eq!(receipt(rail.execute(&t, [3; 32], 1).unwrap()), a);
    assert_eq!(receipt(rail.fence_no_effect([4; 32], 1).unwrap()), a);
}

#[test]
fn terminal_receipt_command_and_mock_effect_roll_back_together() {
    let dir = tempfile::tempdir().unwrap();
    let l = committed(&dir.path().join("finance.db"), false, false);
    let scope = l.sim.test_execution_scope([10; 32]).unwrap();
    let t = ticket(&l);
    let file = dir.path().join("rail.db");
    let rail = TestExecutionRail::create(&file, policy(), scope, key(90)).unwrap();
    let db = rusqlite::Connection::open(&file).unwrap();
    db.execute_batch("CREATE TRIGGER fail_command BEFORE INSERT ON rail_commands BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(rail.execute(&t, [1; 32], 1).is_err());
    assert_eq!(rail.mock_effect_count().unwrap(), 0);
    assert!(rail.terminal_receipt().unwrap().is_none());
    db.execute_batch("DROP TRIGGER fail_command").unwrap();
    for (name,sql) in [
        ("ignore_state","CREATE TRIGGER ignore_state BEFORE UPDATE ON rail_state BEGIN SELECT RAISE(IGNORE); END;"),
        ("ignore_command","CREATE TRIGGER ignore_command BEFORE INSERT ON rail_commands BEGIN SELECT RAISE(IGNORE); END;"),
    ] {
        db.execute_batch(sql).unwrap();assert!(rail.execute(&t,[1;32],1).is_err());
        assert!(rail.terminal_receipt().unwrap().is_none());assert_eq!(rail.mock_effect_count().unwrap(),0);
        db.execute_batch(&format!("DROP TRIGGER {name}")).unwrap();
    }
    assert_eq!(
        receipt(rail.execute(&t, [1; 32], 1).unwrap()).outcome(),
        TestTerminalOutcome::MockEffect
    );
    assert_eq!(rail.mock_effect_count().unwrap(), 1);
}

#[test]
fn exact_request_ids_are_permanent_including_stale_requests() {
    let dir = tempfile::tempdir().unwrap();
    let l = committed(&dir.path().join("finance.db"), false, false);
    let scope = l.sim.test_execution_scope([10; 32]).unwrap();
    let t = ticket(&l);
    let file = dir.path().join("rail.db");
    let rail = TestExecutionRail::create(&file, policy(), scope, key(90)).unwrap();
    assert_eq!(
        rail.execute(&t, [1; 32], 99).unwrap(),
        TestRailDecision::Held
    );
    assert!(matches!(
        rail.execute(&t, [1; 32], 1),
        Err(SimError::Conflict)
    ));
    let no = receipt(rail.fence_no_effect([2; 32], 1).unwrap());
    assert_eq!(
        rail.execute(&t, [1; 32], 99).unwrap(),
        TestRailDecision::Held
    );
    assert!(matches!(
        rail.execute(&t, [2; 32], 1),
        Err(SimError::Conflict)
    ));
    assert_eq!(receipt(rail.execute(&t, [3; 32], 1).unwrap()), no);
    assert_eq!(rail.mock_effect_count().unwrap(), 0);
    assert!(TestExecutionRail::create(&file, policy(), scope, key(90)).is_err());
    assert!(matches!(
        TestExecutionRail::open(
            &file,
            TestRailPolicy {
                epoch: 8,
                ..policy()
            },
            scope,
            key(90)
        ),
        Err(SimError::Conflict)
    ));
    assert!(
        TestExecutionRail::open(dir.path().join("missing.db"), policy(), scope, key(90)).is_err()
    );
}

#[test]
fn signed_terminal_evidence_never_releases_financial_state_and_blocks_pending_claim() {
    for (hm, cm) in [(false, false), (false, true), (true, false), (true, true)] {
        let dir = tempfile::tempdir().unwrap();
        let finance = dir.path().join("finance.db");
        let l = committed(&finance, hm, cm);
        let scope = l.sim.test_execution_scope([10; 32]).unwrap();
        let before = snapshot(&finance);
        let rail = TestExecutionRail::create(dir.path().join("rail.db"), policy(), scope, key(90))
            .unwrap();
        let no = receipt(rail.fence_no_effect([1; 32], 1).unwrap());
        assert_eq!(
            l.sim
                .record_test_terminal_evidence(&no, &UnsupportedRealTerminalAuthority)
                .unwrap(),
            Evidence::Held
        );
        assert!(!l.sim.is_uncertain([10; 32]).unwrap());
        assert_eq!(
            l.sim
                .record_test_terminal_evidence(&no, &SignedTestRailAuthority)
                .unwrap(),
            Evidence::RecordedHeld
        );
        assert_eq!(
            l.sim
                .record_test_terminal_evidence(&no, &SignedTestRailAuthority)
                .unwrap(),
            Evidence::ReplayHeld
        );
        assert!(l.sim.is_uncertain([10; 32]).unwrap());
        assert_eq!(l.admit().unwrap(), Admission::Held);
        assert_eq!(
            l.sim
                .claim_test_dispatch([10; 32], [22; 32], l.f.qr.request().verifier, 3, 150)
                .unwrap(),
            TestDispatchDecision::Held
        );
        assert_eq!(snapshot(&finance), before);
        assert!(matches!(
            l.sim.pin_test_terminal_authority(
                [10; 32],
                TestRailPolicy {
                    epoch: 8,
                    ..policy()
                }
            ),
            Err(SimError::Conflict)
        ));
    }
}

#[test]
fn malformed_untrusted_and_stale_receipts_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let finance = dir.path().join("finance.db");
    let l = committed(&finance, false, false);
    let scope = l.sim.test_execution_scope([10; 32]).unwrap();
    let rail =
        TestExecutionRail::create(dir.path().join("rail.db"), policy(), scope, key(90)).unwrap();
    let r = receipt(rail.fence_no_effect([1; 32], 1).unwrap());
    let frame = r.to_frame();
    assert_eq!(frame.len(), 340);
    assert!(SignedTestTerminal::from_frame(
        &report(4, ObservationKind::ReportedNoDelivery).encode()
    )
    .is_err());
    assert!(SignedTestTerminal::from_frame(&frame[..339]).is_err());
    let mut extra = frame.clone();
    extra.push(0);
    assert!(SignedTestTerminal::from_frame(&extra).is_err());
    let before = snapshot(&finance);
    let mut bad = frame.clone();
    bad[339] ^= 1;
    let bad = SignedTestTerminal::from_frame(&bad).unwrap();
    assert_eq!(
        l.sim
            .record_test_terminal_evidence(&bad, &SignedTestRailAuthority)
            .unwrap(),
        Evidence::Rejected
    );
    let stale = TestExecutionRail::create(
        dir.path().join("stale.db"),
        TestRailPolicy {
            epoch: 8,
            ..policy()
        },
        scope,
        key(90),
    )
    .unwrap();
    let stale = receipt(stale.fence_no_effect([1; 32], 1).unwrap());
    assert_eq!(
        l.sim
            .record_test_terminal_evidence(&stale, &SignedTestRailAuthority)
            .unwrap(),
        Evidence::Rejected
    );
    assert_eq!(snapshot(&finance), before);
    assert!(!l.sim.is_uncertain([10; 32]).unwrap());
    struct MisconfiguredAuthority;
    impl TestTerminalAuthority for MisconfiguredAuthority {
        fn verify(
            &self,
            r: &SignedTestTerminal,
            scope: TestExecutionScope,
            _: TestRailPolicy,
        ) -> TestTerminalCheck {
            SignedTestRailAuthority.verify(r, scope, r.policy())
        }
    }
    assert!(matches!(
        l.sim
            .record_test_terminal_evidence(&stale, &MisconfiguredAuthority),
        Err(SimError::Invalid)
    ));
    for (offset, value) in [(0, 0), (259, 3), (267, 3), (275, 1)] {
        let mut malformed = frame.clone();
        malformed[offset] = value;
        assert!(SignedTestTerminal::from_frame(&malformed).is_err());
    }
    let wrong_dir = tempfile::tempdir().unwrap();
    let wrong = committed(&wrong_dir.path().join("other.db"), false, true);
    let wrong = TestExecutionRail::create(
        wrong_dir.path().join("rail.db"),
        policy(),
        wrong.sim.test_execution_scope([10; 32]).unwrap(),
        key(90),
    )
    .unwrap();
    let wrong = receipt(wrong.fence_no_effect([1; 32], 1).unwrap());
    assert_eq!(
        l.sim
            .record_test_terminal_evidence(&wrong, &SignedTestRailAuthority)
            .unwrap(),
        Evidence::Rejected
    );
}

#[test]
fn preview_pending_and_legacy_dispatch_cannot_authorize_the_scoped_rail() {
    let dir = tempfile::tempdir().unwrap();
    let pending = Lab::new(&dir.path().join("pending.db"), false, false);
    assert!(pending.sim.test_execution_scope([10; 32]).is_err());
    assert!(pending
        .sim
        .pin_test_terminal_authority([10; 32], policy())
        .is_err());
    pending.both();
    assert!(pending.sim.test_execution_scope([10; 32]).is_err());
    let f = fixture(false, false, false);
    let (lot, r, c) = custody_fixture(&f);
    let legacy = Simulator::open(dir.path().join("legacy.db")).unwrap();
    seeded(&legacy, 3, 20, 1000);
    legacy.register_test_lot(&lot).unwrap();
    assert!(matches!(
        legacy.admit(&f.envelope(), &f.policy, &c, &r, 140).unwrap(),
        Admission::Committed(_)
    ));
    let t = match legacy
        .claim_test_dispatch([10; 32], [22; 32], id(4), 3, 150)
        .unwrap()
    {
        TestDispatchDecision::Claimed(t) => t,
        _ => panic!(),
    };
    assert!(t.test_execution_scope().is_none());
    let l = committed(&dir.path().join("current.db"), false, false);
    let rail = TestExecutionRail::create(
        dir.path().join("rail.db"),
        policy(),
        l.sim.test_execution_scope([10; 32]).unwrap(),
        key(90),
    )
    .unwrap();
    assert!(matches!(
        rail.execute(&t, [1; 32], 1),
        Err(SimError::Invalid)
    ));
    assert_eq!(rail.mock_effect_count().unwrap(), 0);
    let scope = l.sim.test_execution_scope([10; 32]).unwrap();
    let t = ticket(&l);
    let wrong = TestExecutionRail::create(
        dir.path().join("wrong-rail.db"),
        TestRailPolicy {
            rail_id: [100; 32],
            ..policy()
        },
        scope,
        key(90),
    )
    .unwrap();
    assert!(matches!(
        wrong.execute(&t, [1; 32], 1),
        Err(SimError::Invalid)
    ));
    assert_eq!(wrong.mock_effect_count().unwrap(), 0);
    assert!(matches!(pending.admit().unwrap(), Admission::Committed(_)));
    let unenrolled = ticket(&pending);
    assert!(unenrolled.test_terminal_policy().is_none());
    pending
        .sim
        .pin_test_terminal_authority([10; 32], policy())
        .unwrap();
    let enrolled_late = TestExecutionRail::create(
        dir.path().join("late.db"),
        policy(),
        pending.sim.test_execution_scope([10; 32]).unwrap(),
        key(90),
    )
    .unwrap();
    assert!(matches!(
        enrolled_late.execute(&unenrolled, [1; 32], 1),
        Err(SimError::Invalid)
    ));
}

#[test]
fn equivocated_signed_receipts_are_retained_and_never_settle_or_release() {
    let dir = tempfile::tempdir().unwrap();
    let finance = dir.path().join("finance.db");
    let mut l = committed(&finance, false, false);
    let scope = l.sim.test_execution_scope([10; 32]).unwrap();
    let t = ticket(&l);
    let before = snapshot(&finance);
    let real =
        TestExecutionRail::create(dir.path().join("rail.db"), policy(), scope, key(90)).unwrap();
    let executed = receipt(real.execute(&t, [1; 32], 1).unwrap());
    assert_eq!(
        l.sim
            .record_test_terminal_evidence(&executed, &SignedTestRailAuthority)
            .unwrap(),
        Evidence::RecordedHeld
    );
    // Deliberately violate the unique-authority-store assumption. Both keys
    // sign correctly: signatures alone cannot establish an honest custody rail.
    let fork =
        TestExecutionRail::create(dir.path().join("fork.db"), policy(), scope, key(90)).unwrap();
    let false_no = receipt(fork.fence_no_effect([2; 32], 1).unwrap());
    assert_eq!(
        l.sim
            .record_test_terminal_evidence(&false_no, &SignedTestRailAuthority)
            .unwrap(),
        Evidence::ConflictHeld
    );
    drop(l.sim);
    l.sim = Simulator::open(&finance).unwrap();
    assert_eq!(
        l.sim
            .record_test_terminal_evidence(&false_no, &SignedTestRailAuthority)
            .unwrap(),
        Evidence::ConflictHeld
    );
    let db = rusqlite::Connection::open(&finance).unwrap();
    let primary: Vec<u8> = db
        .query_row("SELECT frame FROM terminal_test_evidence", [], |r| r.get(0))
        .unwrap();
    assert_eq!(primary, executed.to_frame());
    let conflict: Vec<u8> = db
        .query_row("SELECT frame FROM terminal_test_conflicts", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(conflict, false_no.to_frame());
    assert_eq!(snapshot(&finance), before);
    assert_eq!(
        l.sim
            .claim_test_dispatch([10; 32], [22; 32], id(4), 3, 150)
            .unwrap(),
        TestDispatchDecision::Held
    );
}

#[test]
fn evidence_and_uncertainty_cannot_commit_partially() {
    let dir = tempfile::tempdir().unwrap();
    let finance = dir.path().join("finance.db");
    let l = committed(&finance, false, false);
    let scope = l.sim.test_execution_scope([10; 32]).unwrap();
    let rail =
        TestExecutionRail::create(dir.path().join("rail.db"), policy(), scope, key(90)).unwrap();
    let no = receipt(rail.fence_no_effect([1; 32], 1).unwrap());
    let before = snapshot(&finance);
    let db = rusqlite::Connection::open(&finance).unwrap();
    db.execute_batch("CREATE TRIGGER fail_hold BEFORE UPDATE ON admission_receipts BEGIN SELECT RAISE(ABORT,'injected hold failure'); END;").unwrap();
    assert!(l
        .sim
        .record_test_terminal_evidence(&no, &SignedTestRailAuthority)
        .is_err());
    assert_eq!(snapshot(&finance), before);
    assert!(!l.sim.is_uncertain([10; 32]).unwrap());
    assert_eq!(
        db.query_row::<u64, _, _>("SELECT count(*) FROM terminal_test_evidence", [], |r| r
            .get(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_hold").unwrap();
    assert_eq!(
        l.sim
            .record_test_terminal_evidence(&no, &SignedTestRailAuthority)
            .unwrap(),
        Evidence::RecordedHeld
    );
}

#[test]
fn terminal_crash_child() {
    let Some(finance) = std::env::var_os("HAWALA_TEST_RAIL_FINANCE") else {
        return;
    };
    let rail = std::env::var_os("HAWALA_TEST_RAIL_FILE").unwrap();
    let sim = Simulator::open(finance).unwrap();
    let scope = sim.test_execution_scope([10; 32]).unwrap();
    let rail = TestExecutionRail::open(rail, policy(), scope, key(90)).unwrap();
    if std::env::var("HAWALA_TEST_RAIL_FENCE").unwrap() == "1" {
        rail.fence_no_effect([201; 32], 1).unwrap();
    } else {
        let t = match sim
            .claim_test_dispatch([10; 32], [22; 32], id(4), 3, 150)
            .unwrap()
        {
            TestDispatchDecision::Claimed(t) => t,
            _ => panic!(),
        };
        rail.execute(&t, [201; 32], 1).unwrap();
    }
    std::process::exit(84);
}
#[test]
fn abrupt_exit_lost_ack_and_late_execution_keep_one_terminal_outcome() {
    for fence in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let finance = dir.path().join("finance.db");
        let file = dir.path().join("rail.db");
        let mut l = committed(&finance, false, false);
        let scope = l.sim.test_execution_scope([10; 32]).unwrap();
        let rail = TestExecutionRail::create(&file, policy(), scope, key(90)).unwrap();
        drop(rail);
        drop(l.sim);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "terminal::terminal_crash_child", "--nocapture"])
            .env("HAWALA_TEST_RAIL_FINANCE", &finance)
            .env("HAWALA_TEST_RAIL_FILE", &file)
            .env("HAWALA_TEST_RAIL_FENCE", if fence { "1" } else { "0" })
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(84));
        l.sim = Simulator::open(&finance).unwrap();
        let rail = TestExecutionRail::open(&file, policy(), scope, key(90)).unwrap();
        let r = rail.terminal_receipt().unwrap().unwrap();
        assert_eq!(rail.mock_effect_count().unwrap(), u64::from(!fence));
        if fence {
            let t = ticket(&l);
            assert_eq!(receipt(rail.execute(&t, [202; 32], 1).unwrap()), r);
        }
        assert_eq!(
            l.sim
                .record_test_terminal_evidence(&r, &SignedTestRailAuthority)
                .unwrap(),
            Evidence::RecordedHeld
        );
        assert_eq!(
            l.sim
                .claim_test_dispatch([10; 32], [22; 32], id(4), 3, 150)
                .unwrap(),
            TestDispatchDecision::Held
        );
        assert_eq!(l.sim.balance([42; 32]).unwrap(), Some(100));
        assert_eq!(l.sim.balance([43; 32]).unwrap(), Some(900));
    }
}
