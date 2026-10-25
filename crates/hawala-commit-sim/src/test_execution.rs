//! A durable synthetic execution rail, never bank/custody or ATM control.
//! One scoped mock effect OR a permanent no-effect fence. Signatures attest
//! this TEST store under its persistence assumptions, not physical cash.
use super::*;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hawala_identity_core::IdentityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TestExecutionScope {
    pub(super) transfer: [u8; 32],
    pub(super) attempt: [u8; 32],
    pub(super) binding: [u8; 32],
    pub(super) peer: IdentityId,
    pub(super) financial_epoch: u64,
    pub(super) currency: [u8; 3],
    pub(super) amount: u64,
}
impl TestExecutionScope {
    pub fn transfer_id(self) -> [u8; 32] {
        self.transfer
    }
    pub fn attempt_id(self) -> [u8; 32] {
        self.attempt
    }
    pub fn operation_digest(self) -> [u8; 32] {
        self.binding
    }
    pub fn peer(self) -> IdentityId {
        self.peer
    }
    pub fn financial_epoch(self) -> u64 {
        self.financial_epoch
    }
    pub fn currency(self) -> [u8; 3] {
        self.currency
    }
    pub fn amount_minor(self) -> u64 {
        self.amount
    }
    fn validate(self) -> Result<()> {
        if [self.transfer, self.attempt, self.binding].contains(&[0; 32])
            || self.financial_epoch == 0
            || self.amount == 0
            || self.amount > i64::MAX as u64
            || !self.currency.iter().all(u8::is_ascii_uppercase)
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
    pub(super) fn bytes(self) -> Vec<u8> {
        let mut b = Vec::new();
        for p in [
            self.transfer,
            self.attempt,
            self.binding,
            *self.peer.as_bytes(),
        ] {
            b.extend_from_slice(&p);
        }
        b.extend_from_slice(&self.financial_epoch.to_be_bytes());
        b.extend_from_slice(&self.currency);
        b.extend_from_slice(&self.amount.to_be_bytes());
        b
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TestRailPolicy {
    pub network_id: [u8; 32],
    pub rail_id: [u8; 32],
    pub controller: IdentityId,
    pub epoch: u64,
}
impl TestRailPolicy {
    pub(super) fn validate(self) -> Result<()> {
        if self.network_id == [0; 32] || self.rail_id == [0; 32] || self.epoch == 0 {
            return Err(Error::Invalid);
        }
        VerifyingKey::from_bytes(self.controller.as_bytes()).map_err(|_| Error::Invalid)?;
        Ok(())
    }
    pub(super) fn bytes(self) -> Vec<u8> {
        let mut b = Vec::new();
        for p in [self.network_id, self.rail_id, *self.controller.as_bytes()] {
            b.extend_from_slice(&p);
        }
        b.extend_from_slice(&self.epoch.to_be_bytes());
        b
    }
    pub(super) fn decode(b: &[u8]) -> Result<Self> {
        if b.len() != 104 {
            return Err(Error::Invalid);
        }
        let p = Self {
            network_id: b[..32].try_into().unwrap(),
            rail_id: b[32..64].try_into().unwrap(),
            controller: IdentityId::from_bytes(b[64..96].try_into().unwrap())
                .map_err(|_| Error::Invalid)?,
            epoch: u64::from_be_bytes(b[96..104].try_into().unwrap()),
        };
        p.validate()?;
        Ok(p)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TestTerminalOutcome {
    MockEffect = 1,
    FencedNoEffect = 2,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedTestTerminal {
    policy: TestRailPolicy,
    scope: TestExecutionScope,
    outcome: TestTerminalOutcome,
    signature: [u8; 64],
}
impl SignedTestTerminal {
    pub const FRAME_LEN: usize = 340;
    pub fn scope(&self) -> TestExecutionScope {
        self.scope
    }
    pub fn policy(&self) -> TestRailPolicy {
        self.policy
    }
    pub fn outcome(&self) -> TestTerminalOutcome {
        self.outcome
    }
    fn body(&self) -> Vec<u8> {
        let mut b = b"HWTTERM1".to_vec();
        b.extend(self.policy.bytes());
        b.extend(self.scope.bytes());
        b.push(self.outcome as u8);
        b.extend_from_slice(&2u64.to_be_bytes());
        b.extend_from_slice(
            &(if self.outcome == TestTerminalOutcome::MockEffect {
                1u64
            } else {
                0u64
            })
            .to_be_bytes(),
        );
        b
    }
    fn statement(&self) -> Vec<u8> {
        let mut b = b"hawala/TEST-terminal-execution/v1\0".to_vec();
        b.extend(self.body());
        b
    }
    pub fn to_frame(&self) -> Vec<u8> {
        let mut b = self.body();
        b.extend_from_slice(&self.signature);
        b
    }
    /// Fixed-width parsing does not authenticate the signer or establish funds.
    pub fn from_frame(b: &[u8]) -> Result<Self> {
        if b.len() != Self::FRAME_LEN || &b[..8] != b"HWTTERM1" {
            return Err(Error::Invalid);
        }
        let policy = TestRailPolicy::decode(&b[8..112])?;
        let s = &b[112..259];
        let scope = TestExecutionScope {
            transfer: s[..32].try_into().unwrap(),
            attempt: s[32..64].try_into().unwrap(),
            binding: s[64..96].try_into().unwrap(),
            peer: IdentityId::from_bytes(s[96..128].try_into().unwrap())
                .map_err(|_| Error::Invalid)?,
            financial_epoch: u64::from_be_bytes(s[128..136].try_into().unwrap()),
            currency: s[136..139].try_into().unwrap(),
            amount: u64::from_be_bytes(s[139..147].try_into().unwrap()),
        };
        scope.validate()?;
        let outcome = match b[259] {
            1 => TestTerminalOutcome::MockEffect,
            2 => TestTerminalOutcome::FencedNoEffect,
            _ => return Err(Error::Invalid),
        };
        let receipt = Self {
            policy,
            scope,
            outcome,
            signature: b[276..].try_into().unwrap(),
        };
        if receipt.body() != b[..276] {
            return Err(Error::Invalid);
        }
        Ok(receipt)
    }
    pub fn digest(&self) -> [u8; 32] {
        *blake3::hash(&self.to_frame()).as_bytes()
    }
}
/// Opaque verification result. It is only TEST evidence, never a cash permit.
pub struct VerifiedTestTerminal {
    pub(super) digest: [u8; 32],
    pub(super) scope: TestExecutionScope,
    pub(super) policy: TestRailPolicy,
}
pub enum TestTerminalCheck {
    Rejected,
    Held,
    Verified(VerifiedTestTerminal),
}
pub trait TestTerminalAuthority {
    fn verify(
        &self,
        receipt: &SignedTestTerminal,
        scope: TestExecutionScope,
        pinned: TestRailPolicy,
    ) -> TestTerminalCheck;
}
pub struct SignedTestRailAuthority;
impl TestTerminalAuthority for SignedTestRailAuthority {
    fn verify(
        &self,
        r: &SignedTestTerminal,
        scope: TestExecutionScope,
        pinned: TestRailPolicy,
    ) -> TestTerminalCheck {
        if r.scope != scope || r.policy != pinned || pinned.validate().is_err() {
            return TestTerminalCheck::Rejected;
        }
        let Ok(key) = VerifyingKey::from_bytes(pinned.controller.as_bytes()) else {
            return TestTerminalCheck::Rejected;
        };
        if key
            .verify_strict(&r.statement(), &Signature::from_bytes(&r.signature))
            .is_err()
        {
            return TestTerminalCheck::Rejected;
        }
        TestTerminalCheck::Verified(VerifiedTestTerminal {
            digest: r.digest(),
            scope,
            policy: pinned,
        })
    }
}
pub struct UnsupportedRealTerminalAuthority;
impl TestTerminalAuthority for UnsupportedRealTerminalAuthority {
    fn verify(
        &self,
        _: &SignedTestTerminal,
        _: TestExecutionScope,
        _: TestRailPolicy,
    ) -> TestTerminalCheck {
        TestTerminalCheck::Held
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestRailDecision {
    Held,
    Terminal(SignedTestTerminal),
}
/// One TEST operation in one durable store; no controller migration or reset.
pub struct TestExecutionRail {
    db: Mutex<Connection>,
    policy: TestRailPolicy,
    scope: TestExecutionScope,
    key: SigningKey,
}
impl TestExecutionRail {
    fn connection(path: &Path) -> Result<Connection> {
        validate_test_path(path)?;
        // No CREATE/URI: only `create` may enroll a new store.
        let db = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
        if db.query_row::<String, _, _>("PRAGMA journal_mode", [], |r| r.get(0))? != "wal" {
            return Err(Error::Invalid);
        }
        Ok(db)
    }
    fn meta(policy: TestRailPolicy, scope: TestExecutionScope) -> Vec<u8> {
        let mut b = b"hawala/TEST-execution-store/v1\0".to_vec();
        b.extend(policy.bytes());
        b.extend(scope.bytes());
        b
    }
    fn check_key(
        policy: TestRailPolicy,
        scope: TestExecutionScope,
        key: &SigningKey,
    ) -> Result<()> {
        policy.validate()?;
        scope.validate()?;
        if IdentityId::from_signing_key(key) != policy.controller {
            return Err(Error::Invalid);
        }
        Ok(())
    }
    pub fn create(
        path: impl AsRef<Path>,
        policy: TestRailPolicy,
        scope: TestExecutionScope,
        key: SigningKey,
    ) -> Result<Self> {
        Self::check_key(policy, scope, &key)?;
        let path = path.as_ref();
        validate_test_path(path)?;
        // Exclusive creation prevents accidental reinitialization. A missing
        // store on reopen fails; anti-rollback/host tampering remains outside.
        let _file = OpenOptions::new().write(true).create_new(true).open(path)?;
        let mut db = Self::connection(path)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch("CREATE TABLE rail_state(id INTEGER PRIMARY KEY CHECK(id=1),meta BLOB NOT NULL,
            version INTEGER NOT NULL CHECK(version IN (1,2)),outcome INTEGER NOT NULL CHECK(outcome IN (0,1,2)),
            effects INTEGER NOT NULL CHECK(effects IN (0,1)),frame BLOB,
            CHECK((version=1 AND outcome=0 AND effects=0 AND frame IS NULL)
                OR (version=2 AND outcome=1 AND effects=1 AND frame IS NOT NULL AND length(frame)=340)
                OR (version=2 AND outcome=2 AND effects=0 AND frame IS NOT NULL AND length(frame)=340)));
            CREATE TABLE rail_commands(request BLOB PRIMARY KEY CHECK(length(request)=32),
                command BLOB NOT NULL,frame BLOB CHECK(frame IS NULL OR length(frame)=340)) WITHOUT ROWID;")?;
        tx.execute(
            "INSERT INTO rail_state VALUES(1,?1,1,0,0,NULL)",
            params![Self::meta(policy, scope)],
        )?;
        tx.commit()?;
        Ok(Self {
            db: Mutex::new(db),
            policy,
            scope,
            key,
        })
    }
    pub fn open(
        path: impl AsRef<Path>,
        policy: TestRailPolicy,
        scope: TestExecutionScope,
        key: SigningKey,
    ) -> Result<Self> {
        Self::check_key(policy, scope, &key)?;
        let db = Self::connection(path.as_ref())?;
        let meta: Vec<u8> =
            db.query_row("SELECT meta FROM rail_state WHERE id=1", [], |r| r.get(0))?;
        if meta != Self::meta(policy, scope) {
            return Err(Error::Conflict);
        }
        Ok(Self {
            db: Mutex::new(db),
            policy,
            scope,
            key,
        })
    }
    fn verified_frame(&self, b: &[u8]) -> Result<SignedTestTerminal> {
        let r = SignedTestTerminal::from_frame(b)?;
        if !matches!(
            SignedTestRailAuthority.verify(&r, self.scope, self.policy),
            TestTerminalCheck::Verified(_)
        ) {
            return Err(Error::Invalid);
        }
        Ok(r)
    }
    pub fn terminal_receipt(&self) -> Result<Option<SignedTestTerminal>> {
        let db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        let frame: Option<Vec<u8>> =
            db.query_row("SELECT frame FROM rail_state WHERE id=1", [], |r| r.get(0))?;
        frame.map(|b| self.verified_frame(&b)).transpose()
    }
    pub fn mock_effect_count(&self) -> Result<u64> {
        let db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        Ok(
            db.query_row("SELECT effects FROM rail_state WHERE id=1", [], |r| {
                r.get(0)
            })?,
        )
    }
    pub fn execute(
        &self,
        dispatch: &SyntheticDispatch,
        request: [u8; 32],
        expected_version: u64,
    ) -> Result<TestRailDecision> {
        if dispatch.test_execution_scope() != Some(self.scope)
            || dispatch.test_terminal_policy() != Some(self.policy)
        {
            return Err(Error::Invalid);
        }
        self.transition(request, expected_version, TestTerminalOutcome::MockEffect)
    }
    pub fn fence_no_effect(
        &self,
        request: [u8; 32],
        expected_version: u64,
    ) -> Result<TestRailDecision> {
        self.transition(
            request,
            expected_version,
            TestTerminalOutcome::FencedNoEffect,
        )
    }
    fn transition(
        &self,
        request: [u8; 32],
        expected_version: u64,
        outcome: TestTerminalOutcome,
    ) -> Result<TestRailDecision> {
        if request == [0; 32] {
            return Err(Error::Invalid);
        }
        let mut command = b"hawala/TEST-execution-command/v1\0".to_vec();
        command.extend(Self::meta(self.policy, self.scope));
        command.push(outcome as u8);
        command.extend_from_slice(&expected_version.to_be_bytes());
        let mut db = self.db.lock().map_err(|_| Error::StoragePoisoned)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old: Option<(Vec<u8>, Option<Vec<u8>>)> = tx
            .query_row(
                "SELECT command,frame FROM rail_commands WHERE request=?1",
                params![request.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((prior, frame)) = old {
            if prior != command {
                return Err(Error::Conflict);
            }
            return Ok(match frame {
                Some(b) => TestRailDecision::Terminal(self.verified_frame(&b)?),
                None => TestRailDecision::Held,
            });
        }
        let (version, mut frame): (u64, Option<Vec<u8>>) =
            tx.query_row("SELECT version,frame FROM rail_state WHERE id=1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        if frame.is_none() && expected_version == version {
            let mut r = SignedTestTerminal {
                policy: self.policy,
                scope: self.scope,
                outcome,
                signature: [0; 64],
            };
            r.signature = self.key.sign(&r.statement()).to_bytes();
            frame = Some(r.to_frame());
            if tx.execute(
                "UPDATE rail_state SET version=2,outcome=?1,effects=?2,frame=?3 WHERE id=1 AND version=?4 AND frame IS NULL",
                params![
                    outcome as u8,
                    (outcome == TestTerminalOutcome::MockEffect) as u8,
                    frame
                    ,version
                ],
            )? != 1 {return Err(Error::Stale);}
        }
        if tx.execute(
            "INSERT INTO rail_commands VALUES(?1,?2,?3)",
            params![request.as_slice(), command, frame],
        )? != 1
        {
            return Err(Error::Invalid);
        }
        let result = match frame {
            Some(b) => TestRailDecision::Terminal(self.verified_frame(&b)?),
            None => TestRailDecision::Held,
        };
        tx.commit()?;
        Ok(result)
    }
}
