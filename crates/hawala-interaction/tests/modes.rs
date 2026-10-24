use ed25519_dalek::SigningKey;
use hawala_identity_core::IdentityId;
use hawala_interaction::{
    agree, sign_intent, Action, Error, InteractionIntent, InteractionMode,
    Participant, ParticipantKind,
};

fn key(n: u8) -> SigningKey { SigningKey::from_bytes(&[n; 32]) }
fn id(n: u8) -> IdentityId { IdentityId::from_signing_key(&key(n)) }

fn actor(machine: bool, n: u8) -> Participant {
    if machine { Participant::machine(id(n), id(8)).unwrap() }
    else { Participant::human(id(n)) }
}
fn intent(a_machine: bool, b_machine: bool) -> InteractionIntent {
    InteractionIntent {
        initiator: actor(a_machine, 1),
        responder: actor(b_machine, 2),
        action: Action::DistributeCash,
        hawala_terms_digest: [3; 32],
        interaction_nonce: [4; 32],
        created_at: 100,
        expires_at: 160,
    }
}

#[test]
fn every_direction_and_all_three_families_are_first_class() {
    let pairs = [
        (false, false, InteractionMode::HumanToHuman),
        (false, true, InteractionMode::HumanToMachine),
        (true, false, InteractionMode::MachineToHuman),
        (true, true, InteractionMode::MachineToMachine),
    ];
    for (a, b, mode) in pairs {
        let proposal = intent(a, b);
        assert_eq!(proposal.mode(), mode);
        let initiator = sign_intent(&key(1), &proposal).unwrap();
        let responder = sign_intent(&key(2), &proposal).unwrap();
        let agreed = agree(proposal.clone(), initiator, responder, 110).unwrap();
        assert_eq!(agreed.intent(), &proposal);
        assert_eq!(agreed.verify(120), Ok(()));
        assert_eq!(agreed.verify(160), Err(Error::Expired));
        assert_eq!(agreed.verify(99), Err(Error::NotYetValid));
    }
    assert!(InteractionMode::HumanToMachine.is_human_machine());
    assert!(InteractionMode::MachineToHuman.is_human_machine());
    assert!(!InteractionMode::HumanToHuman.is_human_machine());
    assert!(!InteractionMode::MachineToMachine.is_human_machine());
}

#[test]
fn cannot_spoof_roles_machine_operators_or_amount_terms() {
    let original = intent(true, false);
    let a = sign_intent(&key(1), &original).unwrap();
    let b = sign_intent(&key(2), &original).unwrap();
    assert!(agree(original.clone(), a, b, 110).is_ok());

    let mut tampered = original.clone();
    tampered.responder = Participant::machine(id(2), id(8)).unwrap();
    assert_eq!(agree(tampered, a, b, 110), Err(Error::BadSignature));

    let mut tampered = original.clone();
    tampered.initiator.operator = Some(id(9));
    assert_eq!(agree(tampered, a, b, 110), Err(Error::BadSignature));

    let mut tampered = original.clone();
    tampered.hawala_terms_digest = [7; 32];
    assert_eq!(agree(tampered, a, b, 110), Err(Error::BadSignature));

    let mut tampered = original.clone();
    tampered.action = Action::AcceptCash;
    assert_eq!(agree(tampered, a, b, 110), Err(Error::BadSignature));

    let mut tampered = original.clone();
    tampered.interaction_nonce = [9; 32];
    assert_eq!(agree(tampered, a, b, 110), Err(Error::BadSignature));

    let mut tampered = original.clone();
    std::mem::swap(&mut tampered.initiator, &mut tampered.responder);
    assert_eq!(agree(tampered, a, b, 110), Err(Error::BadSignature));
}

#[test]
fn machine_operator_is_not_machine_identity_and_cannot_cosign() {
    let proposed = intent(false, true);
    assert_eq!(sign_intent(&key(8), &proposed), Err(Error::WrongSigner));
    assert_eq!(sign_intent(&key(9), &proposed), Err(Error::WrongSigner));
    assert_eq!(Participant::machine(id(8), id(8)), Err(Error::InvalidParticipant));

    let one = sign_intent(&key(1), &proposed).unwrap();
    assert_eq!(agree(proposed, one, one, 110), Err(Error::BadSignature));
}

#[test]
fn invalid_ids_nonce_and_window_are_fail_closed() {
    let mut x = intent(false, false);
    x.initiator = x.responder;
    assert_eq!(x.validate(), Err(Error::InvalidIntent));

    let mut x = intent(false, true);
    x.interaction_nonce = [0; 32];
    assert_eq!(x.validate(), Err(Error::InvalidIntent));

    let mut x = intent(false, true);
    x.hawala_terms_digest = [0; 32];
    assert_eq!(x.validate(), Err(Error::InvalidIntent));

    let mut x = intent(false, true);
    x.expires_at = 401;
    assert_eq!(x.validate(), Err(Error::InvalidIntent));

    let mut x = intent(false, true);
    x.initiator.kind = ParticipantKind::Machine; // missing owner
    assert_eq!(x.validate(), Err(Error::InvalidParticipant));

    let mut x = intent(true, false);
    x.initiator.kind = ParticipantKind::Human; // unexpected owner
    assert_eq!(x.validate(), Err(Error::InvalidParticipant));
}

#[test]
fn independently_signed_reconciliation_is_possible_in_all_modes() {
    for (a, b) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut p = intent(a, b);
        p.action = Action::ReconcileObligation;
        let a_sig = sign_intent(&key(1), &p).unwrap();
        let b_sig = sign_intent(&key(2), &p).unwrap();
        assert!(agree(p, a_sig, b_sig, 110).is_ok());
    }
}
