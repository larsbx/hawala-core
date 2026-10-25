use ed25519_dalek::SigningKey;
use hawala_commit_sim::{coordination::*, Error};
use hawala_identity_core::IdentityId;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Barrier},
};

fn binding() -> TestBinding {
    TestBinding {
        transfer: [1; 32],
        terms_digest: [2; 32],
        origin: IdentityId::from_signing_key(&SigningKey::from_bytes(&[3; 32])),
        disburser: IdentityId::from_signing_key(&SigningKey::from_bytes(&[4; 32])),
        left_allocation: [5; 32],
        right_allocation: [6; 32],
        epoch: 7,
    }
}
fn command(c: &TestCoordinator, id: u8, action: TestAction) -> TestCommand {
    TestCommand {
        request_id: [id; 32],
        binding_digest: binding().digest().unwrap(),
        epoch: 7,
        expected_version: c.state().unwrap().version,
        action,
    }
}
fn apply(c: &TestCoordinator, id: u8, action: TestAction) -> TestReceipt {
    c.apply(command(c, id, action)).unwrap()
}
fn open(p: &Path) -> TestCoordinator {
    TestCoordinator::open(p, binding()).unwrap()
}
fn projection(s: TestState) -> [u64; 5] {
    [
        s.decision as u64,
        s.left_locked as u64,
        s.right_locked as u64,
        s.left_released as u64,
        s.right_released as u64,
    ]
}

#[test]
fn lean_generated_coordination_paths_match_durable_rust_every_prefix() {
    let traces: BTreeMap<String, [u64; 5]> =
        include_str!("../../../formal/traces/coordination.tsv")
            .lines()
            .filter(|l| !l.starts_with('#'))
            .map(|line| {
                let mut fields = line.split('\t');
                let path = fields.next().unwrap().to_owned();
                let values: Vec<u64> = fields.map(|v| v.parse().unwrap()).collect();
                (path, values.try_into().unwrap())
            })
            .collect();
    assert_eq!(traces.len(), 126);
    for path in traces.keys() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("coordination.db");
        let mut c = open(&file);
        let mut prefix = Vec::new();
        for (i, event) in path.split(',').enumerate() {
            let before = c.state().unwrap();
            if event == "restart" {
                drop(c);
                c = open(&file);
            } else {
                let action = match event {
                    "left" => TestAction::ConfirmLeft,
                    "right" => TestAction::ConfirmRight,
                    "commit" => TestAction::Commit,
                    "abort" => TestAction::Abort,
                    "releaseLeft" => TestAction::ReleaseLeft,
                    "releaseRight" => TestAction::ReleaseRight,
                    "timeout" | "denied" => TestAction::Timeout,
                    _ => panic!("{event}"),
                };
                let mut cmd = command(&c, (i + 1) as u8, action);
                if event == "denied" {
                    cmd.binding_digest = [255; 32];
                }
                let receipt = c.apply(cmd).unwrap();
                assert!(!receipt.replayed);
                assert_eq!(
                    receipt.result == TestResult::Changed,
                    projection(before) != projection(receipt.state)
                );
                let replay = c.apply(cmd).unwrap();
                assert!(replay.replayed);
                assert_eq!(replay.state, receipt.state);
                assert_eq!(replay.result, receipt.result);
                let changed = projection(before) != projection(receipt.state);
                assert_eq!(receipt.state.version, before.version + u64::from(changed));
            }
            prefix.push(event);
            assert_eq!(
                projection(c.state().unwrap()),
                traces[&prefix.join(",")],
                "{path} prefix={prefix:?}"
            );
        }
    }
}

#[test]
fn partition_and_late_confirmation_cannot_turn_abort_into_commit() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("coord.db");
    let c = open(&file);
    apply(&c, 1, TestAction::ConfirmLeft);
    assert_eq!(apply(&c, 2, TestAction::Commit).result, TestResult::Held);
    for id in 3..8 {
        assert_eq!(apply(&c, id, TestAction::Timeout).result, TestResult::Held);
    }
    assert!(!c.state().unwrap().left_released);
    apply(&c, 8, TestAction::Abort);
    apply(&c, 9, TestAction::ReleaseLeft);
    drop(c);
    let c = open(&file);
    apply(&c, 10, TestAction::ConfirmRight); // reservation acknowledgement arrived late
    assert!(!c.state().unwrap().right_released);
    assert_eq!(apply(&c, 11, TestAction::Commit).result, TestResult::Held);
    apply(&c, 12, TestAction::ReleaseRight);
    assert_eq!(projection(c.state().unwrap()), [2, 1, 1, 1, 1]);
}

#[test]
fn commit_keeps_both_reservations_on_timeout_abort_and_release() {
    let dir = tempfile::tempdir().unwrap();
    let c = open(&dir.path().join("coord.db"));
    apply(&c, 1, TestAction::ConfirmLeft);
    apply(&c, 2, TestAction::ConfirmRight);
    apply(&c, 3, TestAction::Commit);
    for (i, action) in [
        TestAction::Timeout,
        TestAction::Abort,
        TestAction::ReleaseLeft,
        TestAction::ReleaseRight,
        TestAction::Commit,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(apply(&c, (i + 4) as u8, action).result, TestResult::Held);
    }
    assert_eq!(projection(c.state().unwrap()), [1, 1, 1, 0, 0]);
}

#[test]
fn permanent_ids_bind_scope_payload_epoch_and_version_even_for_rejections() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("coord.db");
    let c = open(&file);
    let cmd = command(&c, 1, TestAction::ConfirmLeft);
    let receipt = c.apply(cmd).unwrap();
    let stale = TestCommand {
        request_id: [2; 32],
        action: TestAction::ConfirmRight,
        ..cmd
    };
    assert_eq!(c.apply(stale).unwrap().result, TestResult::Stale);
    let changed = TestCommand {
        expected_version: 2,
        ..stale
    };
    assert!(matches!(c.apply(changed), Err(Error::Conflict)));
    for (id, epoch, digest) in [(3, 6, binding().digest().unwrap()), (4, 7, [99; 32])] {
        let bad = TestCommand {
            request_id: [id; 32],
            epoch,
            binding_digest: digest,
            ..command(&c, id, TestAction::Abort)
        };
        assert_eq!(c.apply(bad).unwrap().result, TestResult::WrongScope);
        assert!(c.apply(bad).unwrap().replayed);
        assert!(matches!(
            c.apply(TestCommand {
                epoch: 7,
                binding_digest: binding().digest().unwrap(),
                ..bad
            }),
            Err(Error::Conflict)
        ));
    }
    drop(c);
    let c = open(&file);
    assert_eq!(
        c.apply(cmd).unwrap(),
        TestReceipt {
            replayed: true,
            ..receipt
        }
    );
    assert!(matches!(
        c.apply(TestCommand {
            action: TestAction::Abort,
            ..cmd
        }),
        Err(Error::Conflict)
    ));
    assert!(matches!(
        TestCoordinator::open(
            &file,
            TestBinding {
                epoch: 8,
                ..binding()
            }
        ),
        Err(Error::Conflict)
    ));
    assert_eq!(projection(c.state().unwrap()), [0, 1, 0, 0, 0]);
}

#[test]
fn competing_connections_cannot_commit_and_abort_the_same_decision() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("coord.db");
    let c = open(&file);
    apply(&c, 1, TestAction::ConfirmLeft);
    apply(&c, 2, TestAction::ConfirmRight);
    let barrier = Arc::new(Barrier::new(2));
    let workers: Vec<_> = [TestAction::Commit, TestAction::Abort]
        .into_iter()
        .enumerate()
        .map(|(i, action)| {
            let worker = open(&file);
            let cmd = command(&worker, (i + 3) as u8, action);
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                worker.apply(cmd).unwrap()
            })
        })
        .collect();
    let replies: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(
        replies
            .iter()
            .filter(|r| r.result == TestResult::Changed)
            .count(),
        1
    );
    assert_eq!(
        replies
            .iter()
            .filter(|r| r.result == TestResult::Stale)
            .count(),
        1
    );
    let terminal = c.state().unwrap();
    assert!(matches!(
        terminal.decision,
        TestDecision::Committed | TestDecision::Aborted
    ));
    assert_eq!(
        apply(&c, 8, TestAction::Commit).state.decision,
        terminal.decision
    );
    assert_eq!(
        apply(&c, 9, TestAction::Abort).state.decision,
        terminal.decision
    );
}

#[test]
fn state_and_receipt_rollback_together_on_storage_failure() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("coord.db");
    let c = open(&file);
    let before = c.state().unwrap();
    let cmd = command(&c, 1, TestAction::Abort);
    let db = rusqlite::Connection::open(&file).unwrap();
    db.execute_batch(
        "CREATE TRIGGER fail_receipt BEFORE INSERT ON test_coordination_requests
        BEGIN SELECT RAISE(ABORT,'injected receipt failure'); END;",
    )
    .unwrap();
    assert!(matches!(c.apply(cmd), Err(Error::Sql(_))));
    assert_eq!(c.state().unwrap(), before);
    db.execute_batch("DROP TRIGGER fail_receipt").unwrap();
    assert!(!c.apply(cmd).unwrap().replayed);
    assert_eq!(c.state().unwrap().decision, TestDecision::Aborted);
}

#[test]
fn abrupt_decision_child() {
    let Ok(file) = std::env::var("HAWALA_TEST_COORD_CRASH_FILE") else {
        return;
    };
    let c = open(Path::new(&file));
    let action = if std::env::var("HAWALA_TEST_COORD_CRASH_COMMIT").unwrap() == "1" {
        TestAction::Commit
    } else {
        TestAction::Abort
    };
    apply(&c, 3, action);
    std::process::exit(81); // bypass destructors immediately after the durable commit
}
#[test]
fn abrupt_exit_recovers_the_same_decision_and_replay_receipt() {
    for commit in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("coord.db");
        let c = open(&file);
        apply(&c, 1, TestAction::ConfirmLeft);
        apply(&c, 2, TestAction::ConfirmRight);
        let action = if commit {
            TestAction::Commit
        } else {
            TestAction::Abort
        };
        let cmd = command(&c, 3, action);
        drop(c);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "abrupt_decision_child", "--nocapture"])
            .env("HAWALA_TEST_COORD_CRASH_FILE", &file)
            .env(
                "HAWALA_TEST_COORD_CRASH_COMMIT",
                if commit { "1" } else { "0" },
            )
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(81));
        let c = open(&file);
        assert!(c.apply(cmd).unwrap().replayed);
        assert_eq!(
            c.state().unwrap().decision,
            if commit {
                TestDecision::Committed
            } else {
                TestDecision::Aborted
            }
        );
        assert_eq!(
            apply(
                &c,
                4,
                if commit {
                    TestAction::Abort
                } else {
                    TestAction::Commit
                }
            )
            .result,
            TestResult::Held
        );
    }
}
