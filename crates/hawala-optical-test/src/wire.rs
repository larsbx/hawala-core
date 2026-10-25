use crate::{Error, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hawala_identity_core::IdentityId;

const MAGIC: &[u8; 8] = b"HWOPT001";
const DOMAIN: &[u8] = b"hawala/optical-TEST-evidence/v1\0";
pub const MAX_COMMIT_SECS: u64 = 300;
pub const MAX_SESSION_SECS: u64 = 120;
/// Fixed width; all integers unsigned big endian, no optional or trailing data.
pub const ENVELOPE_LEN: usize = 8 + 1 + 9 * 32 + 3 * 8 + 3 + 32 + 3 * 32 + 2 * 8 + 1 + 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    pub network: [u8; 32],
    pub transfer: [u8; 32],
    pub terms: [u8; 32],
    pub attempt: [u8; 32],
    pub holder: IdentityId,
    pub terminal: IdentityId,
    pub origin: IdentityId,
    pub disburser: IdentityId,
    /// Complete M6-C7 binding (including both custody requests and exact quote).
    pub coordinated_binding: [u8; 32],
    pub amount_minor: u64,
    pub fee_minor: u64,
    pub epoch: u64,
    pub currency: [u8; 3],
}
impl Scope {
    fn validate(&self) -> Result<()> {
        if [
            self.network,
            self.transfer,
            self.terms,
            self.attempt,
            self.coordinated_binding,
        ]
        .contains(&[0; 32])
            || self.holder == self.terminal
            || self.origin == self.disburser
            || self.amount_minor == 0
            || self.epoch == 0
            || !self.currency.iter().all(u8::is_ascii_uppercase)
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Precommit = 1,
    Challenge = 2,
    Response = 3,
    Observation = 4,
}

/// Private signature; constructors sign the exact canonical wire bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Envelope {
    pub kind: Kind,
    pub scope: Scope,
    pub signer: IdentityId,
    pub session: [u8; 32],
    pub nonce: [u8; 32],
    pub parent: [u8; 32],
    pub issued_at: u64,
    pub expires_at: u64,
    /// 0 for handshake, 1=reported delivery, 2=reported no delivery, 3=unknown.
    pub report: u8,
    signature: [u8; 64],
}
impl Envelope {
    /// TEST provisioning authority only. A signature is a synthetic policy
    /// assertion; use Simulator::export_test_optical after coordinated admission.
    pub fn precommit(
        key: &SigningKey,
        scope: Scope,
        issued_at: u64,
        expires_at: u64,
    ) -> Result<Self> {
        Self::signed(
            key,
            Self {
                kind: Kind::Precommit,
                scope,
                signer: IdentityId::from_signing_key(key),
                session: [0; 32],
                nonce: [0; 32],
                parent: [0; 32],
                issued_at,
                expires_at,
                report: 0,
                signature: [0; 64],
            },
        )
    }
    pub(crate) fn signed(key: &SigningKey, mut value: Self) -> Result<Self> {
        if value.signer != IdentityId::from_signing_key(key) {
            return Err(Error::Binding);
        }
        value.validate()?;
        value.signature = key.sign(&value.statement()).to_bytes();
        Ok(value)
    }
    pub(crate) fn derive(&self, kind: Kind, key: &SigningKey) -> Self {
        let mut e = self.clone();
        e.kind = kind;
        e.signer = IdentityId::from_signing_key(key);
        e.parent = self.digest();
        e.signature = [0; 64];
        e
    }
    fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        let max = if self.kind == Kind::Precommit {
            MAX_COMMIT_SECS
        } else {
            MAX_SESSION_SECS
        };
        if self.issued_at == 0
            || self.expires_at <= self.issued_at
            || self.expires_at - self.issued_at > max
        {
            return Err(Error::Time);
        }
        match self.kind {
            Kind::Precommit
                if self.session == [0; 32]
                    && self.nonce == [0; 32]
                    && self.parent == [0; 32]
                    && self.report == 0 => {}
            Kind::Challenge | Kind::Response | Kind::Observation
                if self.session != [0; 32]
                    && self.nonce != [0; 32]
                    && self.parent != [0; 32]
                    && self.session != self.nonce
                    && ((self.kind != Kind::Observation && self.report == 0)
                        || (self.kind == Kind::Observation && (1..=3).contains(&self.report))) => {}
            _ => return Err(Error::Invalid),
        }
        Ok(())
    }
    pub fn verify(&self, expected: IdentityId, now: u64) -> Result<()> {
        self.validate()?;
        if self.signer != expected {
            return Err(Error::Binding);
        }
        if now < self.issued_at || now >= self.expires_at {
            return Err(Error::Time);
        }
        self.verify_signature()
    }
    fn verify_signature(&self) -> Result<()> {
        VerifyingKey::from_bytes(self.signer.as_bytes())
            .map_err(|_| Error::Signature)?
            .verify_strict(&self.statement(), &Signature::from_bytes(&self.signature))
            .map_err(|_| Error::Signature)
    }
    fn statement(&self) -> Vec<u8> {
        let mut s = DOMAIN.to_vec();
        let b = self.encode();
        s.extend_from_slice(&b[..b.len() - 64]);
        s
    }
    pub fn digest(&self) -> [u8; 32] {
        *blake3::Hasher::new_derive_key("hawala/optical-TEST-envelope/v1")
            .update(&self.encode())
            .finalize()
            .as_bytes()
    }
    pub fn encode(&self) -> Vec<u8> {
        let s = &self.scope;
        let mut b = MAGIC.to_vec();
        b.push(self.kind as u8);
        for f in [
            &s.network,
            &s.transfer,
            &s.terms,
            &s.attempt,
            s.holder.as_bytes(),
            s.terminal.as_bytes(),
            s.origin.as_bytes(),
            s.disburser.as_bytes(),
            &s.coordinated_binding,
        ] {
            b.extend_from_slice(f);
        }
        for n in [s.amount_minor, s.fee_minor, s.epoch] {
            b.extend_from_slice(&n.to_be_bytes());
        }
        b.extend_from_slice(&s.currency);
        b.extend_from_slice(self.signer.as_bytes());
        for f in [&self.session, &self.nonce, &self.parent] {
            b.extend_from_slice(f);
        }
        for n in [self.issued_at, self.expires_at] {
            b.extend_from_slice(&n.to_be_bytes());
        }
        b.push(self.report);
        b.extend_from_slice(&self.signature);
        b
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != ENVELOPE_LEN || &bytes[..8] != MAGIC {
            return Err(Error::Invalid);
        }
        let mut r = Reader { bytes, index: 9 };
        let kind = match bytes[8] {
            1 => Kind::Precommit,
            2 => Kind::Challenge,
            3 => Kind::Response,
            4 => Kind::Observation,
            _ => return Err(Error::Invalid),
        };
        let scope = Scope {
            network: r.take()?,
            transfer: r.take()?,
            terms: r.take()?,
            attempt: r.take()?,
            holder: r.id()?,
            terminal: r.id()?,
            origin: r.id()?,
            disburser: r.id()?,
            coordinated_binding: r.take()?,
            amount_minor: r.number()?,
            fee_minor: r.number()?,
            epoch: r.number()?,
            currency: r.take()?,
        };
        let e = Self {
            kind,
            scope,
            signer: r.id()?,
            session: r.take()?,
            nonce: r.take()?,
            parent: r.take()?,
            issued_at: r.number()?,
            expires_at: r.number()?,
            report: r.take::<1>()?[0],
            signature: r.take()?,
        };
        e.validate()?;
        e.verify_signature()?;
        Ok(e)
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    index: usize,
}
impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        let b = self
            .bytes
            .get(self.index..self.index + N)
            .ok_or(Error::Invalid)?;
        self.index += N;
        b.try_into().map_err(|_| Error::Invalid)
    }
    fn id(&mut self) -> Result<IdentityId> {
        IdentityId::from_bytes(self.take()?).map_err(|_| Error::Invalid)
    }
    fn number(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take()?))
    }
}

/// Trusted holder authenticator: QR-scanned identities and pinned TEST authority
/// are independent inputs. Caller reviews ALL scope fields on a trusted display.
/// Does not assert proximity: an unchanged live relay is indistinguishable here.
pub fn respond(
    key: &SigningKey,
    scanned_terminal: IdentityId,
    authority: IdentityId,
    commit: &Envelope,
    challenge: &Envelope,
    now: u64,
) -> Result<Envelope> {
    commit.verify(authority, now)?;
    challenge.verify(scanned_terminal, now)?;
    if commit.kind != Kind::Precommit
        || challenge.kind != Kind::Challenge
        || commit.scope != challenge.scope
        || challenge.parent != commit.digest()
        || challenge.scope.holder != IdentityId::from_signing_key(key)
        || challenge.scope.terminal != scanned_terminal
        || challenge.issued_at < commit.issued_at
        || challenge.expires_at > commit.expires_at
    {
        return Err(Error::Binding);
    }
    Envelope::signed(key, challenge.derive(Kind::Response, key))
}
/// Terminal-signed claim only. Even contradictory correctly signed claims are
/// held as evidence, not interpreted as physical cash delivery.
pub fn observation(key: &SigningKey, response: &Envelope, report: u8) -> Result<Envelope> {
    if response.kind != Kind::Response
        || IdentityId::from_signing_key(key) != response.scope.terminal
    {
        return Err(Error::Binding);
    }
    response.verify_signature()?;
    let mut e = response.derive(Kind::Observation, key);
    e.report = report;
    Envelope::signed(key, e)
}
