use hawala_cross_authority_test::{test_workflow_gate, Event, Projection, TestHost, FIELDS};
use rusqlite::Connection;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Barrier,
    },
    time::{SystemTime, UNIX_EPOCH},
};
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "hawala-host-test-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn cluster(&self) -> PathBuf {
        self.0.join("cluster")
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn ready(host: &mut TestHost) {
    for (i, e) in [
        "sendPrepare",
        "prepareL(0,0,0)",
        "prepareR(0,0,0)",
        "receiveL",
        "receiveR",
    ]
    .iter()
    .enumerate()
    {
        host.apply(&format!("ready-{i}"), e).unwrap();
    }
}

#[test]
fn every_lean_prefix_replays_through_separate_durable_journals() {
    let mut lines = include_str!("../../../formal/traces/crossAuthority.tsv").lines();
    let header: Vec<_> = lines.next().unwrap().split('\t').collect();
    assert_eq!(&header[3..], &FIELDS);
    let mut scenario = String::new();
    let mut dir = None;
    let mut host = None;
    let mut count = 0;
    let mut scenarios = 0;
    for line in lines {
        let cols: Vec<_> = line.split('\t').collect();
        assert_eq!(cols.len(), 27);
        assert_eq!(cols[1].parse::<usize>().unwrap(), count);
        if scenario != cols[0] {
            scenario = cols[0].into();
            scenarios += 1;
            let next = Directory::new();
            host = Some(TestHost::create(next.cluster()).unwrap());
            dir = Some(next);
        }
        let h = host.as_mut().unwrap();
        let id = format!("prefix-{count}");
        let receipt = h.apply(&id, cols[2]).unwrap();
        let expected = Projection(
            cols[3..]
                .iter()
                .map(|v| match *v {
                    "true" => 1,
                    "false" => 0,
                    _ => v.parse().unwrap(),
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        );
        assert_eq!(
            h.projection().unwrap(),
            expected,
            "scenario {scenario}, prefix {count}, {}",
            cols[2]
        );
        assert!(expected.safe());
        if let Some(first) = receipt {
            let repeated = h.apply(&id, cols[2]).unwrap().unwrap();
            assert!(repeated.replayed);
            assert_eq!(first.local_state, repeated.local_state);
            assert_eq!(first.version, repeated.version);
            assert_eq!(h.projection().unwrap(), expected);
        }
        h.reopen().unwrap();
        assert_eq!(
            h.projection().unwrap(),
            expected,
            "durable reopen at {count}"
        );
        count += 1;
    }
    assert_eq!(count, 232);
    assert_eq!(scenarios, 13);
    drop(host);
    drop(dir);
}
#[test]
fn all_lean_workflow_gates_match_and_unknown_inputs_fail_closed() {
    let rows: Vec<_> = include_str!("../../../formal/traces/financialGates.tsv")
        .lines()
        .skip(1)
        .collect();
    assert_eq!(rows.len(), 72);
    for row in rows {
        let c: Vec<_> = row.split('\t').collect();
        assert_eq!(
            test_workflow_gate(c[0] == "true", c[1], c[2]).unwrap(),
            c[3] == "true",
            "{row}"
        );
    }
    assert!(test_workflow_gate(true, "Settled", "checkoutFulfill").is_err());
    assert!(test_workflow_gate(true, "settled", "shipAnyway").is_err());
}
#[test]
fn replayed_held_and_terminal_receipts_never_roll_back_current_state() {
    let dir = Directory::new();
    let mut h = TestHost::create(dir.cluster()).unwrap();
    let held = h.apply("early", "commit").unwrap().unwrap();
    assert_eq!(h.projection().unwrap().0[6], 0);
    ready(&mut h);
    h.apply("terminal", "commit").unwrap();
    let current = h.projection().unwrap();
    let old = h.apply("early", "commit").unwrap().unwrap();
    assert!(old.replayed);
    assert_eq!(old.local_state, held.local_state);
    assert_eq!(h.projection().unwrap(), current);
    assert!(h.apply("early", "abort").is_err());
    h.reopen().unwrap();
    assert!(h.apply("terminal", "abort").is_err());
    h.apply("late-abort", "abort").unwrap();
    assert_eq!(h.projection().unwrap().0[6], 1);
}
#[test]
fn receipt_insert_failure_rolls_back_state_and_can_be_queried_safely() {
    let dir = Directory::new();
    let mut h = TestHost::create(dir.cluster()).unwrap();
    ready(&mut h);
    let before = h.projection().unwrap();
    let db = Connection::open(dir.cluster().join("coordinator.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_receipt BEFORE INSERT ON receipts BEGIN SELECT RAISE(ABORT,'injected storage failure'); END;").unwrap();
    assert!(h.apply("commit-once", "commit").is_err());
    assert_eq!(h.projection().unwrap(), before);
    db.execute_batch("DROP TRIGGER reject_receipt;").unwrap();
    h.apply("commit-once", "commit").unwrap();
    assert_eq!(h.projection().unwrap().0[6], 1);
}
#[test]
fn missing_or_wrong_role_store_cannot_be_silently_reenrolled() {
    let dir = Directory::new();
    let h = TestHost::create(dir.cluster()).unwrap();
    drop(h);
    std::fs::remove_file(dir.cluster().join("left.db")).unwrap();
    assert!(TestHost::open_existing(dir.cluster()).is_err());
    std::fs::copy(
        dir.cluster().join("coordinator.db"),
        dir.cluster().join("left.db"),
    )
    .unwrap();
    assert!(TestHost::open_existing(dir.cluster()).is_err());
}
#[test]
fn invalid_actions_cannot_mutate_a_store() {
    let dir = Directory::new();
    let mut h = TestHost::create(dir.cluster()).unwrap();
    let before = h.projection().unwrap();
    for text in [
        "prepareL",
        "prepareL(0,0)",
        "prepareL(00,0,0)",
        "prepareL(-1,0,0)",
        "prepareL(0,0,0)junk",
        "settle",
        "timeout(0)",
    ] {
        assert!(Event::parse(text).is_err());
        assert!(h.apply("bad", text).is_err());
    }
    assert_eq!(before, h.projection().unwrap());
}
#[test]
fn competing_connections_keep_one_terminal_decision() {
    let dir = Directory::new();
    let mut h = TestHost::create(dir.cluster()).unwrap();
    ready(&mut h);
    let barrier = Arc::new(Barrier::new(2));
    let mut threads = Vec::new();
    for event in ["commit", "abort"] {
        let path = dir.cluster();
        let b = barrier.clone();
        threads.push(std::thread::spawn(move || {
            let mut host = TestHost::open_existing(path).unwrap();
            b.wait();
            host.apply(event, event).is_ok()
        }));
    }
    let accepted: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert!(accepted.iter().any(|v| *v));
    assert!([1, 2].contains(&h.projection().unwrap().0[6]));
    assert_eq!(h.projection().unwrap().0[9], 0);
    assert_eq!(h.projection().unwrap().0[10], 0);
}
#[test]
fn process_exit_after_commit_child() {
    if let Some(path) = std::env::var_os("HAWALA_TEST_CHILD_STORE") {
        let mut h = TestHost::open_existing(path).unwrap();
        h.apply("unacknowledged", "commit").unwrap();
        std::process::exit(17);
    }
}
#[test]
fn lost_ack_and_abrupt_process_exit_preserve_commit_and_peer_hold() {
    let dir = Directory::new();
    let mut h = TestHost::create(dir.cluster()).unwrap();
    ready(&mut h);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "process_exit_after_commit_child"])
        .env("HAWALA_TEST_CHILD_STORE", dir.cluster())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(17));
    let mut restored = TestHost::open_existing(dir.cluster()).unwrap();
    let p = restored.projection().unwrap();
    assert_eq!(p.0[6], 1);
    assert!(p.held(true) && p.held(false));
    assert!(
        restored
            .apply("unacknowledged", "commit")
            .unwrap()
            .unwrap()
            .replayed
    );
    restored.apply("cannot-abort", "abort").unwrap();
    assert_eq!(restored.projection().unwrap().0[6], 1);
    restored.apply("publish", "publish").unwrap();
    restored.apply("left-delivery", "deliverL").unwrap();
    assert_eq!(restored.projection().unwrap().0[10], 0);
}
#[test]
fn a_deliberately_corrupted_expected_trace_is_detected() {
    let dir = Directory::new();
    let mut h = TestHost::create(dir.cluster()).unwrap();
    h.apply("first", "sendPrepare").unwrap();
    let real = h.projection().unwrap();
    let mut mutated = real.clone();
    mutated.0[6] = 1;
    assert_ne!(
        real, mutated,
        "negative control must detect a false commit projection"
    );
    assert!(!mutated.safe());
}
