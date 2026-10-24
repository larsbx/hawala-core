//! Pure reference checks. No custody, authorization, persistence or consensus.
//! Production callers must atomically persist these decisions with their ledger.
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Overflow,
    Insufficient,
    Conflict,
    Closed,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuoteAmounts {
    pub source_debit: u64,
    pub destination_gross: u64,
    pub destination_credit: u64,
    /// Rounding remainder in units of 1 / rate_den destination minor units.
    pub remainder: u64,
}

/// Rate is in DESTINATION MINOR units per SOURCE MINOR unit. Not a major-unit
/// display rate. Fees are separate source/destination minor-unit amounts.
pub fn quote_amounts(
    principal: u64,
    source_fee: u64,
    destination_fee: u64,
    rate_num: u64,
    rate_den: u64,
) -> Result<QuoteAmounts, Error> {
    if principal == 0 || rate_num == 0 || rate_den == 0 {
        return Err(Error::Invalid);
    }
    let source_debit = principal.checked_add(source_fee).ok_or(Error::Overflow)?;
    let product = u128::from(principal) * u128::from(rate_num);
    let gross = product / u128::from(rate_den);
    let destination_gross = u64::try_from(gross).map_err(|_| Error::Overflow)?;
    let destination_credit = destination_gross
        .checked_sub(destination_fee)
        .filter(|v| *v > 0)
        .ok_or(Error::Invalid)?;
    Ok(QuoteAmounts {
        source_debit,
        destination_gross,
        destination_credit,
        remainder: (product % u128::from(rate_den)) as u64,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Reservation {
    amount: u64,
    active: bool,
}

/// One dar, one currency, one coverage bucket. The constructor's amount MUST
/// come from independently verified eligible custody, not a dar's own claim.
/// Methods are intentionally LOCAL and in-memory. `&mut self` is not a
/// distributed lock. This object must never itself authorize cash or custody.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollateralBook {
    eligible: u64,
    reserved: u64,
    reservations: BTreeMap<[u8; 32], Reservation>,
}

impl CollateralBook {
    pub fn new(verified_eligible: u64) -> Self {
        Self { eligible: verified_eligible, reserved: 0, reservations: BTreeMap::new() }
    }
    pub fn eligible(&self) -> u64 { self.eligible }
    pub fn reserved(&self) -> u64 { self.reserved }
    pub fn free(&self) -> u64 { self.eligible - self.reserved }

    /// true = newly reserved, false = identical active retry.
    /// Completed IDs are tombstoned and cannot be reopened by replay.
    pub fn reserve(&mut self, id: [u8; 32], amount: u64) -> Result<bool, Error> {
        if id == [0; 32] || amount == 0 { return Err(Error::Invalid); }
        if let Some(old) = self.reservations.get(&id) {
            if old.amount != amount { return Err(Error::Conflict); }
            return if old.active { Ok(false) } else { Err(Error::Closed) };
        }
        if amount > self.free() { return Err(Error::Insufficient); }
        // Bound above by eligible <= u64::MAX; addition cannot overflow.
        self.reserved += amount;
        self.reservations.insert(id, Reservation { amount, active: true });
        Ok(true)
    }

    /// Invariant transition ONLY. Caller must establish that the covered
    /// obligation is discharged/replaced under the actual agreement BEFORE
    /// invoking this method. This does not verify evidence or grant permission.
    pub fn release_after_verified_discharge(&mut self, id: [u8; 32]) -> Result<bool, Error> {
        let old = self.reservations.get_mut(&id).ok_or(Error::Unknown)?;
        if !old.active { return Ok(false); }
        self.reserved -= old.amount;
        old.active = false;
        Ok(true)
    }

    /// Preview a withdrawal, without mutating custody or this book. Production
    /// withdrawal needs a unique execution ID and atomic persistence in an
    /// external custody/ledger transaction. A returned number is NOT permission.
    pub fn eligible_after_withdrawal(&self, amount: u64) -> Result<u64, Error> {
        if amount == 0 { return Err(Error::Invalid); }
        if amount > self.free() { return Err(Error::Insufficient); }
        Ok(self.eligible - amount)
    }
}
