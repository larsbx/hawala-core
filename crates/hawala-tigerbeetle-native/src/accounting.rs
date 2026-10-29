//! TB-first translation of #50's sorted-ledger/sorted-account/min-pair compiler.
//! Full account identities map injectively; allocation of ID ranges is external.
use hawala_tigerbeetle_flow::{leg_tag, Error, Plan, Result, Transfer, MAX_AMOUNT, MAX_LEGS};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub identity: [u8; 32],
    pub native_id: u128,
    pub ledger: u32,
    pub normal_credit: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ledger {
    pub id: u32,
    pub currency: [u8; 3],
    pub scale: u8,
}

/// Context contains network, installation, authority, scope and legal-book IDs.
/// Exactly one immutable registry/context per test adapter, never global books.
#[derive(Clone, Debug)]
pub struct Registry {
    pub context: [[u8; 32]; 5],
    pub revision: u64,
    pub previous: [u8; 32],
    pub epoch: u64,
    pub ledgers: Vec<Ledger>,
    pub accounts: Vec<Account>,
}

impl Registry {
    /// Byte-compatible registry encoding from #50, with all widths big endian.
    pub fn bytes(&self) -> Result<Vec<u8>> {
        if self.context.contains(&[0; 32])
            || self.epoch == 0
            || self.revision == 0
            || (self.revision == 1) != (self.previous == [0; 32])
            || self.ledgers.is_empty()
            || self.ledgers.len() > 65535
            || self.accounts.len() < 2
            || self.accounts.len() > 65535
        {
            return Err(Error::InvalidPlan);
        }
        let mut out = b"hawala-node-books-registry/v1\0".to_vec();
        for id in self.context {
            out.extend(id);
        }
        out.extend(self.epoch.to_be_bytes());
        out.extend(self.revision.to_be_bytes());
        out.extend(self.previous);
        out.extend((self.ledgers.len() as u16).to_be_bytes());
        let mut last = 0;
        let mut assets = BTreeSet::new();
        for l in &self.ledgers {
            if l.id <= last
                || l.scale > 18
                || !l.currency.iter().all(u8::is_ascii_uppercase)
                || !assets.insert((l.currency, l.scale))
            {
                return Err(Error::InvalidPlan);
            }
            last = l.id;
            out.extend(l.id.to_be_bytes());
            out.extend(l.currency);
            out.push(l.scale);
        }
        out.extend((self.accounts.len() as u16).to_be_bytes());
        let mut last = [0; 32];
        let mut ids = BTreeSet::new();
        for a in &self.accounts {
            if a.identity <= last
                || a.native_id == 0
                || a.native_id == u128::MAX
                || !ids.insert(a.native_id)
                || !self.ledgers.iter().any(|l| l.id == a.ledger)
            {
                return Err(Error::InvalidPlan);
            }
            last = a.identity;
            out.extend(a.identity);
            out.extend(a.native_id.to_be_bytes());
            out.extend(a.ledger.to_be_bytes());
            out.push(if a.normal_credit { 2 } else { 1 });
        }
        Ok(out)
    }
    pub fn digest(&self) -> Result<[u8; 32]> {
        Ok(Sha256::digest(self.bytes()?).into())
    }
    /// Recompile totals recovered from a plan. Signed native admissions must
    /// use the same deterministic pairing, metadata and reserved ID range.
    pub fn validate_plan(&self, plan: &Plan) -> Result<()> {
        let mut totals: BTreeMap<[u8; 32], (bool, u128)> = BTreeMap::new();
        for t in plan.transfers() {
            for (id, credit) in [(t.debit_account_id, false), (t.credit_account_id, true)] {
                let a = self
                    .accounts
                    .iter()
                    .find(|a| a.native_id == id && a.ledger == t.ledger)
                    .ok_or(Error::Conflict)?;
                let row = totals.entry(a.identity).or_insert((credit, 0));
                if row.0 != credit {
                    return Err(Error::Conflict);
                }
                row.1 = row.1.checked_add(t.amount).ok_or(Error::Conflict)?;
            }
        }
        let postings: Vec<_> = totals
            .into_iter()
            .map(|(account, (credit, amount))| Posting {
                account,
                credit,
                amount,
            })
            .collect();
        if compile(
            self,
            plan.cluster(),
            plan.generation(),
            plan.operation(),
            &postings,
        )? != *plan
        {
            return Err(Error::Conflict);
        }
        Ok(())
    }
    pub fn successor(&self, next: &Self) -> Result<()> {
        self.bytes()?;
        next.bytes()?;
        if self.context != next.context
            || self.epoch != next.epoch
            || self.revision.checked_add(1) != Some(next.revision)
            || self.digest()? != next.previous
            || !self.ledgers.iter().all(|l| next.ledgers.contains(l))
            || !self.accounts.iter().all(|a| next.accounts.contains(a))
        {
            return Err(Error::Conflict);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Posting {
    pub account: [u8; 32],
    pub credit: bool,
    pub amount: u128,
}

pub fn compile(
    registry: &Registry,
    cluster: u128,
    generation: u64,
    operation: u128,
    postings: &[Posting],
) -> Result<Plan> {
    registry.bytes()?;
    if !(2..=256).contains(&postings.len()) {
        return Err(Error::InvalidPlan);
    }
    let mut seen = BTreeSet::new();
    type Sides<'a> = (Vec<(&'a Account, u128)>, Vec<(&'a Account, u128)>);
    let mut per_ledger: BTreeMap<u32, Sides<'_>> = BTreeMap::new();
    for p in postings {
        let a = registry
            .accounts
            .iter()
            .find(|a| a.identity == p.account)
            .ok_or(Error::InvalidPlan)?;
        if !seen.insert(p.account) || p.amount == 0 || p.amount > MAX_AMOUNT {
            return Err(Error::InvalidPlan);
        }
        let sides = per_ledger.entry(a.ledger).or_default();
        if p.credit {
            sides.1.push((a, p.amount));
        } else {
            sides.0.push((a, p.amount));
        }
    }
    let mut transfers = Vec::new();
    for (ledger, (mut debits, mut credits)) in per_ledger {
        debits.sort_by_key(|(a, _)| a.identity);
        credits.sort_by_key(|(a, _)| a.identity);
        let dt: u128 = debits.iter().map(|(_, v)| v).sum();
        let ct: u128 = credits.iter().map(|(_, v)| v).sum();
        if dt != ct || dt == 0 {
            return Err(Error::InvalidPlan);
        }
        let (mut di, mut ci) = (0, 0);
        while di < debits.len() && ci < credits.len() {
            let amount = debits[di].1.min(credits[ci].1);
            let id = operation
                .checked_add(transfers.len() as u128)
                .ok_or(Error::InvalidPlan)?;
            transfers.push(Transfer {
                id,
                debit_account_id: debits[di].0.native_id,
                credit_account_id: credits[ci].0.native_id,
                amount,
                ledger,
                code: 1,
                user_data_128: operation,
                user_data_64: registry.epoch,
                user_data_32: 0,
            });
            debits[di].1 -= amount;
            credits[ci].1 -= amount;
            if debits[di].1 == 0 {
                di += 1;
            }
            if credits[ci].1 == 0 {
                ci += 1;
            }
        }
    }
    if transfers.len() > MAX_LEGS {
        return Err(Error::InvalidPlan);
    }
    let count = transfers.len();
    for (i, t) in transfers.iter_mut().enumerate() {
        t.user_data_32 = leg_tag(count, i);
    }
    Plan::new(cluster, generation, operation, registry.epoch, transfers)
}
