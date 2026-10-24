use hawala_safety::{quote_amounts, CollateralBook, Error};

#[test]
fn shared_quote_vectors() {
    for line in include_str!("../../../spec/quote-vectors.tsv").lines() {
        if line.starts_with('#') || line.is_empty() { continue; }
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 7);
        let n: Vec<u64> = fields[1..6].iter().map(|v| v.parse().unwrap()).collect();
        let result = quote_amounts(n[0], n[1], n[2], n[3], n[4]);
        let actual = match result {
            Ok(q) => format!("{},{},{},{}", q.source_debit, q.destination_gross,
                q.destination_credit, q.remainder),
            Err(e) => format!("{e:?}"),
        };
        assert_eq!(actual, fields[6], "{}", fields[0]);
    }
}

#[test]
fn exact_capacity_replay_conflict_and_tombstones() {
    let mut b = CollateralBook::new(100);
    assert_eq!(b.reserve([1;32], 60), Ok(true));
    assert_eq!(b.reserve([1;32], 60), Ok(false));
    let before = b.clone();
    assert_eq!(b.reserve([1;32], 61), Err(Error::Conflict));
    assert_eq!(b.reserve([2;32], 41), Err(Error::Insufficient));
    assert_eq!(b.eligible_after_withdrawal(41), Err(Error::Insufficient));
    assert_eq!(b, before);
    assert_eq!(b.reserve([2;32], 40), Ok(true));
    assert_eq!(b.free(), 0);
    assert_eq!(b.release_after_verified_discharge([1;32]), Ok(true));
    assert_eq!(b.release_after_verified_discharge([1;32]), Ok(false));
    assert_eq!(b.reserve([1;32], 60), Err(Error::Closed));
    assert_eq!(b.reserve([1;32], 59), Err(Error::Conflict));
    assert_eq!(b.free(), 60);
    assert_eq!(b.eligible_after_withdrawal(60), Ok(40));
    assert_eq!(b.free(), 60); // preview never mutates custody/book
}

#[test]
fn both_serializations_of_competing_reservations_preserve_capacity() {
    for order in [[1u8,2], [2u8,1]] {
        let mut b = CollateralBook::new(100);
        assert_eq!(b.reserve([order[0];32], 60), Ok(true));
        assert_eq!(b.reserve([order[1];32], 60), Err(Error::Insufficient));
        assert_eq!(b.reserved(), 60);
        assert_eq!(b.free(), 40);
    }
}

#[test]
fn malformed_and_unknown_operations_leave_state_unchanged() {
    let mut b = CollateralBook::new(u64::MAX);
    let before = b.clone();
    assert_eq!(b.reserve([0;32], 1), Err(Error::Invalid));
    assert_eq!(b.reserve([1;32], 0), Err(Error::Invalid));
    assert_eq!(b.release_after_verified_discharge([1;32]), Err(Error::Unknown));
    assert_eq!(b.eligible_after_withdrawal(0), Err(Error::Invalid));
    assert_eq!(b, before);
    assert_eq!(b.reserve([1;32], u64::MAX), Ok(true));
    assert_eq!(b.reserve([2;32], 1), Err(Error::Insufficient));
    assert_eq!(b.free(), 0);
}
