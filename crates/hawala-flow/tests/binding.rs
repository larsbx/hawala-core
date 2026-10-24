use ed25519_dalek::SigningKey;
use hawala_flow::{bind, requirements, Error, FlowKind, Role};
use hawala_identity_core::IdentityId;
use hawala_interaction::{agree, sign_intent, Action, AgreedInteraction, InteractionIntent, InteractionMode, Participant};
use hawala_protocol::{accept_terms, sign_terms, SignedTerms, Terms};

fn key(n: u8) -> SigningKey { SigningKey::from_bytes(&[n; 32]) }
fn id(n: u8) -> IdentityId { IdentityId::from_signing_key(&key(n)) }

fn accepted() -> SignedTerms {
    let terms = Terms {
        transfer_id: [21; 32],
        sender: id(1), beneficiary: id(2), origin: id(3), disburser: id(4),
        funding_minor: 9_700, funding_currency: *b"USD",
        payout_minor: 8_200, payout_currency: *b"EUR",
        proposed_at: 80, accepted_at: 100, quote_expires_at: 110,
    };
    let origin = sign_terms(&key(3), &terms).unwrap();
    let disburser = sign_terms(&key(4), &terms).unwrap();
    accept_terms(terms, origin, disburser).unwrap()
}

fn who(principal: u8, machine: bool, device: u8) -> (Participant, SigningKey) {
    if machine {
        (Participant::machine(id(device), id(principal)).unwrap(), key(device))
    } else {
        (Participant::human(id(principal)), key(principal))
    }
}

fn pair(
    signed_terms: &SignedTerms,
    action: Action,
    left: (Participant, SigningKey),
    right: (Participant, SigningKey),
    reverse: bool,
) -> AgreedInteraction {
    let (a, b) = if reverse { (right, left) } else { (left, right) };
    let intent = InteractionIntent {
        initiator: a.0,
        responder: b.0,
        action,
        hawala_terms_digest: signed_terms.terms().digest(),
        interaction_nonce: [44; 32],
        created_at: 120,
        expires_at: 220,
    };
    let s_a = sign_intent(&a.1, &intent).unwrap();
    let s_b = sign_intent(&b.1, &intent).unwrap();
    agree(intent, s_a, s_b, 140).unwrap()
}

#[test]
fn universal_cashout_hh_hm_mh_mm_binds_same_signed_hawala_terms() {
    let terms = accepted();
    let cases = [
        (false, false, InteractionMode::HumanToHuman),
        (false, true, InteractionMode::HumanToMachine),
        (true, false, InteractionMode::MachineToHuman),
        (true, true, InteractionMode::MachineToMachine),
    ];
    for (left_machine, right_machine, mode) in cases {
        let left = who(2, left_machine, 12);
        let right = who(4, right_machine, 14);
        let receipt = pair(&terms, Action::DistributeCash, left, right, false);
        let bound = bind(&terms, &receipt, 140).unwrap();
        assert_eq!(bound.kind(), FlowKind::CashOut);
        assert_eq!(bound.mode(), mode);
        assert_eq!(bound.initiator_role(), Role::Left);
        assert_eq!(bound.transfer_id(), terms.terms().transfer_id);
        assert_eq!(bound.terms_digest(), terms.terms().digest());
        assert_eq!(requirements(&bound).trusted_device_delegation, left_machine || right_machine);
        assert!(requirements(&bound).transaction_bound_person_authorization);
        let flipped = pair(
            &terms, Action::DistributeCash,
            who(2, left_machine, 12), who(4, right_machine, 14), true,
        );
        let reverse = bind(&terms, &flipped, 140).unwrap();
        assert_eq!(reverse.initiator_role(), Role::Right);
        assert_eq!(reverse.mode(), match mode {
            InteractionMode::HumanToMachine => InteractionMode::MachineToHuman,
            InteractionMode::MachineToHuman => InteractionMode::HumanToMachine,
            m => m,
        });
    }
}

#[test]
fn cash_in_and_agent_coordination_allow_all_pair_kinds() {
    let terms = accepted();
    for (left_machine, right_machine) in [(false, false), (false, true), (true, false), (true, true)] {
        for (action, l, r, kind) in [
            (Action::AcceptCash, 1, 3, FlowKind::CashIn),
            (Action::TransferValue, 3, 4, FlowKind::AgentTransfer),
            (Action::ReconcileObligation, 3, 4, FlowKind::AgentReconciliation),
        ] {
            let receipt = pair(
                &terms, action, who(l, left_machine, 15),
                who(r, right_machine, 16), false,
            );
            let bound = bind(&terms, &receipt, 140).unwrap();
            assert_eq!(bound.kind(), kind);
            assert!(requirements(&bound).separate_financial_policy);
            assert!(requirements(&bound).durable_once_only_attempt);
            assert_eq!(
                requirements(&bound).transaction_bound_person_authorization,
                action == Action::AcceptCash
            );
        }
    }
}

#[test]
fn refuses_substituted_terms_parties_and_operations() {
    let terms = accepted();
    let payout = pair(
        &terms, Action::DistributeCash,
        who(2, false, 12), who(4, true, 14), false,
    );
    let mut changed_terms = terms.terms().clone();
    changed_terms.payout_minor += 1;
    let origin = sign_terms(&key(3), &changed_terms).unwrap();
    let disburser = sign_terms(&key(4), &changed_terms).unwrap();
    let changed = accept_terms(changed_terms, origin, disburser).unwrap();
    assert_eq!(bind(&changed, &payout, 140), Err(Error::WrongObligation));

    let stranger = pair(
        &terms, Action::DistributeCash,
        who(1, false, 12), who(4, true, 14), false,
    );
    assert_eq!(bind(&terms, &stranger, 140), Err(Error::PrincipalMismatch));
    let spoofed_machine = pair(
        &terms, Action::DistributeCash,
        who(2, false, 12),
        (Participant::machine(id(14), id(3)).unwrap(), key(14)),
        false,
    );
    assert_eq!(bind(&terms, &spoofed_machine, 140), Err(Error::PrincipalMismatch));
    let wrong_action = pair(
        &terms, Action::AcceptCash, who(2, false, 12), who(4, true, 14), false,
    );
    assert_eq!(bind(&terms, &wrong_action, 140), Err(Error::PrincipalMismatch));
    assert!(matches!(
        bind(&terms, &payout, 221),
        Err(Error::InvalidInteraction(hawala_interaction::Error::Expired))
    ));
}

#[test]
fn scoped_signatures_do_not_create_machine_operating_permissions() {
    let terms = accepted();
    let receipt = pair(
        &terms, Action::ReconcileObligation,
        who(3, true, 15), who(4, true, 16), false,
    );
    let bounded = bind(&terms, &receipt, 140).unwrap();
    assert_eq!(bounded.mode(), InteractionMode::MachineToMachine);
    assert!(bounded.requires_machine_authority());
    // Machine ownership/financial delegation is NOT in this crate. An
    // untrusted device can claim an arbitrary operator; financial policy
    // MUST independently validate that claim before any execution.
}
