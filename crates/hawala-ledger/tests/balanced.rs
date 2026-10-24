use hawala_ledger::{Account,Book,Entry,Error,NormalSide,Posting};
fn account(id:u8,currency:[u8;3],normal:NormalSide)->Account{
    Account{id:[id;32],currency,normal}
}
fn posting(id:u8,debit:u64,credit:u64)->Posting{
    Posting{account:[id;32],debit_minor:debit,credit_minor:credit}
}
fn entry(e:u8,postings:Vec<Posting>)->Entry{
    Entry{event_id:[e;32],postings}
}
fn book()->Book {
    Book::new([
        account(1,*b"EUR",NormalSide::Debit),
        account(2,*b"EUR",NormalSide::Credit),
        account(3,*b"EUR",NormalSide::Debit),
        account(4,*b"EUR",NormalSide::Credit),
        account(5,*b"USD",NormalSide::Debit),
        account(6,*b"USD",NormalSide::Credit),
    ]).unwrap()
}
#[test]
fn balanced_customer_asset_and_liability_issuance_and_redemption(){
    let mut b=book();
    let issue=entry(10,vec![posting(1,100,0),posting(2,0,100)]);
    assert_eq!(b.post(issue.clone()),Ok(true));
    assert_eq!(b.balance([1;32]),Some(100));
    assert_eq!(b.balance([2;32]),Some(100));
    let redeem=entry(11,vec![posting(2,40,0),posting(1,0,40)]);
    assert_eq!(b.post(redeem.clone()),Ok(true));
    assert_eq!(b.balance([1;32]),Some(60));
    assert_eq!(b.balance([2;32]),Some(60));
    assert_eq!(b.post(redeem),Ok(false));
    assert_eq!(b.post(issue),Ok(false));
    assert_eq!(b.event_count(),2);
}
#[test]
fn prevents_any_overdraft_and_never_partially_mutates(){
    let mut b=book();
    let snap=b.clone();
    assert_eq!(b.post(entry(1,vec![posting(2,1,0),posting(1,0,1)])),
               Err(Error::Insufficient));
    assert_eq!(b,snap);
    assert_eq!(b.post(entry(2,vec![posting(1,20,0),posting(2,0,19)])),
               Err(Error::Unbalanced));
    assert_eq!(b,snap);
    assert_eq!(b.post(entry(3,vec![posting(1,10,0),posting(1,0,10)])),
               Err(Error::DuplicateAccount));
    assert_eq!(b,snap);
    assert_eq!(b.post(entry(4,vec![posting(1,10,10),posting(2,0,10)])),
               Err(Error::Invalid));
    assert_eq!(b,snap);
}
#[test]
fn currencies_cannot_offset_each_other(){
    let mut b=book();
    assert_eq!(
        b.post(entry(1,vec![posting(1,90,0),posting(6,0,90)])),
        Err(Error::Unbalanced)
    );
    // Each currency has independent balanced legs: cross-currency
    // exchange is not "EUR debit = USD credit"; FX clearing accounts,
    // actual positions and quote controls still need separate enforcement.
    assert_eq!(b.post(entry(2,vec![
        posting(1,90,0),posting(2,0,90),
        posting(5,100,0),posting(6,0,100),
    ])),Ok(true));
    assert_eq!(b.balance([1;32]),Some(90));
    assert_eq!(b.balance([2;32]),Some(90));
    assert_eq!(b.balance([5;32]),Some(100));
    assert_eq!(b.balance([6;32]),Some(100));
}
#[test]
fn repeated_event_id_with_changed_amount_is_conflict(){
    let mut b=book();
    b.post(entry(1,vec![posting(1,100,0),posting(2,0,100)])).unwrap();
    let spent=entry(2,vec![posting(2,100,0),posting(1,0,100)]);
    b.post(spent.clone()).unwrap();
    assert_eq!(b.balance([1;32]),Some(0));
    assert_eq!(b.post(spent),Ok(false));
    let different=entry(2,vec![posting(2,99,0),posting(1,0,99)]);
    assert_eq!(b.post(different),Err(Error::Conflict));
    assert_eq!(b.event_count(),2);
    assert_eq!(b.balance([1;32]),Some(0));
}
#[test]
fn account_and_integer_overflow_guards(){
    let mut b=book();
    b.post(entry(1,vec![posting(1,u64::MAX,0),posting(2,0,u64::MAX)])).unwrap();
    let snap=b.clone();
    assert_eq!(b.post(entry(2,vec![posting(1,1,0),posting(2,0,1)])),
               Err(Error::Overflow));
    assert_eq!(b,snap);
    assert_eq!(Book::new([account(1,*b"EUR",NormalSide::Debit),
                           account(1,*b"EUR",NormalSide::Debit)]),
               Err(Error::DuplicateAccount));
    assert_eq!(Book::new([account(7,*b"eUR",NormalSide::Credit)]),
               Err(Error::Invalid));
    assert_eq!(Book::new([account(0,*b"USD",NormalSide::Debit)]),
               Err(Error::Invalid));
}
