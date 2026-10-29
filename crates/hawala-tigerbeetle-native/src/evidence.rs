//! Retain original signed admission bytes before any uncertain native request.
//! This is evidence custody, never a SQL financial commit or execution consent.
use crate::accounting::Registry;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hawala_tigerbeetle_flow::{Confirmed, Error, Plan, Result, Transfer, MAX_LEGS};

pub const MAX_EVIDENCE: usize = 1024 * 1024;
const PLAN: &[u8] = b"hawala-tb-first-plan/v1\0";
const ADMISSION: &[u8] = b"hawala-tb-first-test-admission/v1\0";

pub fn plan_bytes(p: &Plan) -> Vec<u8> {
    let mut b = PLAN.to_vec();
    b.extend(p.cluster().to_be_bytes());
    b.extend(p.generation().to_be_bytes());
    b.extend(p.operation().to_be_bytes());
    b.extend(p.epoch().to_be_bytes());
    b.extend((p.transfers().len() as u16).to_be_bytes());
    for t in p.transfers() {
        for v in [
            t.id,
            t.debit_account_id,
            t.credit_account_id,
            t.amount,
            t.user_data_128,
        ] {
            b.extend(v.to_be_bytes());
        }
        b.extend(t.ledger.to_be_bytes());
        b.extend(t.code.to_be_bytes());
        b.extend(t.user_data_64.to_be_bytes());
        b.extend(t.user_data_32.to_be_bytes());
    }
    b
}

pub fn parse_plan(bytes: &[u8]) -> Result<Plan> {
    let mut r = Reader::new(bytes, PLAN)?;
    let cluster = r.u128()?;
    let generation = r.u64()?;
    let operation = r.u128()?;
    let epoch = r.u64()?;
    let n = r.u16()? as usize;
    if n == 0 || n > MAX_LEGS {
        return Err(Error::InvalidPlan);
    }
    let mut transfers = Vec::new();
    for _ in 0..n {
        let id = r.u128()?;
        let debit_account_id = r.u128()?;
        let credit_account_id = r.u128()?;
        let amount = r.u128()?;
        let user_data_128 = r.u128()?;
        transfers.push(Transfer {
            id,
            debit_account_id,
            credit_account_id,
            amount,
            user_data_128,
            ledger: r.u32()?,
            code: r.u16()?,
            user_data_64: r.u64()?,
            user_data_32: r.u32()?,
        });
    }
    r.finish()?;
    Plan::new(cluster, generation, operation, epoch, transfers)
}

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8], domain: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_EVIDENCE || !bytes.starts_with(domain) {
            return Err(Error::InvalidPlan);
        }
        Ok(Self {
            bytes,
            at: domain.len(),
        })
    }
    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or(Error::InvalidPlan)?;
        let b = self.bytes.get(self.at..end).ok_or(Error::InvalidPlan)?;
        self.at = end;
        Ok(b)
    }
    pub(crate) fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub(crate) fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub(crate) fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub(crate) fn u128(&mut self) -> Result<u128> {
        Ok(u128::from_be_bytes(self.take(16)?.try_into().unwrap()))
    }
    pub(crate) fn finish(&self) -> Result<()> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(Error::InvalidPlan)
        }
    }
}

#[derive(Clone, Debug)]
pub struct Admission {
    pub bytes: Vec<u8>,
    pub signature: [u8; 64],
}
impl Admission {
    /// Test authority only. Production admission must come through its own gate.
    pub fn sign(
        registry: &Registry,
        plan: &Plan,
        original: &[u8],
        key: &SigningKey,
    ) -> Result<Self> {
        if original.is_empty()
            || original.len() > MAX_EVIDENCE / 2
            || plan.epoch() != registry.epoch
        {
            return Err(Error::InvalidPlan);
        }
        registry.validate_plan(plan)?;
        let mut bytes = ADMISSION.to_vec();
        bytes.extend(registry.digest()?);
        let p = plan_bytes(plan);
        bytes.extend((p.len() as u32).to_be_bytes());
        bytes.extend(p);
        bytes.extend((original.len() as u32).to_be_bytes());
        bytes.extend(original);
        let signature = key.sign(&bytes).to_bytes();
        Ok(Self { bytes, signature })
    }
    pub fn verify(&self, registry: &Registry, key: &VerifyingKey) -> Result<Plan> {
        key.verify_strict(&self.bytes, &Signature::from_bytes(&self.signature))
            .map_err(|_| Error::Conflict)?;
        let mut r = Reader::new(&self.bytes, ADMISSION)?;
        if r.take(32)? != registry.digest()? {
            return Err(Error::Conflict);
        }
        let n = r.u32()? as usize;
        let plan = parse_plan(r.take(n)?)?;
        let n = r.u32()? as usize;
        if n == 0 {
            return Err(Error::InvalidPlan);
        }
        r.take(n)?;
        r.finish()?;
        if plan.epoch() != registry.epoch {
            return Err(Error::Conflict);
        }
        registry.validate_plan(&plan)?;
        for t in plan.transfers() {
            if !registry
                .accounts
                .iter()
                .any(|a| a.native_id == t.debit_account_id && a.ledger == t.ledger)
                || !registry
                    .accounts
                    .iter()
                    .any(|a| a.native_id == t.credit_account_id && a.ledger == t.ledger)
            {
                return Err(Error::Conflict);
            }
        }
        Ok(plan)
    }
}

pub fn outcome_bytes(
    group: &Confirmed,
    admission: &Admission,
    destination: &[u8; 32],
    previous: u64,
) -> Vec<u8> {
    let mut b = b"hawala-tb-first-test-outcome/v1\0".to_vec();
    b.extend(destination);
    b.extend(previous.to_be_bytes());
    b.extend((admission.bytes.len() as u32).to_be_bytes());
    b.extend(&admission.bytes);
    b.extend(admission.signature);
    let p = plan_bytes(group.plan());
    b.extend((p.len() as u32).to_be_bytes());
    b.extend(p);
    for r in group.records() {
        b.extend(r.timestamp.to_be_bytes());
    }
    b
}
