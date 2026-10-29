#![cfg(feature = "native-conformance")]
mod support;
use futures::executor::block_on;
use hawala_tigerbeetle_flow::{
    persist_once, record, recover,
    replay::{HistorySource, NativeHistory, NativeRecord},
    CommitSource, Decision, DurableSink, Error, FinancialPort,
};
use hawala_tigerbeetle_native::{evidence::Admission, native::NativeTestAdapter};
use std::{
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use support::*;
use tempfile::{tempdir, TempDir};
use tigerbeetle as tb;

struct Server {
    child: Child,
    binary: PathBuf,
    file: PathBuf,
    address: SocketAddr,
    _dir: TempDir,
}
impl Server {
    fn start() -> Self {
        let binary = PathBuf::from(
            std::env::var_os("TB_BINARY").expect("TB_BINARY must name the pinned 0.17.9 binary"),
        );
        let version = Command::new(&binary).arg("version").output().unwrap();
        assert!(String::from_utf8_lossy(&version.stdout).contains("0.17.9"));
        let dir = tempdir().unwrap();
        let file = dir.path().join("cluster.tigerbeetle");
        let output = Command::new(&binary)
            .args([
                "format",
                "--cluster=71028",
                "--replica=0",
                "--replica-count=1",
                "--development",
            ])
            .arg(&file)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "native format failed (no skip): {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let child = Self::spawn(&binary, &file, address);
        let mut s = Self {
            child,
            binary,
            file,
            address,
            _dir: dir,
        };
        s.ready();
        s
    }
    fn spawn(binary: &Path, file: &Path, address: SocketAddr) -> Child {
        Command::new(binary)
            .args(["start", &format!("--addresses={address}"), "--development"])
            .arg(file)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap()
    }
    fn ready(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while TcpStream::connect_timeout(&self.address, Duration::from_millis(50)).is_err() {
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "native TB process exited"
            );
            assert!(Instant::now() < deadline, "TB process startup timeout");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    fn restart(&mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
        self.child = Self::spawn(&self.binary, &self.file, self.address);
        self.ready();
    }
    fn client(&self) -> tb::Client {
        tb::Client::new(CLUSTER, &self.address.to_string()).unwrap()
    }
    fn accounts(&self, count: u128) {
        let a: Vec<_> = (1..=count)
            .map(|id| tb::Account {
                id,
                ledger: 1,
                code: 1,
                ..Default::default()
            })
            .collect();
        let r = block_on(self.client().create_accounts(&a)).unwrap();
        assert_eq!(r.len(), a.len());
        assert!(r
            .iter()
            .all(|r| r.status == tb::CreateAccountStatus::Created));
    }
    fn adapter(&self, ev: &Path) -> NativeTestAdapter {
        NativeTestAdapter::connect_loopback_test(self.address, evidence(ev)).unwrap()
    }
    fn rows(&self) -> Vec<tb::Transfer> {
        block_on(self.client().query_transfers(tb::QueryFilter {
            limit: 4096,
            ..Default::default()
        }))
        .unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn requests(p: &hawala_tigerbeetle_flow::Plan) -> Vec<tb::Transfer> {
    p.linked_transfers()
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
        .collect()
}

#[test]
#[ignore = "requires actual pinned TigerBeetle process; mandatory native CI job"]
fn native_commit_recovery_destinations_and_server_restart() {
    let mut server = Server::start();
    server.accounts(4);
    let d = tempdir().unwrap();
    let ev = d.path().join("evidence.sqlite");
    let p = plan(100);
    let p2 = plan(200);
    let mut e = evidence(&ev);
    retain(&mut e, &p);
    retain(&mut e, &p2);
    drop(e);
    // Process dies after the native request is queued, before observing its outcome.
    // Either it committed or it did not: recovery must use the identical IDs.
    for (mode, operation) in [("interrupt", 100u128), ("lost-ack", 200u128)] {
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "native_worker", "--nocapture"])
            .env("HAWALA_NATIVE_WORKER", mode)
            .env("HAWALA_EVIDENCE", &ev)
            .env("HAWALA_NATIVE_ADDRESS", server.address.to_string())
            .env("HAWALA_OPERATION", operation.to_string())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(17));
        let mut port = server.adapter(&ev);
        assert!(matches!(
            recover(&mut port, &plan(operation)).unwrap(),
            Decision::Committed(_)
        ));
        assert!(matches!(
            record(&mut port, &plan(operation)).unwrap(),
            Decision::Committed(_)
        ));
    }
    assert_eq!(server.rows().len(), 6);
    // Immutable evidence also rejects changed native bodies with original IDs.
    let mut changed = p.transfers().to_vec();
    changed[0].amount += 1;
    let changed = hawala_tigerbeetle_flow::Plan::new(CLUSTER, 1, 100, 9, changed).unwrap();
    assert_eq!(
        server.adapter(&ev).submit_linked(&changed),
        Err(Error::Conflict)
    );
    let a = Admission::sign(&registry(), &changed, b"changed retry", &key()).unwrap();
    assert_eq!(evidence(&ev).retain(&a), Err(Error::Conflict));
    let a = d.path().join("a.sqlite");
    let b = d.path().join("b.sqlite");
    let mut sa = sink(&a, &ev, 10);
    let mut sb = sink(&b, &ev, 11);
    let mut source = HistorySource::new(server.adapter(&ev), CLUSTER, 1, 1).unwrap();
    let lock = rusqlite::Connection::open(&b).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert_eq!(
        persist_once(&mut source, &mut sa, CLUSTER, 1, 1).unwrap(),
        1
    );
    assert!(persist_once(&mut source, &mut sb, CLUSTER, 1, 2).is_err());
    assert_eq!(sb.watermark(CLUSTER, 1).unwrap(), 0);
    // The committed TB effect remains while this destination cannot accept it.
    assert_eq!(server.rows().len(), 6);
    drop(source);
    drop(sa);
    drop(sb);
    lock.execute_batch("ROLLBACK").unwrap();
    drop(lock);
    server.restart();
    assert_eq!(server.rows().len(), 6);
    let mut source = HistorySource::new(server.adapter(&ev), CLUSTER, 1, 2).unwrap();
    let mut sa = sink(&a, &ev, 10);
    let mut sb = sink(&b, &ev, 11);
    assert_eq!(
        persist_once(&mut source, &mut sa, CLUSTER, 1, 2).unwrap(),
        1
    );
    assert_eq!(
        persist_once(&mut source, &mut sb, CLUSTER, 1, 2).unwrap(),
        2
    );
    assert_eq!(
        persist_once(&mut source, &mut sb, CLUSTER, 1, 2).unwrap(),
        0
    );
    assert_eq!(sa.audit().unwrap(), 2);
    assert_eq!(sb.audit().unwrap(), 2);
    let balances = block_on(server.client().lookup_accounts(&[1, 2, 3, 4])).unwrap();
    assert_eq!(balances[0].debits_posted, 14);
    assert_eq!(balances[1].debits_posted, 6);
    assert_eq!(balances[2].credits_posted, 8);
    assert_eq!(balances[3].credits_posted, 12);
    assert_eq!(server.rows().len(), 6);
}
#[test]
#[ignore = "helper only; invoked by native matrix with explicit environment"]
fn native_worker() {
    let Ok(mode) = std::env::var("HAWALA_NATIVE_WORKER") else {
        return;
    };
    let ev = PathBuf::from(std::env::var_os("HAWALA_EVIDENCE").unwrap());
    let address: SocketAddr = std::env::var("HAWALA_NATIVE_ADDRESS")
        .unwrap()
        .parse()
        .unwrap();
    let operation: u128 = std::env::var("HAWALA_OPERATION").unwrap().parse().unwrap();
    if mode == "interrupt" {
        let client = tb::Client::new(CLUSTER, &address.to_string()).unwrap();
        let request = requests(&plan(operation));
        let _future = client.create_transfers(&request);
        std::process::exit(17);
    }
    let mut port = NativeTestAdapter::connect_loopback_test(address, evidence(&ev)).unwrap();
    assert!(matches!(
        record(&mut port, &plan(operation)).unwrap(),
        Decision::Committed(_)
    ));
    std::process::exit(17); // No financial acknowledgment or sink delivery to parent.
}
#[test]
#[ignore = "requires actual pinned TigerBeetle process; mandatory native CI job"]
fn native_linked_rejection_has_no_prefix_and_no_false_rejection() {
    let server = Server::start();
    server.accounts(3);
    let d = tempdir().unwrap();
    let ev = d.path().join("e.sqlite");
    let p = plan(100);
    retain(&mut evidence(&ev), &p);
    let result = block_on(server.client().create_transfers(&requests(&p))).unwrap();
    assert_eq!(result.len(), 3);
    assert!(result
        .iter()
        .all(|s| s.status != tb::CreateTransferStatus::Created));
    assert!(server.rows().is_empty());
    let mut adapter = server.adapter(&ev);
    assert!(matches!(recover(&mut adapter, &p).unwrap(), Decision::Held));
    assert!(server.rows().is_empty());
}
#[test]
#[ignore = "requires actual pinned TigerBeetle process; mandatory native CI job"]
fn native_partial_history_and_missing_evidence_cannot_advance_destination() {
    struct Truncated {
        inner: NativeTestAdapter,
        read: bool,
    }
    impl NativeHistory for Truncated {
        fn context(&self) -> (u128, u64) {
            self.inner.context()
        }
        fn read_after(
            &mut self,
            t: u64,
            n: usize,
        ) -> hawala_tigerbeetle_flow::Result<Vec<NativeRecord>> {
            if self.read {
                return Ok(vec![]);
            }
            self.read = true;
            self.inner.read_after(t, n)
        }
    }
    let server = Server::start();
    server.accounts(4);
    let d = tempdir().unwrap();
    let ev = d.path().join("e.sqlite");
    let p = plan(100);
    retain(&mut evidence(&ev), &p);
    assert!(matches!(
        record(&mut server.adapter(&ev), &p).unwrap(),
        Decision::Committed(_)
    ));
    let mut partial = HistorySource::new(
        Truncated {
            inner: server.adapter(&ev),
            read: false,
        },
        CLUSTER,
        1,
        1,
    )
    .unwrap();
    let mut dest = sink(&d.path().join("s.sqlite"), &ev, 10);
    assert!(persist_once(&mut partial, &mut dest, CLUSTER, 1, 1).is_err());
    assert_eq!(dest.watermark(CLUSTER, 1).unwrap(), 0);
    let mut source = HistorySource::new(server.adapter(&ev), CLUSTER, 1, 1).unwrap();
    let complete = source.read_after(0, 1).unwrap();
    dest.commit(0, &complete[0]).unwrap();
    dest.commit(0, &complete[0]).unwrap();
    assert_eq!(dest.audit().unwrap(), 1);
    let empty = d.path().join("empty.sqlite");
    let mut untrusted = HistorySource::new(server.adapter(&empty), CLUSTER, 1, 1).unwrap();
    assert!(untrusted.read_after(0, 1).is_err());
    assert_eq!(server.rows().len(), 3);
}
