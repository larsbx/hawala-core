//! M6-C1: immutable *preflight plan* for a possible Hawala cash-out.
//!
//! NOT a durable financial commit, custodian interface, settlement, or device
//! permit. No public method returns an executable authorization. This is an
//! inspectable pure check that brings signed and locally reported evidence
//! together, and previews balanced journal postings. Reported outcomes are
//! inputs, not proof of their truth: a caller can lie about them. Never
//! connect this API to a cash dispenser or spendable customer balance.
use hawala_admission::{evaluate, Authorities, EvidenceBundle};
use hawala_identity_core::IdentityId;
use hawala_interaction::AgreedInteraction;
use hawala_ledger::{Book, Entry};
use hawala_protocol::SignedTerms;
use hawala_qr_auth::SignedAuthorization;
use hawala_safety::quote_amounts;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Review { ReportedClear, Denied, Unknown }

/// Each result is a CLAIM from independently authenticated and verified
/// external control systems. These fields are not self-attesting and do NOT
/// establish issuer authority, collateral or regulatory clearance by themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExternalReviews {
    pub custody_asset_control: Review,
    pub funding_finality: Review,
    pub dar_eligibility: Review,
    pub sanctions_and_aml: Review,
    pub secured_exposure: Review,
    pub recipient_quote_acceptance: Review,
}
impl ExternalReviews {
    fn status(self) -> std::result::Result<(), Decision> {
        let statuses = [
            self.custody_asset_control, self.funding_finality,
            self.dar_eligibility, self.sanctions_and_aml,
            self.secured_exposure, self.recipient_quote_acceptance,
        ];
        if statuses.contains(&Review::Denied) {
            return Err(Decision::Rejected(RejectReason::DeniedByExternalControl));
        }
        if statuses.contains(&Review::Unknown) {
            return Err(Decision::Held(HoldReason::MissingIndependentEvidence));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quote {
    /// Rate is destination minor units per source minor unit.
    pub rate_num: u64,
    pub rate_den: u64,
    pub source_fee_minor: u64,
    pub destination_fee_minor: u64,
    /// Independently resolved signed executable quote reference.
    pub quote_digest: [u8; 32],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Versions {
    pub authority_epoch: u64,
    pub account_version: u64,
    pub custody_account_version: u64,
    pub custody_authority_epoch: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectReason {
    InvalidEvidence,
    InvalidQuote,
    WrongEventId,
    InvalidLedgerEntry,
    DeniedByExternalControl,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldReason {
    MissingIndependentEvidence,
    StaleVersionOrEpoch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreviewPlan {
    /// Opaque digest for a subsequent independently-authorized state commit.
    /// It is NOT a token accepted by financial services or cash equipment.
    pub plan_digest: [u8; 32],
    pub transfer_id: [u8; 32],
    pub attempt_id: [u8; 32],
    pub nonce: [u8; 32],
    pub ledger_event_id: [u8; 32],
    pub authority_epoch: u64,
    pub account_version: u64,
    pub custody_account_version: u64,
    pub currency: [u8; 3],
    pub payout_minor: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Rejected(RejectReason),
    Held(HoldReason),
    /// PREPARED is NOT committed, spendable or a hardware execution permit.
    Prepared(PreviewPlan),
}

pub struct AuthorityEnvelope<'a> {
    pub accepted: &'a SignedTerms,
    pub agreement: &'a AgreedInteraction,
    pub qr: &'a SignedAuthorization,
    pub expected_qr_nonce: [u8; 32],
    pub expected_challenge: [u8; 32],
    pub expected_session: [u8; 32],
    pub expected_verifier: IdentityId,
    pub pinned_authorities: &'a Authorities,
    pub assertions: &'a EvidenceBundle,
    pub exact_quote: Quote,
    pub versions: Versions,
    pub external_reviews: ExternalReviews,
    pub reserve_entry: &'a Entry,
}

/// Deterministic domain-separated event ID for the local payout reservation.
/// Financial authority must retain event-ID tombstones beyond expiry.
pub fn reserve_event_id(transfer_id: [u8; 32]) -> [u8; 32] {
    let mut h = blake3::Hasher::new_derive_key("hawala/reserve-ledger-event/v1");
    h.update(&transfer_id);
    *h.finalize().as_bytes()
}

/// Strictly pure, no state mutation, no risk of mistaking a plan for money.
/// The plan digest includes the full canonical sorted journal and the
/// externally provided custody/account versions; no state is persisted.
pub fn admit_preview(
    envelope: &AuthorityEnvelope<'_>,
    book: &Book,
    now: u64,
) -> Decision {
    let a = envelope;
    let t = a.accepted.terms();
    let signed = match evaluate(
        a.accepted, a.agreement, a.qr, a.expected_qr_nonce,
        a.expected_challenge, a.expected_session, a.pinned_authorities,
        a.assertions, now,
    ) {
        Ok(v) => v,
        Err(_) => return Decision::Rejected(RejectReason::InvalidEvidence),
    };
    let q = a.exact_quote;
    if q.quote_digest == [0; 32] || q.rate_num == 0 || q.rate_den == 0
        || q.destination_fee_minor != a.pinned_authorities.expected_fee_minor
    {
        return Decision::Rejected(RejectReason::InvalidQuote);
    }
    let amounts = match quote_amounts(
        t.funding_minor, q.source_fee_minor, q.destination_fee_minor,
        q.rate_num, q.rate_den,
    ) {
        Ok(v) => v,
        Err(_) => return Decision::Rejected(RejectReason::InvalidQuote),
    };
    if amounts.destination_credit != t.payout_minor {
        return Decision::Rejected(RejectReason::InvalidQuote);
    }
    if let Err(decision) = a.external_reviews.status() { return decision; }
    if a.versions.authority_epoch == 0 || a.versions.account_version == 0
        || a.versions.authority_epoch != a.pinned_authorities.epoch
        || a.versions.custody_authority_epoch != a.pinned_authorities.epoch
        || a.versions.custody_account_version != signed.custody_version
    {
        return Decision::Held(HoldReason::StaleVersionOrEpoch);
    }
    let expected_event = reserve_event_id(t.transfer_id);
    if a.reserve_entry.event_id != expected_event {
        return Decision::Rejected(RejectReason::WrongEventId);
    }
    // Verify reference journal balanced per currency and no overdraft,
    // WITHOUT ever posting it to a real account or retaining the clone.
    let mut trial = book.clone();
    match trial.post(a.reserve_entry.clone()) {
        Ok(true) => {},
        _ => return Decision::Rejected(RejectReason::InvalidLedgerEntry),
    }

    // The canonical digest includes normal-side metadata and all postings.
    // Book::post has already checked the same account registry. This plan
    // remains a pure hash of intended operations and reviewed versions.
    let mut postings = a.reserve_entry.postings.clone();
    postings.sort_by_key(|p| p.account);
    let mut h = blake3::Hasher::new_derive_key("hawala/admission-plan/v1");
    let terms_digest = t.digest();
    let qr_digest = a.qr.digest();
    let epoch = a.versions.authority_epoch.to_be_bytes();
    let account_version = a.versions.account_version.to_be_bytes();
    let custody_version = a.versions.custody_account_version.to_be_bytes();
    for bytes in [
        t.transfer_id.as_slice(), terms_digest.as_slice(),
        signed.attempt_id.as_slice(), a.qr.request().qr_nonce.as_slice(),
        qr_digest.as_slice(), expected_event.as_slice(),
        q.quote_digest.as_slice(),
        epoch.as_slice(), account_version.as_slice(), custody_version.as_slice(),
    ] {
        h.update(&(bytes.len() as u32).to_be_bytes());
        h.update(bytes);
    }
    for p in postings {
        let account = match book.account(p.account) {
            Some(a) => a,
            None => return Decision::Rejected(RejectReason::InvalidLedgerEntry),
        };
        h.update(&p.account);
        h.update(&account.currency);
        h.update(&[match account.normal {
            hawala_ledger::NormalSide::Debit => 1,
            hawala_ledger::NormalSide::Credit => 2,
        }]);
        h.update(&p.debit_minor.to_be_bytes());
        h.update(&p.credit_minor.to_be_bytes());
    }
    Decision::Prepared(PreviewPlan {
        plan_digest: *h.finalize().as_bytes(),
        transfer_id: t.transfer_id,
        attempt_id: signed.attempt_id,
        nonce: a.qr.request().qr_nonce,
        ledger_event_id: expected_event,
        authority_epoch: a.versions.authority_epoch,
        account_version: a.versions.account_version,
        custody_account_version: a.versions.custody_account_version,
        currency: t.payout_currency,
        payout_minor: t.payout_minor,
    })
}
