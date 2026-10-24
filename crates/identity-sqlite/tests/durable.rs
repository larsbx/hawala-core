use std::sync::Arc;
use std::thread;

use ed25519_dalek::SigningKey;
use hawala_identity_core::{IdentityId, ReplayGuard, Error};
use hawala_identity_sqlite::{OpenError, SqliteReplayGuard};

fn id(n: u8) -> IdentityId {
    IdentityId::from_signing_key(&SigningKey::from_bytes(&[n; 32]))
}

#[test]
fn replay_never_becomes_valid_after_reopen() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("nonces.sqlite");
    {
        let guard = SqliteReplayGuard::open(&path).unwrap();
        assert_eq!(guard.consume_once(id(1), id(2), [3; 32]), Ok(true));
        assert_eq!(guard.consume_once(id(1), id(2), [3; 32]), Ok(false));
        assert_eq!(guard.count(), Ok(1));
    }
    let guard = SqliteReplayGuard::open(&path).unwrap();
    assert_eq!(guard.consume_once(id(1), id(2), [3; 32]), Ok(false));
    assert_eq!(guard.consume_once(id(1), id(2), [4; 32]), Ok(true));
    assert_eq!(guard.consume_once(id(1), id(3), [3; 32]), Ok(true));
    assert_eq!(guard.count(), Ok(3));
}

#[test]
fn concurrent_races_consume_exactly_once() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("racing.sqlite");
    let guard = Arc::new(SqliteReplayGuard::open(&path).unwrap());
    let workers: Vec<_> = (0..16).map(|_| {
        let guard = Arc::clone(&guard);
        thread::spawn(move || guard.consume_once(id(1), id(2), [42; 32]).unwrap())
    }).collect();
    assert_eq!(workers.into_iter().filter(|w| w.join().unwrap()).count(), 1);
    assert_eq!(guard.count(), Ok(1));
}

#[test]
fn two_independent_sqlite_handles_consume_only_once() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("two-writers.sqlite");
    let first = SqliteReplayGuard::open(&path).unwrap();
    let second = SqliteReplayGuard::open(&path).unwrap();
    assert_eq!(first.consume_once(id(1), id(2), [42; 32]), Ok(true));
    assert_eq!(second.consume_once(id(1), id(2), [42; 32]), Ok(false));
}

#[test]
fn memory_and_invalid_nonce_are_rejected() {
    assert!(matches!(SqliteReplayGuard::open(":memory:"), Err(OpenError::InMemoryNotDurable)));
    let d = tempfile::tempdir().unwrap();
    let guard = SqliteReplayGuard::open(d.path().join("nonce.sqlite")).unwrap();
    assert_eq!(guard.consume_once(id(1), id(2), [0; 32]), Err(Error::InvalidInput));
}
