//! An *in-memory reference* double-entry invariant checker, not a bank,
//! claim issuer, customer money ledger, custody adapter or execution authority.
//! Posting a balanced journal is not proof that money/assets exist.
//!
//! No floats. Each individual journal must balance per currency; no FX value
//! is created by balancing one currency against a different currency.
//! State mutation is all-or-nothing in memory only (NOT crash durable).
use std::collections::{BTreeMap, BTreeSet};
use blake3::Hasher;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    UnknownAccount,
    DuplicateAccount,
    CurrencyMismatch,
    Unbalanced,
    Insufficient,
    Overflow,
    Conflict,
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalSide { Debit, Credit }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Account {
    pub id: [u8;32],
    pub currency: [u8;3],
    /// Assets/expenses generally debit-normal; liabilities/equity credit-normal.
    pub normal: NormalSide,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Posting {
    pub account: [u8;32],
    /// At most one of debit or credit may be nonzero for one posting.
    pub debit_minor: u64,
    pub credit_minor: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub event_id: [u8;32],
    pub postings: Vec<Posting>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    accounts: BTreeMap<[u8;32], Account>,
    natural_balances: BTreeMap<[u8;32], u64>,
    recorded: BTreeMap<[u8;32], [u8;32]>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Book {
    state: State,
}
impl Book {
    /// Every account begins at zero. The book has no API that accepts an
    /// arbitrary 'backed' opening balance; any changes require balanced legs.
    pub fn new(accounts: impl IntoIterator<Item=Account>) -> Result<Self> {
        let mut registered = BTreeMap::new();
        let mut balances = BTreeMap::new();
        for a in accounts {
            if a.id == [0;32] || !a.currency.iter().all(u8::is_ascii_uppercase) {
                return Err(Error::Invalid);
            }
            if registered.insert(a.id, a).is_some() {
                return Err(Error::DuplicateAccount);
            }
            balances.insert(a.id, 0);
        }
        if registered.is_empty() { return Err(Error::Invalid); }
        Ok(Self {state:State{
            accounts:registered,natural_balances:balances,recorded:BTreeMap::new()
        }})
    }
    pub fn balance(&self, id: [u8;32]) -> Option<u64> {
        self.state.natural_balances.get(&id).copied()
    }
    pub fn event_count(&self) -> usize { self.state.recorded.len() }
    pub fn account(&self, id: [u8;32]) -> Option<Account> {
        self.state.accounts.get(&id).copied()
    }

    /// Atomic ONLY in-memory: a staged BTreeMap is committed after checking
    /// every leg and per-currency zero sum. No bank or custody operation.
    pub fn post(&mut self, entry: Entry) -> Result<bool> {
        if entry.event_id == [0;32] || entry.postings.len() < 2
            || entry.postings.len() > 256 { return Err(Error::Invalid); }
        let mut ordered = entry.postings.clone();
        ordered.sort_by_key(|x| x.account);
        let mut seen = BTreeSet::new();
        let mut currency_totals: BTreeMap<[u8;3], (u128,u128)> = BTreeMap::new();
        // Validate all legs and canonical digest BEFORE checking prior event.
        // Exact replay must succeed even after the account was later emptied.
        for p in &ordered {
            if !seen.insert(p.account) { return Err(Error::DuplicateAccount); }
            if (p.debit_minor == 0) == (p.credit_minor == 0) {
                return Err(Error::Invalid);
            }
            let a = self.state.accounts.get(&p.account).ok_or(Error::UnknownAccount)?;
            let sums = currency_totals.entry(a.currency).or_insert((0,0));
            sums.0 = sums.0.checked_add(u128::from(p.debit_minor)).ok_or(Error::Overflow)?;
            sums.1 = sums.1.checked_add(u128::from(p.credit_minor)).ok_or(Error::Overflow)?;
        }
        if currency_totals.values().any(|(debit,credit)| debit != credit) {
            return Err(Error::Unbalanced);
        }
        // Order-invariant encoding of a journal including the account registry
        // identity, currency and normal side; no JSON/float ambiguity.
        let mut h=Hasher::new_derive_key("hawala/reference-ledger-entry/v1");
        h.update(&entry.event_id);
        for p in &ordered {
            let a = self.state.accounts.get(&p.account).ok_or(Error::UnknownAccount)?;
            h.update(&a.id);
            h.update(&a.currency);
            h.update(&[match a.normal {NormalSide::Debit=>1,NormalSide::Credit=>2}]);
            h.update(&p.debit_minor.to_be_bytes());
            h.update(&p.credit_minor.to_be_bytes());
        }
        let digest=*h.finalize().as_bytes();
        if let Some(old) = self.state.recorded.get(&entry.event_id) {
            return if *old == digest {Ok(false)} else {Err(Error::Conflict)};
        }
        let mut staged = self.state.natural_balances.clone();
        for p in &ordered {
            let a = self.state.accounts.get(&p.account).ok_or(Error::UnknownAccount)?;
            let balance = *staged.get(&a.id).ok_or(Error::UnknownAccount)?;
            let (increase, decrease) = match a.normal {
                NormalSide::Debit => (p.debit_minor, p.credit_minor),
                NormalSide::Credit => (p.credit_minor, p.debit_minor),
            };
            let next = u128::from(balance).checked_add(u128::from(increase))
                .ok_or(Error::Overflow)?
                .checked_sub(u128::from(decrease)).ok_or(Error::Insufficient)?;
            staged.insert(a.id, u64::try_from(next).map_err(|_| Error::Overflow)?);
        }
        self.state.natural_balances = staged;
        self.state.recorded.insert(entry.event_id, digest);
        Ok(true)
    }
}
