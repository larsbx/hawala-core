//! Per-role disclosure for cross-authority payment records (ADR 0005 gap G7).
//!
//! A `DisclosurePolicy` grants each role a set of field classes. Its digest is
//! the manifest's `permitted_disclosure_policy_digest` (`CROSS_AUTHORITY_V1`).
//! `validate` enforces the invariants every policy must keep; `project` strips
//! from a record everything a role may not see, failing closed on unknown
//! fields. See `docs/DISCLOSURE.md`.
use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

pub const DOMAIN: &[u8] = b"hawala-disclosure-policy/v1\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Malformed,
    RoutingPeerSeesContent,
    ObserverSeesContent,
    EvidenceWithoutManifest,
    SignerBlind(Role),
    UnknownField,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    /// Opaque network, business and attempt identifiers.
    Correlation,
    /// The typed payment state (`Proposed` … `Settled`, `Held`, `Aborted`).
    State,
    /// Keys and scopes of payees, authorities and the coordinator.
    Parties,
    Amounts,
    /// Allocation, obligation, funding lot and destination digests.
    Destination,
    /// Policy, mandate, custody-profile digests and deadlines.
    Policy,
    /// Accepted dual-signed terms and participant consent objects.
    Terms,
    /// Signed evidence envelopes. Each one signs H(manifest bytes).
    Evidence,
}

impl Class {
    pub const ALL: [Class; 8] = [
        Class::Correlation,
        Class::State,
        Class::Parties,
        Class::Amounts,
        Class::Destination,
        Class::Policy,
        Class::Terms,
        Class::Evidence,
    ];

    /// Classes needed to recompute the manifest digest.
    const MANIFEST: [Class; 5] = [
        Class::Correlation,
        Class::Parties,
        Class::Amounts,
        Class::Destination,
        Class::Policy,
    ];

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// A consenting participant device (payer or payee).
    Participant,
    /// The merchant or recipient business node of a leg's payee.
    Payee,
    Coordinator,
    Authority,
    /// A leg's execution observer or settlement attestor.
    LegAgent,
    /// A non-party instance that only displays progress (for example a
    /// delivery node). It can verify nothing and so may advance nothing.
    StatusObserver,
    /// A peer that forwards end-to-end encrypted messages.
    RoutingPeer,
}

impl Role {
    pub const ALL: [Role; 7] = [
        Role::Participant,
        Role::Payee,
        Role::Coordinator,
        Role::Authority,
        Role::LegAgent,
        Role::StatusObserver,
        Role::RoutingPeer,
    ];

    fn signs(self) -> bool {
        matches!(
            self,
            Role::Participant | Role::Coordinator | Role::Authority
        )
    }
}

pub struct ManifestField {
    pub name: &'static str,
    pub per_leg: bool,
    pub class: Class,
}

const fn field(name: &'static str, per_leg: bool, class: Class) -> ManifestField {
    ManifestField {
        name,
        per_leg,
        class,
    }
}

/// Every `CROSS_AUTHORITY_V1` manifest field, in specification order.
pub const MANIFEST_FIELDS: [ManifestField; 25] = [
    field("network_id", false, Class::Correlation),
    field("business_id", false, Class::Correlation),
    field("business_revision", false, Class::Correlation),
    field("attempt_id", false, Class::Correlation),
    field("accepted_terms_digest", false, Class::Policy),
    field("participant_consent_digest", false, Class::Policy),
    field("permitted_disclosure_policy_digest", false, Class::Policy),
    field("finality_policy_digest", false, Class::Policy),
    field("recovery_mandate_digest", false, Class::Policy),
    field("coordinator_key", false, Class::Parties),
    field("coordinator_epoch", false, Class::Parties),
    field("participant_count", false, Class::Policy),
    field("authority_key", true, Class::Parties),
    field("scope_id", true, Class::Parties),
    field("authority_epoch", true, Class::Parties),
    field("allocation_id", true, Class::Destination),
    field("obligation_digest", true, Class::Destination),
    field("funding_lot_id", true, Class::Destination),
    field("payee_key", true, Class::Parties),
    field("currency", true, Class::Amounts),
    field("amount", true, Class::Amounts),
    field("fee", true, Class::Amounts),
    field("destination_digest", true, Class::Destination),
    field("custody_profile_digest", true, Class::Policy),
    field("latest_prepare_time", false, Class::Policy),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisclosurePolicy {
    grants: BTreeMap<Role, BTreeSet<Class>>,
}

impl DisclosurePolicy {
    /// v1 profile. In the v1 manifest format every verifier needs the whole
    /// manifest, so verifying roles get everything; non-verifying roles get
    /// the minimum.
    pub fn v1_minimal() -> Self {
        let all: BTreeSet<Class> = Class::ALL.into_iter().collect();
        let grants = Role::ALL
            .into_iter()
            .map(|role| {
                let classes = match role {
                    Role::StatusObserver => BTreeSet::from([Class::Correlation, Class::State]),
                    Role::RoutingPeer => BTreeSet::new(),
                    _ => all.clone(),
                };
                (role, classes)
            })
            .collect();
        Self { grants }
    }

    /// A copy with `role`'s grant replaced. The result may be invalid.
    pub fn with_grant(&self, role: Role, classes: BTreeSet<Class>) -> Self {
        let mut grants = self.grants.clone();
        grants.insert(role, classes);
        Self { grants }
    }

    fn granted(&self, role: Role) -> &BTreeSet<Class> {
        &self.grants[&role]
    }

    pub fn permits(&self, role: Role, class: Class) -> bool {
        self.granted(role).contains(&class)
    }

    pub fn validate(&self) -> Result<()> {
        if !self.granted(Role::RoutingPeer).is_empty() {
            return Err(Error::RoutingPeerSeesContent);
        }
        if !self
            .granted(Role::StatusObserver)
            .is_subset(&BTreeSet::from([Class::Correlation, Class::State]))
        {
            return Err(Error::ObserverSeesContent);
        }
        for role in Role::ALL {
            let g = self.granted(role);
            if g.contains(&Class::Evidence) && !Class::MANIFEST.iter().all(|c| g.contains(c)) {
                return Err(Error::EvidenceWithoutManifest);
            }
            if role.signs() && g.len() != Class::ALL.len() {
                return Err(Error::SignerBlind(role));
            }
        }
        Ok(())
    }

    /// Domain, then one class bitmask byte per role in `Role::ALL` order.
    pub fn bytes(&self) -> Vec<u8> {
        let masks = Role::ALL.map(|role| self.granted(role).iter().fold(0u8, |m, c| m | c.bit()));
        [DOMAIN, &masks].concat()
    }

    pub fn digest(&self) -> [u8; 32] {
        Sha256::digest(self.bytes()).into()
    }

    /// Strictly decode and validate a policy received by digest reference.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let masks = bytes.strip_prefix(DOMAIN).ok_or(Error::Malformed)?;
        if masks.len() != Role::ALL.len() {
            return Err(Error::Malformed);
        }
        let grants = Role::ALL
            .into_iter()
            .zip(masks)
            .map(|(role, m)| {
                (
                    role,
                    Class::ALL
                        .into_iter()
                        .filter(|c| m & c.bit() != 0)
                        .collect(),
                )
            })
            .collect();
        let policy = Self { grants };
        policy.validate()?;
        Ok(policy)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// One manifest field; `leg` is set exactly for per-leg fields.
    Manifest {
        name: &'static str,
        leg: Option<u8>,
        bytes: Vec<u8>,
    },
    State(Vec<u8>),
    Terms(Vec<u8>),
    Evidence(Vec<u8>),
}

fn class_of(item: &Item) -> Result<Class> {
    match item {
        Item::Manifest { name, leg, .. } => MANIFEST_FIELDS
            .iter()
            .find(|f| f.name == *name && f.per_leg == leg.is_some())
            .map(|f| f.class)
            .ok_or(Error::UnknownField),
        Item::State(_) => Ok(Class::State),
        Item::Terms(_) => Ok(Class::Terms),
        Item::Evidence(_) => Ok(Class::Evidence),
    }
}

/// The part of `record` that `role` may see under a valid `policy`. Any item
/// that cannot be classified rejects the whole projection.
pub fn project(policy: &DisclosurePolicy, role: Role, record: &[Item]) -> Result<Vec<Item>> {
    policy.validate()?;
    let classified = record
        .iter()
        .map(|i| class_of(i).map(|c| (c, i)))
        .collect::<Result<Vec<_>>>()?;
    Ok(classified
        .into_iter()
        .filter(|(c, _)| policy.permits(role, *c))
        .map(|(_, i)| i.clone())
        .collect())
}
