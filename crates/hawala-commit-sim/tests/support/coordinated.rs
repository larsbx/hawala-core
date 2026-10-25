use super::*;
use hawala_commit_sim::coordination::TestDecision;
use hawala_commit_sim::{OriginReservation, TestReservationSide};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Barrier},
};

fn origin_fixture(f: &Fixture) -> (Request, SignedTestCustody) {
    let policy = CustodyPolicy {
        network_id: [60; 32],
        custody_account: [71; 32],
        depositing_dar: id(3),
        currency: *b"USD",
        mandate_digest: [72; 32],
        epoch: 3,
        mode: Mode::IndependentEscrow { controller: id(9) },
    };
    let r = Request {
        network_id: policy.network_id,
        custody_account: policy.custody_account,
        depositing_dar: id(3),
        currency: *b"USD",
        mandate_digest: policy.mandate_digest,
        policy_digest: policy.digest().unwrap(),
        epoch: 3,
        expected_version: 5,
        request_id: [73; 32],
        allocation_id: [74; 32],
        obligation_digest: f.terms.terms().digest(),
        command: Command::Reserve,
        amount_minor: 1000,
    };
    let approvals = vec![sign_request(&policy, &r, &key(9)).unwrap()];
    (
        r,
        SignedTestCustody {
            policy,
            approvals,
            version: 5,
            valid_until: 170,
            capacity_minor: 1000,
        },
    )
}
struct Lab {
    sim: Simulator,
    f: Fixture,
    r: Request,
    c: SignedTestCustody,
    o: Request,
    oc: SignedTestCustody,
    digest: [u8; 32],
}
impl Lab {
    fn new(path: &Path, holder: bool, cash: bool) -> Self {
        let f = fixture(holder, cash, false);
        let (lot, r, c) = custody_fixture(&f);
        let (o, oc) = origin_fixture(&f);
        let sim = Simulator::open(path).unwrap();
        seeded(&sim, 3, 20, 1000);
        sim.register_test_lot(&lot).unwrap();
        let digest = sim
            .prepare_test_coordination(&f.envelope(), &o, &r)
            .unwrap();
        Self {
            sim,
            f,
            r,
            c,
            o,
            oc,
            digest,
        }
    }
    fn confirm(&self, side: TestReservationSide) {
        let version = self.sim.test_coordination_state([10; 32]).unwrap().version;
        let (r, c) = match side {
            TestReservationSide::Origin => (&self.o, &self.oc),
            TestReservationSide::Disburser => (&self.r, &self.c),
        };
        self.sim
            .confirm_test_reservation([10; 32], self.digest, side, r, c, version, 140)
            .unwrap();
    }
    fn both(&self) {
        self.confirm(TestReservationSide::Origin);
        self.confirm(TestReservationSide::Disburser);
    }
    fn admit(&self) -> hawala_commit_sim::Result<Admission> {
        let version = self.sim.test_coordination_state([10; 32])?.version;
        self.sim.admit_coordinated(
            &self.f.envelope(),
            &self.f.policy,
            &OriginReservation {
                request: &self.o,
                authority: &self.oc,
                expected_coordination_version: version,
            },
            &self.c,
            &self.r,
            140,
        )
    }
}
fn projection(l: &Lab, path: &Path) -> [u64; 8] {
    let s = l.sim.test_coordination_state([10; 32]).unwrap();
    let db = rusqlite::Connection::open(path).unwrap();
    let count = |table: &str| {
        db.query_row::<u64, _, _>(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    };
    let events = count("sim_events");
    assert_eq!(count("admission_receipts"), events);
    assert_eq!(count("test_outbox"), events);
    assert_eq!(count("sim_postings"), events * 2);
    assert_eq!(l.sim.state_version().unwrap(), 20 + events);
    assert_eq!(l.sim.balance([41; 32]).unwrap(), Some(1000));
    assert_eq!(
        db.query_row::<u64, _, _>("SELECT used FROM admission_lots", [], |r| r.get(0))
            .unwrap(),
        events
    );
    for (account, start) in [(71u8, 5u64), (61u8, 11u64)] {
        let v: u64 = db
            .query_row(
                "SELECT version FROM admission_custody WHERE account=?1",
                rusqlite::params![[account; 32].as_slice()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v, start + events);
    }
    let epoch: u64 = db
        .query_row("SELECT epoch FROM sim_state", [], |r| r.get(0))
        .unwrap();
    let nonces: u64 = db
        .query_row("SELECT count(DISTINCT nonce) FROM sim_events", [], |r| {
            r.get(0)
        })
        .unwrap();
    [
        s.decision as u64,
        s.left_locked as u64,
        s.right_locked as u64,
        l.sim.balance([42; 32]).unwrap().unwrap(),
        l.sim.balance([43; 32]).unwrap().unwrap(),
        events,
        nonces,
        (epoch == 3) as u64,
    ]
}

#[test]
fn lean_coordinated_paths_match_atomic_admission_for_all_peer_modes() {
    let traces: BTreeMap<String, [u64; 8]> =
        include_str!("../../../../formal/traces/coordinated.tsv")
            .lines()
            .filter(|l| !l.starts_with('#'))
            .map(|line| {
                let mut fields = line.split('\t');
                let path = fields.next().unwrap().to_owned();
                let values: Vec<u64> = fields.map(|v| v.parse().unwrap()).collect();
                (path, values.try_into().unwrap())
            })
            .collect();
    assert_eq!(traces.len(), 162);
    for (hm, cm) in [(false, false), (false, true), (true, false), (true, true)] {
        for path in traces.keys() {
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("coordinated.db");
            let mut l = Lab::new(&file, hm, cm);
            let mut prefix = Vec::new();
            for event in path.split(',') {
                match event {
                    "left" => l.confirm(TestReservationSide::Origin),
                    "right" => l.confirm(TestReservationSide::Disburser),
                    "admit" => {
                        l.admit().unwrap();
                    }
                    "denied" => {
                        let old = l.f.reviews;
                        l.f.reviews.funding_finality = Review::Unknown;
                        l.admit().unwrap();
                        l.f.reviews = old;
                    }
                    "abort" => {
                        let v = l.sim.test_coordination_state([10; 32]).unwrap().version;
                        l.sim
                            .abort_test_coordination([10; 32], l.digest, v)
                            .unwrap();
                    }
                    "fence" => {
                        rusqlite::Connection::open(&file)
                            .unwrap()
                            .execute("UPDATE sim_state SET epoch=4", [])
                            .unwrap();
                    }
                    "restart" => {
                        drop(l.sim);
                        l.sim = Simulator::open(&file).unwrap();
                    }
                    "replay" => {
                        if l.sim.test_coordination_state([10; 32]).unwrap().decision
                            == TestDecision::Committed
                        {
                            assert!(matches!(l.admit().unwrap(), Admission::Committed(_)));
                        }
                    }
                    "legacy" => {
                        assert_eq!(
                            l.sim
                                .admit(&l.f.envelope(), &l.f.policy, &l.c, &l.r, 140)
                                .unwrap(),
                            Admission::Held
                        );
                        let p = match admit_preview(&l.f.envelope(), &book(), 140) {
                            Decision::Prepared(p) => p,
                            _ => panic!(),
                        };
                        assert!(matches!(
                            l.sim.record_simulated_hold(&p, &l.f.reserve),
                            Err(SimError::Invalid)
                        ));
                    }
                    _ => panic!("{event}"),
                }
                prefix.push(event);
                assert_eq!(
                    projection(&l, &file),
                    traces[&prefix.join(",")],
                    "{path} prefix={prefix:?} modes={hm}/{cm}"
                );
            }
        }
    }
}

#[test]
fn decision_and_every_admission_effect_roll_back_on_final_write_failure() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("fault.db");
    let l = Lab::new(&file, false, false);
    l.both();
    let old = l.sim.test_coordination_state([10; 32]).unwrap();
    let before = projection(&l, &file);
    let db = rusqlite::Connection::open(&file).unwrap();
    db.execute_batch(
        "CREATE TRIGGER fail_decision BEFORE UPDATE ON coordinated_operations
        BEGIN SELECT RAISE(ABORT,'decision storage failure'); END;",
    )
    .unwrap();
    assert!(l.admit().is_err());
    assert_eq!(projection(&l, &file), before);
    assert_eq!(l.sim.test_coordination_state([10; 32]).unwrap(), old);
    db.execute_batch("DROP TRIGGER fail_decision").unwrap();
    assert!(matches!(l.admit().unwrap(), Admission::Committed(_)));
    assert_eq!(projection(&l, &file), [1, 1, 1, 100, 900, 1, 1, 1]);
}

#[test]
fn abort_and_admission_share_one_serial_order() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("race.db");
    let l = Lab::new(&file, true, true);
    l.both();
    let barrier = Arc::new(Barrier::new(2));
    std::thread::scope(|scope| {
        let lab = &l;
        let b = barrier.clone();
        let first = scope.spawn(move || {
            b.wait();
            lab.admit().unwrap()
        });
        let lab = &l;
        let b = barrier.clone();
        let second = scope.spawn(move || {
            b.wait();
            lab.sim.abort_test_coordination([10; 32], lab.digest, 3)
        });
        let admitted = first.join().unwrap();
        let aborted = second.join().unwrap().unwrap();
        match l.sim.test_coordination_state([10; 32]).unwrap().decision {
            TestDecision::Committed => {
                assert!(matches!(admitted, Admission::Committed(_)));
                assert_eq!(aborted.decision, TestDecision::Committed);
            }
            TestDecision::Aborted => {
                assert_eq!(admitted, Admission::Held);
                assert_eq!(aborted.decision, TestDecision::Aborted);
            }
            _ => panic!("terminal decision missing"),
        }
    });
    let p = projection(&l, &file);
    assert_eq!(p[0] == 1, p[5] == 1);
}

#[test]
fn both_current_signed_test_authorities_are_required_at_commit() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("controls.db");
    let mut l = Lab::new(&file, false, false);
    let none = l
        .sim
        .confirm_test_reservation(
            [10; 32],
            l.digest,
            TestReservationSide::Origin,
            &l.o,
            &UnsupportedRealCustody,
            1,
            140,
        )
        .unwrap();
    assert!(!none.left_locked);
    assert_eq!(l.admit().unwrap(), Admission::Held);
    l.both();
    let origin = OriginReservation {
        request: &l.o,
        authority: &UnsupportedRealCustody,
        expected_coordination_version: 3,
    };
    assert_eq!(
        l.sim
            .admit_coordinated(&l.f.envelope(), &l.f.policy, &origin, &l.c, &l.r, 140)
            .unwrap(),
        Admission::Held
    );
    let origin = OriginReservation {
        request: &l.o,
        authority: &l.oc,
        expected_coordination_version: 3,
    };
    assert_eq!(
        l.sim
            .admit_coordinated(
                &l.f.envelope(),
                &l.f.policy,
                &origin,
                &UnsupportedRealCustody,
                &l.r,
                140
            )
            .unwrap(),
        Admission::Held
    );
    l.oc.valid_until = 139;
    assert_eq!(l.admit().unwrap(), Admission::Held);
    l.oc.valid_until = 170;
    let db = rusqlite::Connection::open(&file).unwrap();
    db.execute(
        "UPDATE admission_custody SET version=6 WHERE account=?1",
        rusqlite::params![[71u8; 32].as_slice()],
    )
    .unwrap();
    assert_eq!(l.admit().unwrap(), Admission::Held);
    assert_eq!(l.sim.event_count().unwrap(), 0);
}

#[test]
fn abort_tombstone_cannot_be_rebound_or_bypassed_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("abort.db");
    let mut l = Lab::new(&file, false, true);
    l.confirm(TestReservationSide::Origin);
    l.sim
        .abort_test_coordination([10; 32], l.digest, 2)
        .unwrap();
    drop(l.sim);
    l.sim = Simulator::open(&file).unwrap();
    l.confirm(TestReservationSide::Disburser);
    assert_eq!(l.admit().unwrap(), Admission::Held);
    let mut changed = l.o.clone();
    changed.request_id = [75; 32];
    assert!(matches!(
        l.sim
            .prepare_test_coordination(&l.f.envelope(), &changed, &l.r),
        Err(SimError::Conflict)
    ));
    assert!(matches!(
        l.sim.confirm_test_reservation(
            [10; 32],
            l.digest,
            TestReservationSide::Origin,
            &changed,
            &l.oc,
            3,
            140
        ),
        Err(SimError::Conflict)
    ));
    assert_eq!(projection(&l, &file), [2, 1, 1, 1000, 0, 0, 0, 1]);
}

#[test]
fn a_legacy_commit_cannot_acquire_coordinated_authority_retroactively() {
    let f = fixture(false, false, false);
    let (lot, r, c) = custody_fixture(&f);
    let (o, _) = origin_fixture(&f);
    let dir = tempfile::tempdir().unwrap();
    let sim = Simulator::open(dir.path().join("old.db")).unwrap();
    seeded(&sim, 3, 20, 1000);
    sim.register_test_lot(&lot).unwrap();
    assert!(matches!(
        sim.admit(&f.envelope(), &f.policy, &c, &r, 140).unwrap(),
        Admission::Committed(_)
    ));
    assert!(matches!(
        sim.prepare_test_coordination(&f.envelope(), &o, &r),
        Err(SimError::Invalid)
    ));
}

#[test]
fn admission_memory_uri_cannot_bypass_durability() {
    for path in [
        "file::memory:",
        "file:sim?mode=memory&cache=shared",
        ":memory:",
        "",
    ] {
        assert!(matches!(Simulator::open(path), Err(SimError::Invalid)));
    }
}

#[test]
fn stale_coordination_versions_cannot_change_or_commit_preparation() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("version.db");
    let l = Lab::new(&file, false, false);
    assert!(matches!(
        l.sim.confirm_test_reservation(
            [10; 32],
            l.digest,
            TestReservationSide::Origin,
            &l.o,
            &l.oc,
            2,
            140
        ),
        Err(SimError::Stale)
    ));
    assert!(!l.sim.test_coordination_state([10; 32]).unwrap().left_locked);
    l.both();
    assert!(matches!(
        l.sim.abort_test_coordination([10; 32], l.digest, 2),
        Err(SimError::Stale)
    ));
    let stale = OriginReservation {
        request: &l.o,
        authority: &l.oc,
        expected_coordination_version: 2,
    };
    assert_eq!(
        l.sim
            .admit_coordinated(&l.f.envelope(), &l.f.policy, &stale, &l.c, &l.r, 140)
            .unwrap(),
        Admission::Held
    );
    assert_eq!(projection(&l, &file), [0, 1, 1, 1000, 0, 0, 0, 1]);
}

#[test]
fn the_same_custody_checkpoint_cannot_prepare_two_operations() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pledge.db");
    let l = Lab::new(&file, false, false);
    let f = fixture_transfer(false, false, false, 26, 27, 11);
    let (_, mut payout, mut pc) = custody_fixture(&f);
    pc.policy.custody_account = [81; 32];
    payout.custody_account = [81; 32];
    payout.allocation_id = [83; 32];
    payout.policy_digest = pc.policy.digest().unwrap();
    pc.approvals = vec![sign_request(&pc.policy, &payout, &key(6)).unwrap()];
    let (mut origin, mut oc) = origin_fixture(&f);
    origin.request_id = [76; 32];
    origin.allocation_id = [77; 32];
    oc.approvals = vec![sign_request(&oc.policy, &origin, &key(9)).unwrap()];
    use hawala_commit_sim::{Control, CustodyAuthority};
    assert_eq!(oc.verify(&origin, 140), Control::VerifiedTestFixture);
    assert_eq!(pc.verify(&payout, 140), Control::VerifiedTestFixture);
    assert!(matches!(
        l.sim
            .prepare_test_coordination(&f.envelope(), &origin, &payout),
        Err(SimError::Sql(_))
    ));
    let db = rusqlite::Connection::open(&file).unwrap();
    assert_eq!(
        db.query_row::<u64, _, _>("SELECT count(*) FROM coordinated_operations", [], |r| r
            .get(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row::<u64, _, _>("SELECT count(*) FROM coordinated_allocations", [], |r| r
            .get(0))
            .unwrap(),
        2
    );
    assert_eq!(l.sim.event_count().unwrap(), 0);
}

#[test]
fn coordinated_crash_child() {
    let Some(path) = std::env::var_os("HAWALA_TEST_COUPLED_CRASH_DB") else {
        return;
    };
    let f = fixture(false, false, false);
    let (_, r, c) = custody_fixture(&f);
    let (o, oc) = origin_fixture(&f);
    let sim = Simulator::open(path).unwrap();
    let digest = sim
        .prepare_test_coordination(&f.envelope(), &o, &r)
        .unwrap();
    if std::env::var("HAWALA_TEST_COUPLED_ABORT").unwrap() == "1" {
        sim.abort_test_coordination([10; 32], digest, 3).unwrap();
    } else {
        assert!(matches!(
            sim.admit_coordinated(
                &f.envelope(),
                &f.policy,
                &OriginReservation {
                    request: &o,
                    authority: &oc,
                    expected_coordination_version: 3
                },
                &c,
                &r,
                140
            )
            .unwrap(),
            Admission::Committed(_)
        ));
    }
    std::process::exit(82);
}
#[test]
fn abrupt_restart_keeps_abort_or_commit_coupled_and_uncertain_execution_locked() {
    for abort in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("crash.db");
        let mut l = Lab::new(&file, false, false);
        l.both();
        drop(l.sim);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "coordinated::coordinated_crash_child",
                "--nocapture",
            ])
            .env("HAWALA_TEST_COUPLED_CRASH_DB", &file)
            .env("HAWALA_TEST_COUPLED_ABORT", if abort { "1" } else { "0" })
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(82));
        l.sim = Simulator::open(&file).unwrap();
        if abort {
            assert_eq!(projection(&l, &file), [2, 1, 1, 1000, 0, 0, 0, 1]);
            assert_eq!(l.admit().unwrap(), Admission::Held);
        } else {
            assert_eq!(projection(&l, &file), [1, 1, 1, 100, 900, 1, 1, 1]);
            use hawala_commit_sim::TestDispatchDecision;
            assert!(matches!(
                l.sim
                    .claim_test_dispatch([10; 32], [22; 32], id(4), 3, 150)
                    .unwrap(),
                TestDispatchDecision::Claimed(_)
            ));
            drop(l.sim);
            l.sim = Simulator::open(&file).unwrap();
            assert_eq!(l.admit().unwrap(), Admission::Held);
            assert_eq!(
                l.sim
                    .claim_test_dispatch([10; 32], [22; 32], id(4), 3, 150)
                    .unwrap(),
                TestDispatchDecision::Held
            );
            assert_eq!(
                l.sim
                    .abort_test_coordination([10; 32], l.digest, 4)
                    .unwrap()
                    .decision,
                TestDecision::Committed
            );
            assert_eq!(projection(&l, &file), [1, 1, 1, 100, 900, 1, 1, 1]);
        }
    }
}
