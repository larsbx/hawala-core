//! Signed authority descriptors and relying-party pinning (ADR 0005 gap G5).
//!
//! There is no global registry. A relying instance trusts the authority for a
//! scope only through a descriptor whose digest its operator pinned out of
//! band, and then only through successors that each epoch pre-committed to.
//! Discovery (gossip, ActivityPub, a biz-kit log) delivers bytes; it never
//! confers trust. Nothing is revoked or erased (ADR 0001): accepted history
//! stays evidence, and equivocation is retained as a `Conflict`, which means
//! the scope must be held.
//!
//! Encoding follows `docs/CROSS_AUTHORITY_V1.md`: fixed-width fields, `U64`
//! big-endian, `H` = SHA-256, no optional fields or trailing bytes, and
//! `digest = H(envelope)`.
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hawala_identity_core::IdentityId;
use sha2::{Digest, Sha256};

pub const DOMAIN: &[u8] = b"hawala-authority-descriptor/v1\0";
const BODY_LEN: usize = 32 * 3 + 8 + 32 * 5;
pub const ENVELOPE_LEN: usize = DOMAIN.len() + BODY_LEN + 64;

/// `next_key_digest` of a retired scope: no successor is ever accepted.
pub const RETIRED: [u8; 32] = [0; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Malformed,
    InvalidDescriptor,
    InvalidSignature,
    WrongSigner,
    AnchorMismatch,
    WrongNetwork,
    WrongScope,
    BeforeAnchor,
    Gap,
    BrokenChain,
    Retired,
    UncommittedSuccessor,
    /// Two valid descriptors for one scope and epoch. Retain both; hold the scope.
    Conflict {
        epoch: u64,
        pinned: [u8; 32],
        presented: [u8; 32],
    },
    WrongEpoch,
    WrongAuthority,
    CustodyProfileMismatch,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Commitment to a successor key: `H(key)`.
pub fn key_commitment(key: &IdentityId) -> [u8; 32] {
    Sha256::digest(key.as_bytes()).into()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Descriptor {
    pub network_id: [u8; 32],
    pub scope_id: [u8; 32],
    pub authority_key: IdentityId,
    pub authority_epoch: u64,
    /// `H` of the previous epoch's envelope; zero exactly for epoch 0.
    pub predecessor_digest: [u8; 32],
    /// `key_commitment` of the only key that may hold the next epoch, or `RETIRED`.
    pub next_key_digest: [u8; 32],
    pub custody_profile_digest: [u8; 32],
    pub fencing_policy_digest: [u8; 32],
    pub mandate_digest: [u8; 32],
}

impl Descriptor {
    fn validate(&self) -> Result<()> {
        let nonzero = [
            self.network_id,
            self.scope_id,
            self.custody_profile_digest,
            self.fencing_policy_digest,
            self.mandate_digest,
        ];
        if nonzero.contains(&[0; 32])
            || (self.authority_epoch == 0) != (self.predecessor_digest == [0; 32])
            || self.next_key_digest == key_commitment(&self.authority_key)
        {
            return Err(Error::InvalidDescriptor);
        }
        Ok(())
    }

    pub fn signing_input(&self) -> Vec<u8> {
        [
            DOMAIN,
            &self.network_id,
            &self.scope_id,
            self.authority_key.as_bytes(),
            &self.authority_epoch.to_be_bytes(),
            &self.predecessor_digest,
            &self.next_key_digest,
            &self.custody_profile_digest,
            &self.fencing_policy_digest,
            &self.mandate_digest,
        ]
        .concat()
    }

    fn decode_body(body: &[u8]) -> Result<Self> {
        let b32 = |at: usize| -> [u8; 32] { body[at..at + 32].try_into().unwrap() };
        let authority_key = IdentityId::from_bytes(b32(64)).map_err(|_| Error::Malformed)?;
        Ok(Self {
            network_id: b32(0),
            scope_id: b32(32),
            authority_key,
            authority_epoch: u64::from_be_bytes(body[96..104].try_into().unwrap()),
            predecessor_digest: b32(104),
            next_key_digest: b32(136),
            custody_profile_digest: b32(168),
            fencing_policy_digest: b32(200),
            mandate_digest: b32(232),
        })
    }
}

/// A descriptor with a valid signature by its own `authority_key`. Holding one
/// proves authenticity under that key, not that any relying party trusts it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedDescriptor {
    descriptor: Descriptor,
    signature: [u8; 64],
}

impl SignedDescriptor {
    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }

    pub fn encode(&self) -> Vec<u8> {
        [self.descriptor.signing_input().as_slice(), &self.signature].concat()
    }

    pub fn digest(&self) -> [u8; 32] {
        Sha256::digest(self.encode()).into()
    }

    /// Parse exactly one envelope and verify its self-signature.
    pub fn decode(envelope: &[u8]) -> Result<Self> {
        if envelope.len() != ENVELOPE_LEN || !envelope.starts_with(DOMAIN) {
            return Err(Error::Malformed);
        }
        let (input, signature) = envelope.split_at(ENVELOPE_LEN - 64);
        let descriptor = Descriptor::decode_body(&input[DOMAIN.len()..])?;
        descriptor.validate()?;
        let signature: [u8; 64] = signature.try_into().unwrap();
        VerifyingKey::from_bytes(descriptor.authority_key.as_bytes())
            .map_err(|_| Error::InvalidSignature)?
            .verify_strict(input, &Signature::from_bytes(&signature))
            .map_err(|_| Error::InvalidSignature)?;
        Ok(Self {
            descriptor,
            signature,
        })
    }
}

pub fn sign_descriptor(key: &SigningKey, descriptor: Descriptor) -> Result<SignedDescriptor> {
    descriptor.validate()?;
    if IdentityId::from_signing_key(key) != descriptor.authority_key {
        return Err(Error::WrongSigner);
    }
    let signature = key.sign(&descriptor.signing_input()).to_bytes();
    Ok(SignedDescriptor {
        descriptor,
        signature,
    })
}

/// The scope fields a cross-authority manifest leg names for one authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Leg {
    pub network_id: [u8; 32],
    pub authority_key: IdentityId,
    pub scope_id: [u8; 32],
    pub authority_epoch: u64,
    pub custody_profile_digest: [u8; 32],
}

/// A relying party's local trust in one scope: the operator-pinned anchor and
/// every descriptor accepted since. Immutable; `advance` returns a new pin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    anchor_epoch: u64,
    /// Envelope digests from the anchor epoch to the current epoch, in order.
    chain: Vec<[u8; 32]>,
    current: Descriptor,
}

impl Pin {
    /// Trust a descriptor only if it matches the digest the operator pinned.
    pub fn anchor(envelope: &[u8], pinned_digest: [u8; 32]) -> Result<Self> {
        let signed = SignedDescriptor::decode(envelope)?;
        if signed.digest() != pinned_digest {
            return Err(Error::AnchorMismatch);
        }
        Ok(Self {
            anchor_epoch: signed.descriptor.authority_epoch,
            chain: vec![pinned_digest],
            current: signed.descriptor,
        })
    }

    pub fn current(&self) -> &Descriptor {
        &self.current
    }

    fn current_epoch(&self) -> u64 {
        self.current.authority_epoch
    }

    /// Consider a descriptor from any source. Known epochs must match exactly
    /// (idempotent replay, else `Conflict`); the next epoch must link to the
    /// current one and be held by its pre-committed successor key.
    pub fn advance(&self, envelope: &[u8]) -> Result<Self> {
        let signed = SignedDescriptor::decode(envelope)?;
        let (d, presented) = (&signed.descriptor, signed.digest());
        if d.network_id != self.current.network_id {
            return Err(Error::WrongNetwork);
        }
        if d.scope_id != self.current.scope_id {
            return Err(Error::WrongScope);
        }
        let epoch = d.authority_epoch;
        if epoch < self.anchor_epoch {
            return Err(Error::BeforeAnchor);
        }
        if epoch <= self.current_epoch() {
            let pinned = self.chain[(epoch - self.anchor_epoch) as usize];
            return if pinned == presented {
                Ok(self.clone())
            } else {
                Err(Error::Conflict {
                    epoch,
                    pinned,
                    presented,
                })
            };
        }
        if epoch != self.current_epoch() + 1 {
            return Err(Error::Gap);
        }
        if d.predecessor_digest != *self.chain.last().unwrap() {
            return Err(Error::BrokenChain);
        }
        if self.current.next_key_digest == RETIRED {
            return Err(Error::Retired);
        }
        if key_commitment(&d.authority_key) != self.current.next_key_digest {
            return Err(Error::UncommittedSuccessor);
        }
        Ok(Self {
            anchor_epoch: self.anchor_epoch,
            chain: [self.chain.as_slice(), &[presented]].concat(),
            current: signed.descriptor,
        })
    }

    /// A manifest leg is admissible only for the exact current pinned epoch.
    /// Older epochs remain evidence for their own history, never fresh authority.
    pub fn admit_leg(&self, leg: &Leg) -> Result<()> {
        let c = &self.current;
        if leg.network_id != c.network_id {
            return Err(Error::WrongNetwork);
        }
        if leg.scope_id != c.scope_id {
            return Err(Error::WrongScope);
        }
        if leg.authority_epoch != c.authority_epoch {
            return Err(Error::WrongEpoch);
        }
        if leg.authority_key != c.authority_key {
            return Err(Error::WrongAuthority);
        }
        if leg.custody_profile_digest != c.custody_profile_digest {
            return Err(Error::CustodyProfileMismatch);
        }
        Ok(())
    }
}
