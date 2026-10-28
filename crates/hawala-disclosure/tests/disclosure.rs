//! ADR 0005 gap G7: which roles may see which parts of a cross-authority
//! payment record. Routing peers and status observers never see amounts or
//! counterparties; any role that verifies evidence must see the whole manifest.
use std::collections::BTreeSet;

use hawala_disclosure::*;

const SENSITIVE: [&str; 7] = [
    "payee_key",
    "authority_key",
    "coordinator_key",
    "amount",
    "fee",
    "currency",
    "destination_digest",
];

/// Field names of the manifest block in `docs/CROSS_AUTHORITY_V1.md`, in order.
fn spec_manifest_fields() -> Vec<String> {
    let doc = include_str!("../../../docs/CROSS_AUTHORITY_V1.md");
    let start = doc
        .find("\"hawala-cross-authority-manifest/v1\\0\"")
        .expect("manifest block");
    let block = &doc[start..start + doc[start..].find("```").unwrap()];
    block
        .split_whitespace()
        .filter_map(|t| {
            t.split_once(':')
                .map(|(name, _)| name.trim_start_matches('[').to_owned())
        })
        .filter(|name| !name.is_empty())
        .collect()
}

#[test]
fn classifies_exactly_the_specified_manifest_fields_in_order() {
    let ours: Vec<String> = MANIFEST_FIELDS.iter().map(|f| f.name.to_owned()).collect();
    assert_eq!(ours, spec_manifest_fields());
}

#[test]
fn minimal_profile_is_valid_and_has_a_stable_hawala_domain_digest() {
    let p = DisclosurePolicy::v1_minimal();
    assert_eq!(p.validate(), Ok(()));
    assert!(p.bytes().starts_with(b"hawala-disclosure-policy/v1\0"));
    assert_eq!(DisclosurePolicy::decode(&p.bytes()), Ok(p.clone()));
    assert_eq!(
        hex::encode(p.digest()),
        "94248705002187a0c7b05b56ef4615373bd49ec90c136e2a9a9e325a15186ecf",
        "golden digest of the v1 minimal profile"
    );
}

fn with(role: Role, classes: &[Class]) -> DisclosurePolicy {
    DisclosurePolicy::v1_minimal().with_grant(role, classes.iter().copied().collect())
}

#[test]
fn routing_peers_may_see_nothing() {
    for class in Class::ALL {
        assert_eq!(
            with(Role::RoutingPeer, &[class]).validate(),
            Err(Error::RoutingPeerSeesContent)
        );
    }
}

#[test]
fn status_observers_see_only_correlation_and_state() {
    assert_eq!(
        with(Role::StatusObserver, &[Class::Correlation, Class::State]).validate(),
        Ok(())
    );
    for class in [
        Class::Parties,
        Class::Amounts,
        Class::Destination,
        Class::Policy,
        Class::Terms,
        Class::Evidence,
    ] {
        assert_eq!(
            with(
                Role::StatusObserver,
                &[Class::Correlation, Class::State, class]
            )
            .validate(),
            Err(Error::ObserverSeesContent)
        );
    }
}

#[test]
fn whoever_receives_evidence_receives_the_whole_manifest() {
    // Evidence signs H(manifest bytes); a partial manifest cannot be verified.
    let partial = [
        Class::Correlation,
        Class::State,
        Class::Parties,
        Class::Policy,
        Class::Evidence,
    ];
    assert_eq!(
        with(Role::LegAgent, &partial).validate(),
        Err(Error::EvidenceWithoutManifest)
    );
}

#[test]
fn signers_see_everything_they_sign() {
    for role in [Role::Participant, Role::Coordinator, Role::Authority] {
        let mut classes: BTreeSet<Class> = Class::ALL.into_iter().collect();
        classes.remove(&Class::Terms);
        let p = DisclosurePolicy::v1_minimal().with_grant(role, classes);
        assert_eq!(p.validate(), Err(Error::SignerBlind(role)));
    }
}

#[test]
fn decoding_is_strict() {
    let b = DisclosurePolicy::v1_minimal().bytes();
    assert_eq!(
        DisclosurePolicy::decode(&b[..b.len() - 1]),
        Err(Error::Malformed)
    );
    assert_eq!(
        DisclosurePolicy::decode(&[b.as_slice(), &[0]].concat()),
        Err(Error::Malformed)
    );
    let mut wrong_domain = b.clone();
    wrong_domain[0] ^= 1;
    assert_eq!(
        DisclosurePolicy::decode(&wrong_domain),
        Err(Error::Malformed)
    );
    // Eight classes use every bit of a mask byte, so no byte value is unassigned.
    assert_eq!(Class::ALL.len(), 8);
    let leaky = with(Role::RoutingPeer, &[Class::Amounts]).bytes();
    assert_eq!(
        DisclosurePolicy::decode(&leaky),
        Err(Error::RoutingPeerSeesContent)
    );
}

fn record() -> Vec<Item> {
    let mut items: Vec<Item> = MANIFEST_FIELDS
        .iter()
        .flat_map(|f| {
            let legs: Vec<Option<u8>> = if f.per_leg {
                vec![Some(0), Some(1)]
            } else {
                vec![None]
            };
            legs.into_iter().map(move |leg| Item::Manifest {
                name: f.name,
                leg,
                bytes: vec![leg.unwrap_or(9); 4],
            })
        })
        .collect();
    items.push(Item::State(b"Committed(intent)".to_vec()));
    items.push(Item::Terms(b"dual-signed terms".to_vec()));
    items.push(Item::Evidence(b"CommitAck envelope".to_vec()));
    items
}

fn names(view: &[Item]) -> BTreeSet<&'static str> {
    view.iter()
        .filter_map(|i| match i {
            Item::Manifest { name, .. } => Some(*name),
            _ => None,
        })
        .collect()
}

#[test]
fn routing_peers_project_to_nothing() {
    let p = DisclosurePolicy::v1_minimal();
    assert_eq!(project(&p, Role::RoutingPeer, &record()), Ok(vec![]));
}

#[test]
fn status_observers_get_correlation_and_state_without_counterparties_or_amounts() {
    let view = project(
        &DisclosurePolicy::v1_minimal(),
        Role::StatusObserver,
        &record(),
    )
    .unwrap();
    let seen = names(&view);
    assert_eq!(
        seen,
        BTreeSet::from([
            "network_id",
            "business_id",
            "business_revision",
            "attempt_id"
        ])
    );
    assert!(SENSITIVE.iter().all(|s| !seen.contains(s)));
    assert!(view.contains(&Item::State(b"Committed(intent)".to_vec())));
    assert!(!view
        .iter()
        .any(|i| matches!(i, Item::Terms(_) | Item::Evidence(_))));
}

#[test]
fn verifying_roles_get_the_complete_record() {
    for role in [
        Role::Participant,
        Role::Payee,
        Role::Coordinator,
        Role::Authority,
        Role::LegAgent,
    ] {
        assert_eq!(
            project(&DisclosurePolicy::v1_minimal(), role, &record()),
            Ok(record()),
            "{role:?}"
        );
    }
}

#[test]
fn unknown_fields_fail_closed() {
    let items = vec![Item::Manifest {
        name: "customer_name",
        leg: None,
        bytes: vec![1],
    }];
    assert_eq!(
        project(&DisclosurePolicy::v1_minimal(), Role::Payee, &items),
        Err(Error::UnknownField)
    );
    let leg_on_global = vec![Item::Manifest {
        name: "business_id",
        leg: Some(0),
        bytes: vec![1],
    }];
    assert_eq!(
        project(&DisclosurePolicy::v1_minimal(), Role::Payee, &leg_on_global),
        Err(Error::UnknownField)
    );
}

#[test]
fn projection_refuses_an_invalid_policy() {
    let leaky = with(Role::RoutingPeer, &[Class::Amounts]);
    assert_eq!(
        project(&leaky, Role::Payee, &record()),
        Err(Error::RoutingPeerSeesContent)
    );
}
