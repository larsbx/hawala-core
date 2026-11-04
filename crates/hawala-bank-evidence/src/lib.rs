//! Source-attributed, minimal-disclosure financial predicate attestations.
//!
//! SECURITY: An accepted attestation establishes ONLY that a specifically
//! pinned attestor signed an exact public predicate at a stated observation
//! time. It is NOT a zero-knowledge proof, bank-origin authentication, account
//! ownership, reserved funds, payment finality, or financial permission.
//! No bank APIs, customer credentials, raw bank data, execution, or storage.
use std::collections::BTreeSet;

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

const DOMAIN: &[u8] = b"hawala/bank-predicate-attestation/v1\0";
const EVIDENCE_DOMAIN: &[u8] = b"hawala/bank-predicate-evidence-id/v1\0";
pub const VERSION: u16 = 1;
pub const MAX_MINOR: u64 = i64::MAX as u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    UnsupportedVersion,
    Malformed,
    InvalidPolicy,
    UntrustedAttestor,
    WrongStatement,
    BadSignature,
    NotYetObserved,
    Expired,
    TooOld,
    TooLong,
    Replay,
    Storage,
}
pub type Result<T> = std::result::Result<T, Error>;

/// Explicit predicate: *not* a payment instruction or amount reservation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Predicate {
    AccountEligible = 1,
    BalanceAtLeast = 2,
    ExactPaymentObserved = 3,
}

/// Only public, intentionally disclosed inputs. Account identifiers and raw
/// balances MUST remain with the customer's private bank connector.
/// Account commitment is an independently randomized, purpose-scoped digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Statement {
    pub version: u16,
    pub predicate: Predicate,
    pub business_id: [u8; 32],
    pub account_commitment: [u8; 32],
    pub obligation_digest: [u8; 32],
    pub amount_minor: u64,
    pub currency: [u8; 3],
    pub nonce: [u8; 32],
}

impl Statement {
    fn validate(&self) -> Result<()> {
        if self.version != VERSION {
            return Err(Error::UnsupportedVersion);
        }
        if self.business_id == [0; 32]
            || self.account_commitment == [0; 32]
            || self.nonce == [0; 32]
            || self.amount_minor > MAX_MINOR
            || !self.currency.iter().all(|c| c.is_ascii_uppercase())
        {
            return Err(Error::Malformed);
        }
        match self.predicate {
            Predicate::AccountEligible => {
                if self.amount_minor != 0 || self.obligation_digest != [0; 32] {
                    return Err(Error::Malformed);
                }
            }
            Predicate::BalanceAtLeast => {
                if self.amount_minor == 0 || self.obligation_digest != [0; 32] {
                    return Err(Error::Malformed);
                }
            }
            Predicate::ExactPaymentObserved => {
                if self.amount_minor == 0 || self.obligation_digest == [0; 32] {
                    return Err(Error::Malformed);
                }
            }
        }
        Ok(())
    }

    pub fn signing_bytes(&self, source_digest: [u8; 32], observed_at: u64, expires_at: u64) -> Vec<u8> {
        let mut b = Vec::with_capacity(DOMAIN.len() + 2 + 1 + 32 * 5 + 8 * 3 + 3);
        b.extend_from_slice(DOMAIN);
        b.extend_from_slice(&self.version.to_be_bytes());
        b.push(self.predicate as u8);
        b.extend_from_slice(&self.business_id);
        b.extend_from_slice(&self.account_commitment);
        b.extend_from_slice(&self.obligation_digest);
        b.extend_from_slice(&self.amount_minor.to_be_bytes());
        b.extend_from_slice(&self.currency);
        b.extend_from_slice(&self.nonce);
        b.extend_from_slice(&source_digest);
        b.extend_from_slice(&observed_at.to_be_bytes());
        b.extend_from_slice(&expires_at.to_be_bytes());
        b
    }
}

/// Deliberately source-attributed *claim*, not a verified bank statement.
/// A bank or provider signature over the raw source must be checked by a
/// separate trusted adapter before the attestor signs this public assertion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignedAttestation {
    pub statement: Statement,
    pub source_digest: [u8; 32],
    pub observed_at: u64,
    pub expires_at: u64,
    pub attestor_key: [u8; 32],
    pub signature: [u8; 64],
}

impl SignedAttestation {
    pub fn signing_bytes(&self) -> Vec<u8> {
        self.statement.signing_bytes(self.source_digest, self.observed_at, self.expires_at)
    }
    pub fn evidence_id(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(EVIDENCE_DOMAIN);
        hash.update(self.attestor_key);
        hash.update(self.signing_bytes());
        hash.update(self.signature);
        hash.finalize().into()
    }
}

/// The relying business chooses its independent trusted data attestors.
/// A platform host key must never be implicitly accepted as one.
#[derive(Clone, Debug)]
pub struct Policy {
    pub attestor_keys: BTreeSet<[u8; 32]>,
    pub max_age_secs: u64,
    pub max_lifetime_secs: u64,
}
impl Policy {
    fn validate(&self) -> Result<()> {
        if self.attestor_keys.is_empty() || self.max_age_secs == 0 || self.max_lifetime_secs == 0 {
            return Err(Error::InvalidPolicy);
        }
        Ok(())
    }
}

/// Production implementations must atomically and *durably* consume the
/// business/nonce pair. In-memory guards are only appropriate for tests.
pub trait ReplayStore {
    fn consume_once(&mut self, business_id: [u8; 32], nonce: [u8; 32]) -> std::result::Result<bool, ()>;
}

/// Non-constructible outside this crate except by passing admission.
/// An admitted source attestation is never a ZK verification result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmittedAttestation {
    evidence_id: [u8; 32],
    predicate: Predicate,
}
impl AdmittedAttestation {
    pub fn evidence_id(self) -> [u8; 32] { self.evidence_id }
    pub fn predicate(self) -> Predicate { self.predicate }
}

/// Fails closed on malformed, out-of-context, untrusted, unsigned, stale,
/// replayed and unpersistable claims. Caller supplies the exact authorized
/// public statement; source-data provenance and customer consent are EXTERNAL.
pub fn admit(
    policy: &Policy,
    expected: &Statement,
    signed: &SignedAttestation,
    now: u64,
    replay: &mut impl ReplayStore,
) -> Result<AdmittedAttestation> {
    policy.validate()?;
    expected.validate()?;
    signed.statement.validate()?;
    if &signed.statement != expected {
        return Err(Error::WrongStatement);
    }
    if signed.source_digest == [0; 32]
        || signed.observed_at >= signed.expires_at
    {
        return Err(Error::Malformed);
    }
    if !policy.attestor_keys.contains(&signed.attestor_key) {
        return Err(Error::UntrustedAttestor);
    }
    let vk = VerifyingKey::from_bytes(&signed.attestor_key).map_err(|_| Error::BadSignature)?;
    let sig = Signature::from_bytes(&signed.signature);
    vk.verify_strict(&signed.signing_bytes(), &sig).map_err(|_| Error::BadSignature)?;
    if now < signed.observed_at {
        return Err(Error::NotYetObserved);
    }
    if now >= signed.expires_at {
        return Err(Error::Expired);
    }
    if signed.expires_at - signed.observed_at > policy.max_lifetime_secs {
        return Err(Error::TooLong);
    }
    if now - signed.observed_at > policy.max_age_secs {
        return Err(Error::TooOld);
    }
    match replay.consume_once(signed.statement.business_id, signed.statement.nonce) {
        Ok(true) => Ok(AdmittedAttestation {
            evidence_id: signed.evidence_id(),
            predicate: signed.statement.predicate,
        }),
        Ok(false) => Err(Error::Replay),
        Err(_) => Err(Error::Storage),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[derive(Default)]
    struct MemoryFence {
        seen: BTreeSet<([u8; 32], [u8; 32])>,
        fail: bool,
    }
    impl ReplayStore for MemoryFence {
        fn consume_once(&mut self, business: [u8; 32], nonce: [u8; 32]) -> std::result::Result<bool, ()> {
            if self.fail { return Err(()); }
            Ok(self.seen.insert((business, nonce)))
        }
    }
    fn statement(predicate: Predicate) -> Statement {
        Statement {
            version: VERSION,
            predicate,
            business_id: [7; 32],
            account_commitment: [8; 32],
            obligation_digest: if predicate == Predicate::ExactPaymentObserved { [3; 32] } else { [0; 32] },
            amount_minor: if predicate == Predicate::AccountEligible { 0 } else { 1_000_00 },
            currency: *b"USD",
            nonce: [4; 32],
        }
    }
    fn issue(s: Statement) -> (Policy, SignedAttestation) {
        let signer = SigningKey::from_bytes(&[17; 32]);
        let mut a = SignedAttestation {
            statement: s,
            source_digest: [91; 32],
            observed_at: 100,
            expires_at: 200,
            attestor_key: signer.verifying_key().to_bytes(),
            signature: [0; 64],
        };
        a.signature = signer.sign(&a.signing_bytes()).to_bytes();
        (Policy {
            attestor_keys: [a.attestor_key].into_iter().collect(),
            max_age_secs: 45,
            max_lifetime_secs: 100,
        }, a)
    }
    #[test]
    fn valid_predicates_disclose_no_raw_balance_or_account_number() {
        for kind in [Predicate::AccountEligible, Predicate::BalanceAtLeast, Predicate::ExactPaymentObserved] {
            let s = statement(kind);
            let (p, a) = issue(s);
            let approved = admit(&p, &s, &a, 112, &mut MemoryFence::default()).unwrap();
            assert_eq!(approved.predicate(), kind);
            assert_eq!(approved.evidence_id(), a.evidence_id());
        }
    }
    #[test]
    fn rejects_tamper_and_cross_business_and_obligation() {
        let s = statement(Predicate::ExactPaymentObserved);
        let (p, a) = issue(s);
        let mut wrong = a;
        wrong.statement.amount_minor += 1;
        assert_eq!(admit(&p, &s, &wrong, 112, &mut MemoryFence::default()), Err(Error::WrongStatement));
        let mut wrong = a;
        wrong.source_digest = [5; 32];
        assert_eq!(admit(&p, &s, &wrong, 112, &mut MemoryFence::default()), Err(Error::BadSignature));
        let mut expected = s;
        expected.business_id = [9; 32];
        assert_eq!(admit(&p, &expected, &a, 112, &mut MemoryFence::default()), Err(Error::WrongStatement));
        expected = s;
        expected.obligation_digest = [44; 32];
        assert_eq!(admit(&p, &expected, &a, 112, &mut MemoryFence::default()), Err(Error::WrongStatement));
    }
    #[test]
    fn refuses_untrusted_issuer_invalid_signature_and_future() {
        let s = statement(Predicate::BalanceAtLeast);
        let (mut p, mut a) = issue(s);
        p.attestor_keys = [[6; 32]].into_iter().collect();
        assert_eq!(admit(&p, &s, &a, 112, &mut MemoryFence::default()), Err(Error::UntrustedAttestor));
        p.attestor_keys = [a.attestor_key].into_iter().collect();
        a.signature[0] ^= 1;
        assert_eq!(admit(&p, &s, &a, 112, &mut MemoryFence::default()), Err(Error::BadSignature));
        let (p, a) = issue(s);
        assert_eq!(admit(&p, &s, &a, 99, &mut MemoryFence::default()), Err(Error::NotYetObserved));
        assert_eq!(admit(&p, &s, &a, 200, &mut MemoryFence::default()), Err(Error::Expired));
        assert_eq!(admit(&p, &s, &a, 150, &mut MemoryFence::default()), Err(Error::TooOld));
    }
    #[test]
    fn enforces_time_policy_and_fails_closed_on_store_error_and_replay() {
        let s = statement(Predicate::ExactPaymentObserved);
        let (mut p, a) = issue(s);
        p.max_lifetime_secs = 99;
        assert_eq!(admit(&p, &s, &a, 110, &mut MemoryFence::default()), Err(Error::TooLong));
        p.max_lifetime_secs = 100;
        let mut fence = MemoryFence::default();
        assert!(admit(&p, &s, &a, 110, &mut fence).is_ok());
        assert_eq!(admit(&p, &s, &a, 110, &mut fence), Err(Error::Replay));
        fence.fail = true;
        assert_eq!(admit(&p, &s, &a, 110, &mut fence), Err(Error::Storage));
    }
    #[test]
    fn malformed_statements_are_refused_before_signer_or_nonce() {
        let mut s = statement(Predicate::BalanceAtLeast);
        s.amount_minor = 0;
        let (p, a) = issue(s);
        assert_eq!(admit(&p, &s, &a, 110, &mut MemoryFence::default()), Err(Error::Malformed));
        s = statement(Predicate::AccountEligible);
        s.currency = *b"usd";
        let (p, a) = issue(s);
        assert_eq!(admit(&p, &s, &a, 110, &mut MemoryFence::default()), Err(Error::Malformed));
        s = statement(Predicate::BalanceAtLeast);
        s.amount_minor = MAX_MINOR + 1;
        let (p, a) = issue(s);
        assert_eq!(admit(&p, &s, &a, 110, &mut MemoryFence::default()), Err(Error::Malformed));
        s = statement(Predicate::BalanceAtLeast);
        s.nonce = [0; 32];
        let (p, a) = issue(s);
        assert_eq!(admit(&p, &s, &a, 110, &mut MemoryFence::default()), Err(Error::Malformed));
    }
    #[test]
    fn exact_signing_bytes_are_domain_separated_and_deterministic() {
        let a = statement(Predicate::BalanceAtLeast);
        let (_, signed) = issue(a);
        let bytes = signed.signing_bytes();
        assert!(bytes.starts_with(DOMAIN));
        assert_eq!(bytes, signed.signing_bytes());
        let mut changed = a;
        changed.nonce = [12; 32];
        assert_ne!(bytes, changed.signing_bytes(signed.source_digest, signed.observed_at, signed.expires_at));
    }
}
