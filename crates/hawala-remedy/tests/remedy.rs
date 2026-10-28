//! ADR 0005 gap G4: refunds, replacements and compensation are new dual-signed
//! obligations linked to the original, never rewrites of it. A dispute opens a
//! case; only consent or a pinned adjudicator's award is a basis for a remedy,
//! and cumulative caps prevent recovering the same loss twice.
use ed25519_dalek::SigningKey;
use hawala_identity_core::IdentityId;
use hawala_protocol::{accept_terms, sign_terms, SignedTerms, Terms};
use hawala_remedy::*;

const SENDER: u8 = 1;
const BENEFICIARY: u8 = 2;
const ORIGIN_DAR: u8 = 3;
const DISBURSER_DAR: u8 = 4;
const ADJUDICATOR: u8 = 30;
const MANDATE: [u8; 32] = [31; 32];

fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}

fn id(n: u8) -> IdentityId {
    IdentityId::from_signing_key(&key(n))
}

/// Accepted terms paying `amount` of `currency` from `sender` to `beneficiary`.
fn terms(
    transfer: u8,
    sender: u8,
    beneficiary: u8,
    origin: u8,
    disburser: u8,
    amount: u64,
    currency: &[u8; 3],
) -> SignedTerms {
    let t = Terms {
        transfer_id: [transfer; 32],
        sender: id(sender),
        beneficiary: id(beneficiary),
        origin: id(origin),
        disburser: id(disburser),
        funding_minor: amount,
        funding_currency: *currency,
        payout_minor: amount,
        payout_currency: *currency,
        proposed_at: 100,
        accepted_at: 110,
        quote_expires_at: 120,
    };
    let a = sign_terms(&key(origin), &t).unwrap();
    let b = sign_terms(&key(disburser), &t).unwrap();
    accept_terms(t, a, b).unwrap()
}

/// The original payment: 10 000 USD from SENDER to BENEFICIARY.
fn original() -> SignedTerms {
    terms(
        9,
        SENDER,
        BENEFICIARY,
        ORIGIN_DAR,
        DISBURSER_DAR,
        10_000,
        b"USD",
    )
}

/// A refund obligation: BENEFICIARY pays `amount` USD back to SENDER.
fn refund_terms(transfer: u8, amount: u64) -> SignedTerms {
    terms(
        transfer,
        BENEFICIARY,
        SENDER,
        DISBURSER_DAR,
        ORIGIN_DAR,
        amount,
        b"USD",
    )
}

fn book() -> RemedyBook {
    RemedyBook::open(
        original(),
        Adjudicator {
            key: id(ADJUDICATOR),
            mandate_digest: MANDATE,
        },
    )
}

fn consented(kind: Kind, by: u8, remedy_terms: SignedTerms) -> Remedy {
    let signature = consent(&key(by), kind, &original(), &remedy_terms);
    let basis = match kind {
        Kind::Replacement => Basis::SenderConsent(signature),
        _ => Basis::PayeeConsent(signature),
    };
    Remedy {
        kind,
        terms: remedy_terms,
        basis,
    }
}

fn award(kind: Kind, claimant: u8, liable: u8, currency: &[u8; 3], amount: u64) -> Award {
    Award {
        case_id: [40; 32],
        original_terms_digest: original().terms().digest(),
        mandate_digest: MANDATE,
        kind,
        claimant: id(claimant),
        liable: id(liable),
        currency: *currency,
        amount,
        issued_at: 500,
    }
}

fn awarded(kind: Kind, a: Award, by: u8, remedy_terms: SignedTerms) -> Remedy {
    Remedy {
        kind,
        terms: remedy_terms,
        basis: Basis::Award(sign_award(&key(by), a).unwrap()),
    }
}

#[test]
fn a_consented_refund_is_a_new_obligation_and_the_original_is_untouched() {
    let b = book();
    let after = b
        .admit(&consented(
            Kind::Refund,
            BENEFICIARY,
            refund_terms(20, 4_000),
        ))
        .unwrap();
    assert_eq!(after.refunded(), 4_000);
    assert_eq!(after.original(), &original());
    assert_eq!(b.refunded(), 0, "admit never mutates the prior book");
}

#[test]
fn only_the_original_payee_can_consent_to_a_refund() {
    let refund = refund_terms(20, 4_000);
    assert_eq!(
        book()
            .admit(&consented(Kind::Refund, 66, refund.clone()))
            .unwrap_err(),
        Error::InvalidSignature
    );
    let by_sender = Remedy {
        kind: Kind::Refund,
        terms: refund.clone(),
        basis: Basis::SenderConsent(consent(&key(SENDER), Kind::Refund, &original(), &refund)),
    };
    assert_eq!(
        book().admit(&by_sender).unwrap_err(),
        Error::BasisNotPermitted
    );
}

#[test]
fn refunds_must_flow_back_to_the_sender_in_the_funding_currency() {
    let to_stranger = terms(
        20,
        BENEFICIARY,
        66,
        DISBURSER_DAR,
        ORIGIN_DAR,
        4_000,
        b"USD",
    );
    assert_eq!(
        book()
            .admit(&consented(Kind::Refund, BENEFICIARY, to_stranger))
            .unwrap_err(),
        Error::WrongShape
    );
    let in_euros = terms(
        20,
        BENEFICIARY,
        SENDER,
        DISBURSER_DAR,
        ORIGIN_DAR,
        4_000,
        b"EUR",
    );
    assert_eq!(
        book()
            .admit(&consented(Kind::Refund, BENEFICIARY, in_euros))
            .unwrap_err(),
        Error::WrongShape
    );
}

#[test]
fn cumulative_refunds_cannot_exceed_what_the_sender_paid() {
    let b = book()
        .admit(&consented(
            Kind::Refund,
            BENEFICIARY,
            refund_terms(20, 6_000),
        ))
        .unwrap();
    let b = b
        .admit(&consented(
            Kind::Refund,
            BENEFICIARY,
            refund_terms(21, 4_000),
        ))
        .unwrap();
    assert_eq!(b.refunded(), 10_000);
    assert_eq!(
        b.admit(&consented(Kind::Refund, BENEFICIARY, refund_terms(22, 1)))
            .unwrap_err(),
        Error::ExceedsOriginal
    );
}

#[test]
fn every_remedy_has_its_own_transfer_and_replay_is_idempotent() {
    let reuse_original = terms(
        9,
        BENEFICIARY,
        SENDER,
        DISBURSER_DAR,
        ORIGIN_DAR,
        1_000,
        b"USD",
    );
    assert_eq!(
        book()
            .admit(&consented(Kind::Refund, BENEFICIARY, reuse_original))
            .unwrap_err(),
        Error::DuplicateTransfer
    );
    let first = consented(Kind::Refund, BENEFICIARY, refund_terms(20, 4_000));
    let b = book().admit(&first).unwrap();
    assert_eq!(b.admit(&first).unwrap(), b);
    let same_id_other_amount = consented(Kind::Refund, BENEFICIARY, refund_terms(20, 3_000));
    assert_eq!(
        b.admit(&same_id_other_amount).unwrap_err(),
        Error::DuplicateTransfer
    );
}

#[test]
fn compensation_needs_an_award_from_the_pinned_adjudicator_under_its_mandate() {
    let comp = terms(
        20,
        BENEFICIARY,
        SENDER,
        DISBURSER_DAR,
        ORIGIN_DAR,
        500,
        b"USD",
    );
    let a = award(Kind::Compensation, SENDER, BENEFICIARY, b"USD", 500);
    assert_eq!(
        book()
            .admit(&consented(Kind::Compensation, BENEFICIARY, comp.clone()))
            .unwrap_err(),
        Error::BasisNotPermitted
    );
    assert_eq!(
        book()
            .admit(&awarded(Kind::Compensation, a.clone(), 66, comp.clone()))
            .unwrap_err(),
        Error::WrongAdjudicator
    );
    let mut other_mandate = a.clone();
    other_mandate.mandate_digest = [0xEE; 32];
    assert_eq!(
        book()
            .admit(&awarded(
                Kind::Compensation,
                other_mandate,
                ADJUDICATOR,
                comp.clone()
            ))
            .unwrap_err(),
        Error::WrongAdjudicator
    );
    let mut other_case = a.clone();
    other_case.original_terms_digest = [0xEE; 32];
    assert_eq!(
        book()
            .admit(&awarded(
                Kind::Compensation,
                other_case,
                ADJUDICATOR,
                comp.clone()
            ))
            .unwrap_err(),
        Error::AwardMismatch
    );
    let refund_award = award(Kind::Refund, SENDER, BENEFICIARY, b"USD", 500);
    assert_eq!(
        book()
            .admit(&awarded(
                Kind::Compensation,
                refund_award,
                ADJUDICATOR,
                comp.clone()
            ))
            .unwrap_err(),
        Error::AwardMismatch
    );
    assert!(book()
        .admit(&awarded(Kind::Compensation, a, ADJUDICATOR, comp))
        .is_ok());
}

#[test]
fn compensation_flows_from_the_liable_party_to_the_claimant_within_the_award() {
    let a = award(Kind::Compensation, SENDER, BENEFICIARY, b"USD", 500);
    let reversed = terms(
        20,
        SENDER,
        BENEFICIARY,
        ORIGIN_DAR,
        DISBURSER_DAR,
        500,
        b"USD",
    );
    assert_eq!(
        book()
            .admit(&awarded(
                Kind::Compensation,
                a.clone(),
                ADJUDICATOR,
                reversed
            ))
            .unwrap_err(),
        Error::WrongShape
    );
    let part = |t, amount| {
        terms(
            t,
            BENEFICIARY,
            SENDER,
            DISBURSER_DAR,
            ORIGIN_DAR,
            amount,
            b"USD",
        )
    };
    let b = book()
        .admit(&awarded(
            Kind::Compensation,
            a.clone(),
            ADJUDICATOR,
            part(20, 300),
        ))
        .unwrap();
    let b = b
        .admit(&awarded(
            Kind::Compensation,
            a.clone(),
            ADJUDICATOR,
            part(21, 200),
        ))
        .unwrap();
    assert_eq!(
        b.admit(&awarded(Kind::Compensation, a, ADJUDICATOR, part(22, 1)))
            .unwrap_err(),
        Error::ExceedsAward
    );
}

#[test]
fn awarded_and_consented_refunds_share_one_cap() {
    // A loss recovered by agreement cannot be recovered again by award.
    let b = book()
        .admit(&consented(
            Kind::Refund,
            BENEFICIARY,
            refund_terms(20, 7_000),
        ))
        .unwrap();
    let a = award(Kind::Refund, SENDER, BENEFICIARY, b"USD", 10_000);
    let b = b
        .admit(&awarded(
            Kind::Refund,
            a.clone(),
            ADJUDICATOR,
            refund_terms(21, 3_000),
        ))
        .unwrap();
    assert_eq!(
        b.admit(&awarded(Kind::Refund, a, ADJUDICATOR, refund_terms(22, 1)))
            .unwrap_err(),
        Error::ExceedsOriginal
    );
}

#[test]
fn replacements_resend_to_the_same_beneficiary_up_to_the_original_payout() {
    let replacement = |t, amount| {
        terms(
            t,
            SENDER,
            BENEFICIARY,
            ORIGIN_DAR,
            DISBURSER_DAR,
            amount,
            b"USD",
        )
    };
    let b = book()
        .admit(&consented(
            Kind::Replacement,
            SENDER,
            replacement(20, 10_000),
        ))
        .unwrap();
    assert_eq!(b.replaced(), 10_000);
    assert_eq!(
        b.admit(&consented(Kind::Replacement, SENDER, replacement(21, 1)))
            .unwrap_err(),
        Error::ExceedsOriginal
    );
    assert_eq!(
        book()
            .admit(&consented(
                Kind::Replacement,
                BENEFICIARY,
                replacement(20, 1)
            ))
            .unwrap_err(),
        Error::InvalidSignature
    );
    let to_sender = terms(
        20,
        BENEFICIARY,
        SENDER,
        DISBURSER_DAR,
        ORIGIN_DAR,
        1,
        b"USD",
    );
    assert_eq!(
        book()
            .admit(&consented(Kind::Replacement, SENDER, to_sender))
            .unwrap_err(),
        Error::WrongShape
    );
}

fn dispute(claimant: u8, original_digest: [u8; 32]) -> Dispute {
    Dispute {
        case_id: [40; 32],
        original_terms_digest: original_digest,
        claimant: id(claimant),
        reason_digest: [41; 32],
        evidence_digest: [42; 32],
        opened_at: 400,
    }
}

#[test]
fn any_party_to_the_original_can_open_a_case_and_nobody_else_can() {
    let digest = original().terms().digest();
    for party in [SENDER, BENEFICIARY, ORIGIN_DAR, DISBURSER_DAR] {
        let d = sign_dispute(&key(party), dispute(party, digest)).unwrap();
        assert_eq!(verify_dispute(&d, &original()), Ok(()));
    }
    let outsider = sign_dispute(&key(66), dispute(66, digest)).unwrap();
    assert_eq!(
        verify_dispute(&outsider, &original()),
        Err(Error::NotAParty)
    );
    let elsewhere = sign_dispute(&key(SENDER), dispute(SENDER, [0xEE; 32])).unwrap();
    assert_eq!(
        verify_dispute(&elsewhere, &original()),
        Err(Error::WrongOriginal)
    );
    assert_eq!(
        sign_dispute(&key(SENDER), dispute(BENEFICIARY, digest)).unwrap_err(),
        Error::WrongSigner
    );
}

#[test]
fn every_signing_input_lives_in_the_hawala_domain() {
    let digest = original().terms().digest();
    assert!(sign_dispute(&key(SENDER), dispute(SENDER, digest))
        .unwrap()
        .signing_bytes()
        .starts_with(b"hawala"));
    let a = sign_award(
        &key(ADJUDICATOR),
        award(Kind::Refund, SENDER, BENEFICIARY, b"USD", 1),
    )
    .unwrap();
    assert!(a.signing_bytes().starts_with(b"hawala"));
    assert!(consent_bytes(Kind::Refund, &original(), &refund_terms(20, 1)).starts_with(b"hawala"));
}

#[test]
fn awards_must_be_well_formed() {
    let zero = award(Kind::Refund, SENDER, BENEFICIARY, b"USD", 0);
    assert_eq!(
        sign_award(&key(ADJUDICATOR), zero).unwrap_err(),
        Error::InvalidAward
    );
    let self_award = award(Kind::Compensation, SENDER, SENDER, b"USD", 5);
    assert_eq!(
        sign_award(&key(ADJUDICATOR), self_award).unwrap_err(),
        Error::InvalidAward
    );
}

#[test]
fn a_second_different_award_for_the_same_case_is_a_conflict() {
    let first = award(Kind::Compensation, SENDER, BENEFICIARY, b"USD", 500);
    let part = |t, amount| {
        terms(
            t,
            BENEFICIARY,
            SENDER,
            DISBURSER_DAR,
            ORIGIN_DAR,
            amount,
            b"USD",
        )
    };
    let b = book()
        .admit(&awarded(
            Kind::Compensation,
            first.clone(),
            ADJUDICATOR,
            part(20, 500),
        ))
        .unwrap();
    let mut second = first;
    second.amount = 900;
    second.issued_at = 600;
    assert_eq!(
        b.admit(&awarded(
            Kind::Compensation,
            second,
            ADJUDICATOR,
            part(21, 400)
        ))
        .unwrap_err(),
        Error::AwardConflict
    );
}

#[test]
fn a_replacement_spends_the_senders_money_so_only_the_sender_can_consent() {
    let replacement = terms(
        20,
        SENDER,
        BENEFICIARY,
        ORIGIN_DAR,
        DISBURSER_DAR,
        1_000,
        b"USD",
    );
    let by_payee = Remedy {
        kind: Kind::Replacement,
        terms: replacement.clone(),
        basis: Basis::PayeeConsent(consent(
            &key(BENEFICIARY),
            Kind::Replacement,
            &original(),
            &replacement,
        )),
    };
    assert_eq!(
        book().admit(&by_payee).unwrap_err(),
        Error::BasisNotPermitted
    );
}
