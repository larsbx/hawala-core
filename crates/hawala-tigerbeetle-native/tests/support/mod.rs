#![allow(dead_code)]
use ed25519_dalek::SigningKey;
use hawala_tigerbeetle_flow::{Confirmed, Plan, RecordedTransfer};
use hawala_tigerbeetle_native::{
    accounting::{compile, Account, Ledger, Posting, Registry},
    durable::{EvidenceStore, SignedSink},
    evidence::Admission,
};
use std::path::Path;
pub const CLUSTER: u128 = 71028;
pub fn key() -> SigningKey {
    SigningKey::from_bytes(&[71; 32])
}
pub fn registry() -> Registry {
    Registry {
        context: [[1; 32], [2; 32], [3; 32], [4; 32], [5; 32]],
        revision: 1,
        previous: [0; 32],
        epoch: 9,
        ledgers: vec![Ledger {
            id: 1,
            currency: *b"USD",
            scale: 2,
        }],
        accounts: (1..=4)
            .map(|i| Account {
                identity: [i as u8; 32],
                native_id: i,
                ledger: 1,
                normal_credit: i > 2,
            })
            .collect(),
    }
}
pub fn plan(operation: u128) -> Plan {
    compile(
        &registry(),
        CLUSTER,
        1,
        operation,
        &[
            Posting {
                account: [1; 32],
                credit: false,
                amount: 7,
            },
            Posting {
                account: [2; 32],
                credit: false,
                amount: 3,
            },
            Posting {
                account: [3; 32],
                credit: true,
                amount: 4,
            },
            Posting {
                account: [4; 32],
                credit: true,
                amount: 6,
            },
        ],
    )
    .unwrap()
}
pub fn evidence(path: &Path) -> EvidenceStore {
    EvidenceStore::open(path, registry(), CLUSTER, 1, key().verifying_key()).unwrap()
}
pub fn retain(store: &mut EvidenceStore, p: &Plan) {
    let a = Admission::sign(
        &registry(),
        p,
        b"retained original TEST business request; execution and settlement absent",
        &key(),
    )
    .unwrap();
    store.retain(&a).unwrap();
}
pub fn group(p: &Plan, first: u64) -> Confirmed {
    Confirmed::from_records(
        p.clone(),
        p.transfers()
            .iter()
            .enumerate()
            .map(|(i, t)| RecordedTransfer {
                transfer: t.clone(),
                timestamp: first + i as u64,
            })
            .collect(),
    )
    .unwrap()
}
pub fn sink(path: &Path, ev: &Path, id: u8) -> SignedSink {
    SignedSink::open(
        path,
        evidence(ev),
        [id; 32],
        SigningKey::from_bytes(&[id; 32]),
    )
    .unwrap()
}
