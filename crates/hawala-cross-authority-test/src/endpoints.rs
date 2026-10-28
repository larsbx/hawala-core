//! Signed, local, TEST-only RPC endpoints. No financial/participant authority.
use crate::{Result, TestHost};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        net::UnixListener,
    },
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub const REQUEST_DOMAIN: &[u8] = b"hawala/TEST-endpoints/request/v1\0";
pub const RESPONSE_DOMAIN: &[u8] = b"hawala/TEST-endpoints/response/v1\0";
pub const MAX_FRAME: usize = 4096;
pub fn digest(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}
pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
pub fn unhex(s: &str) -> Result<[u8; 32]> {
    if s.len() != 64
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("canonical hex32 required".into());
    }
    let mut a = [0; 32];
    for (i, x) in a.iter_mut().enumerate() {
        *x = u8::from_str_radix(&s[2 * i..2 * i + 2], 16)?;
    }
    Ok(a)
}
#[derive(Clone)]
pub struct Policy {
    pub host: [u8; 32],
    pub grant: [u8; 32],
    pub epoch: u64,
    pub allowed: u8,
}
#[derive(Clone, Debug)]
pub struct Call {
    pub operation: u8,
    pub service: [u8; 32],
    pub host: [u8; 32],
    pub grant: [u8; 32],
    pub epoch: u64,
    pub request: [u8; 32],
    pub attempt: [u8; 32],
    pub binding: [u8; 32],
    pub issued: u64,
    pub expires: u64,
}
impl Call {
    pub fn bytes(&self) -> Vec<u8> {
        let mut b = REQUEST_DOMAIN.to_vec();
        b.push(self.operation);
        for p in [&self.service, &self.host, &self.grant] {
            b.extend(p);
        }
        b.extend(self.epoch.to_be_bytes());
        for p in [&self.request, &self.attempt, &self.binding] {
            b.extend(p);
        }
        b.extend(self.issued.to_be_bytes());
        b.extend(self.expires.to_be_bytes());
        b
    }
    pub fn sign(&self, key: &SigningKey) -> Vec<u8> {
        let mut b = self.bytes();
        b.extend(key.sign(&b).to_bytes());
        b
    }
    pub fn parse(b: &[u8]) -> Result<Self> {
        let expected = REQUEST_DOMAIN.len() + 1 + 6 * 32 + 3 * 8 + 64;
        if b.len() != expected || !b.starts_with(REQUEST_DOMAIN) {
            return Err("bad TEST request".into());
        }
        let mut p = REQUEST_DOMAIN.len();
        let operation = b[p];
        p += 1;
        let service = take32(b, &mut p);
        let host = take32(b, &mut p);
        let grant = take32(b, &mut p);
        let epoch = take64(b, &mut p);
        let request = take32(b, &mut p);
        let attempt = take32(b, &mut p);
        let binding = take32(b, &mut p);
        let issued = take64(b, &mut p);
        let expires = take64(b, &mut p);
        Ok(Self {
            operation,
            service,
            host,
            grant,
            epoch,
            request,
            attempt,
            binding,
            issued,
            expires,
        })
    }
}
fn take32(b: &[u8], p: &mut usize) -> [u8; 32] {
    let a = b[*p..*p + 32].try_into().unwrap();
    *p += 32;
    a
}
fn take64(b: &[u8], p: &mut usize) -> u64 {
    let a = u64::from_be_bytes(b[*p..*p + 8].try_into().unwrap());
    *p += 8;
    a
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub stage: u8,
    pub held: bool,
}
fn response(key: &SigningKey, raw: &[u8], call: &Call, view: View) -> Vec<u8> {
    let mut b = RESPONSE_DOMAIN.to_vec();
    b.extend(key.verifying_key().to_bytes());
    b.extend(digest(raw));
    b.extend(call.attempt);
    b.extend(call.binding);
    b.extend([view.stage, u8::from(view.held), 0, 0]);
    b.extend(call.epoch.to_be_bytes());
    b.extend(key.sign(&b).to_bytes());
    b
}
pub fn verify_response(raw: &[u8], request: &[u8], service: [u8; 32]) -> Result<View> {
    let call = Call::parse(request)?;
    if call.service != service {
        return Err("wrong pinned service".into());
    }
    let n = RESPONSE_DOMAIN.len() + 4 * 32 + 4 + 8 + 64;
    if raw.len() != n || !raw.starts_with(RESPONSE_DOMAIN) {
        return Err("wrong TEST response domain/length".into());
    }
    let mut p = RESPONSE_DOMAIN.len();
    if take32(raw, &mut p) != service
        || take32(raw, &mut p) != digest(request)
        || take32(raw, &mut p) != call.attempt
        || take32(raw, &mut p) != call.binding
    {
        return Err("wrong response binding".into());
    }
    let stage = raw[p];
    let held = raw[p + 1];
    if stage > 3 || held > 1 || raw[p + 2] != 0 || raw[p + 3] != 0 {
        return Err("financial claims forbidden in TEST response".into());
    }
    p += 4;
    if take64(raw, &mut p) != call.epoch {
        return Err("wrong response epoch".into());
    }
    VerifyingKey::from_bytes(&service)?
        .verify_strict(&raw[..p], &Signature::from_bytes(raw[p..].try_into()?))?;
    Ok(View {
        stage,
        held: held == 1,
    })
}

type AttemptRow = (Vec<u8>, Vec<u8>, u8, Option<Vec<u8>>);

pub struct Service {
    root: PathBuf,
    policy: Policy,
    key: SigningKey,
}
impl Service {
    pub fn open(
        root: impl AsRef<Path>,
        policy: Policy,
        key: SigningKey,
        initialize: bool,
    ) -> Result<Self> {
        if policy.epoch == 0
            || policy.grant == [0; 32]
            || policy.host == key.verifying_key().to_bytes()
            || policy.allowed & !15 != 0
        {
            return Err("invalid TEST role/grant policy".into());
        }
        VerifyingKey::from_bytes(&policy.host)?;
        let root = root.as_ref().to_path_buf();
        if initialize {
            fs::create_dir(&root)?;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        }
        let s = Self { root, policy, key };
        let db = s.db(initialize)?;
        let mut pin = s.key.verifying_key().to_bytes().to_vec();
        pin.extend(s.policy.host);
        pin.extend(s.policy.grant);
        pin.extend(s.policy.epoch.to_be_bytes());
        pin.push(s.policy.allowed);
        let pin = digest(&pin);
        if initialize {
            db.execute_batch("CREATE TABLE enrollment (pin BLOB NOT NULL); CREATE TABLE attempts(id BLOB PRIMARY KEY, owner BLOB NOT NULL, binding BLOB NOT NULL, stage INTEGER NOT NULL, target INTEGER NOT NULL, inflight BLOB); CREATE TABLE requests(id BLOB PRIMARY KEY, body BLOB NOT NULL, response BLOB);")?;
            db.execute("INSERT INTO enrollment VALUES (?1)", [pin.as_slice()])?;
        }
        let found: Vec<u8> = db.query_row("SELECT pin FROM enrollment", [], |r| r.get(0))?;
        if found != pin {
            return Err("HOLD: service enrollment/epoch mismatch".into());
        }
        Ok(s)
    }
    fn db(&self, create: bool) -> Result<Connection> {
        let db = Connection::open_with_flags(
            self.root.join("endpoint.db"),
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | if create {
                    OpenFlags::SQLITE_OPEN_CREATE
                } else {
                    OpenFlags::empty()
                },
        )?;
        db.busy_timeout(Duration::from_secs(2))?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "FULL")?;
        Ok(db)
    }
    fn authorize(&self, raw: &[u8], c: &Call, now: u64) -> Result<()> {
        if !(1..=4).contains(&c.operation)
            || c.service != self.key.verifying_key().to_bytes()
            || c.host != self.policy.host
            || c.grant != self.policy.grant
            || c.epoch != self.policy.epoch
            || self.policy.allowed & (1 << (c.operation - 1)) == 0
            || c.request == [0; 32]
            || c.attempt == [0; 32]
            || c.binding == [0; 32]
            || c.issued > now
            || c.expires <= now
            || c.expires <= c.issued
            || c.expires - c.issued > 60
        {
            return Err("HOLD: call not admitted".into());
        }
        let p = raw.len() - 64;
        VerifyingKey::from_bytes(&self.policy.host)?
            .verify_strict(&raw[..p], &Signature::from_bytes(raw[p..].try_into()?))?;
        Ok(())
    }
    fn actors(&self, c: &Call) -> PathBuf {
        self.root.join(hex(&c.attempt))
    }
    fn observed(&self, c: &Call) -> Result<u8> {
        let h = TestHost::open_existing(self.actors(c))?;
        let s = h.projection()?.0;
        if s[6] == 2 {
            return Err("HOLD: aborted lab".into());
        }
        Ok(if s[6] == 1 && s[7] == 1 && s[9] == 1 && s[10] == 1 {
            3
        } else if s[2] == 1 && s[3] == 1 && s[4] == 1 && s[5] == 1 {
            2
        } else {
            1
        })
    }
    fn reconcile(&self, attempt: &[u8; 32]) -> Result<()> {
        let mut db = self.db(false)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let pending: Option<(Vec<u8>, u8)> = tx
            .query_row(
                "SELECT inflight,target FROM attempts WHERE id=?1 AND inflight IS NOT NULL",
                [attempt.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((id, target)) = pending {
            let raw: Vec<u8> =
                tx.query_row("SELECT body FROM requests WHERE id=?1", [&id], |r| r.get(0))?;
            let c = Call::parse(&raw)?;
            if self.observed(&c).is_ok_and(|v| v >= target) {
                let reply = response(
                    &self.key,
                    &raw,
                    &c,
                    View {
                        stage: target,
                        held: false,
                    },
                );
                tx.execute(
                    "UPDATE requests SET response=?1 WHERE id=?2",
                    params![reply, id],
                )?;
                tx.execute(
                    "UPDATE attempts SET stage=?1,inflight=NULL WHERE id=?2",
                    params![target, attempt.as_slice()],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn handle(&self, raw: &[u8], now: u64) -> Result<Vec<u8>> {
        let c = Call::parse(raw)?;
        self.authorize(raw, &c, now)?;
        let held = || {
            response(
                &self.key,
                raw,
                &c,
                View {
                    stage: 0,
                    held: true,
                },
            )
        };
        // Reject wrong binding and request-ID substitution before *any* recovery
        // write. An invalid call must not finalize even a different known attempt.
        let pre = self.db(false)?;
        let prior: Option<(Vec<u8>, Option<Vec<u8>>)> = pre
            .query_row(
                "SELECT body,response FROM requests WHERE id=?1",
                [c.request.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((body, reply)) = prior {
            if body != raw {
                return Ok(held());
            }
            if let Some(reply) = reply {
                return Ok(reply);
            }
        }
        let scope: Option<(Vec<u8>, Vec<u8>)> = pre
            .query_row(
                "SELECT owner,binding FROM attempts WHERE id=?1",
                [c.attempt.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if scope.is_some_and(|(owner, binding)| owner != c.host || binding != c.binding) {
            return Ok(held());
        }
        drop(pre);
        // Reconciliation only observes already durable effects. Never resends a
        // mutation or abandons an unknown attempt, even under a fresh request ID.
        self.reconcile(&c.attempt)?;
        let mut db = self.db(false)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let prior: Option<(Vec<u8>, Option<Vec<u8>>)> = tx
            .query_row(
                "SELECT body,response FROM requests WHERE id=?1",
                [c.request.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((body, reply)) = prior {
            if body != raw {
                return Ok(held());
            }
            return Ok(reply.unwrap_or_else(held));
        }
        let row: Option<AttemptRow> = tx
            .query_row(
                "SELECT owner,binding,stage,inflight FROM attempts WHERE id=?1",
                [c.attempt.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        if row
            .as_ref()
            .is_some_and(|(owner, binding, _, _)| owner != &c.host || binding != &c.binding)
        {
            return Ok(held());
        }
        let stage = row.as_ref().map_or(0, |r| r.2);
        let busy = row.as_ref().is_some_and(|r| r.3.is_some());
        let unavailable = c.operation == 4 && stage > 0 && self.observed(&c).ok() != Some(stage);
        if c.operation == 4 || stage >= c.operation || busy || c.operation != stage + 1 {
            let reply = response(
                &self.key,
                raw,
                &c,
                View {
                    stage,
                    held: busy
                        || unavailable
                        || stage == 0
                        || (c.operation != 4 && c.operation > stage + 1),
                },
            );
            tx.execute(
                "INSERT INTO requests VALUES (?1,?2,?3)",
                params![c.request.as_slice(), raw, reply],
            )?;
            tx.commit()?;
            return Ok(reply);
        }
        if row.is_none() {
            tx.execute(
                "INSERT INTO attempts VALUES (?1,?2,?3,0,?4,?5)",
                params![
                    c.attempt.as_slice(),
                    c.host.as_slice(),
                    c.binding.as_slice(),
                    c.operation,
                    c.request.as_slice()
                ],
            )?;
        } else {
            tx.execute(
                "UPDATE attempts SET target=?1,inflight=?2 WHERE id=?3",
                params![c.operation, c.request.as_slice(), c.attempt.as_slice()],
            )?;
        }
        tx.execute(
            "INSERT INTO requests VALUES (?1,?2,NULL)",
            params![c.request.as_slice(), raw],
        )?;
        tx.commit()?;
        let effects = (|| -> Result<()> {
            if c.operation == 1 {
                TestHost::create(self.actors(&c))?;
            } else {
                let mut h = TestHost::open_existing(self.actors(&c))?;
                let events: &[&str] = if c.operation == 2 {
                    &[
                        "sendPrepare",
                        "prepareL(0,0,0)",
                        "prepareR(0,0,0)",
                        "receiveL",
                        "receiveR",
                    ]
                } else {
                    &["commit", "publish", "deliverL", "deliverR"]
                };
                for e in events {
                    h.apply(&format!("{}:{e}", hex(&c.request)), e)?;
                }
            }
            Ok(())
        })();
        if effects.is_err() {
            return Ok(held());
        }
        self.reconcile(&c.attempt)?;
        let db = self.db(false)?;
        let reply: Option<Vec<u8>> = db.query_row(
            "SELECT response FROM requests WHERE id=?1",
            [c.request.as_slice()],
            |r| r.get(0),
        )?;
        Ok(reply.unwrap_or_else(held))
    }
    pub fn serve_loopback(
        &self,
        port_file: &Path,
        max_requests: usize,
        drop_first_commit_reply: bool,
    ) -> Result<()> {
        use std::net::{Ipv4Addr, TcpListener};
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(port_file)?;
        fs::set_permissions(port_file, fs::Permissions::from_mode(0o600))?;
        writeln!(file, "{}", listener.local_addr()?.port())?;
        file.sync_all()?;
        let mut dropped = false;
        for incoming in listener.incoming().take(if max_requests == 0 {
            usize::MAX
        } else {
            max_requests
        }) {
            let mut stream = incoming?;
            stream.set_read_timeout(Some(Duration::from_secs(1)))?;
            stream.set_write_timeout(Some(Duration::from_secs(1)))?;
            let _ = self.process_one(&mut stream, &mut dropped, drop_first_commit_reply);
        }
        Ok(())
    }
    fn process_one(
        &self,
        stream: &mut (impl Read + Write),
        dropped: &mut bool,
        drop_reply: bool,
    ) -> Result<()> {
        let request = read_frame(stream)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let reply = self.handle(&request, now)?;
        if drop_reply
            && !*dropped
            && Call::parse(&request)?.operation == 3
            && verify_response(&reply, &request, self.key.verifying_key().to_bytes())?.stage == 3
        {
            *dropped = true;
            return Ok(());
        }
        write_frame(stream, &reply)
    }
    pub fn serve(
        &self,
        socket: &Path,
        max_requests: usize,
        drop_first_commit_reply: bool,
    ) -> Result<()> {
        if socket.exists() {
            return Err("refuse existing socket path".into());
        }
        let listener = UnixListener::bind(socket)?;
        fs::set_permissions(socket, fs::Permissions::from_mode(0o600))?;
        let meta = fs::symlink_metadata(socket)?;
        struct Cleanup {
            path: PathBuf,
            dev: u64,
            ino: u64,
        }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                if let Ok(m) = fs::symlink_metadata(&self.path) {
                    if (m.dev(), m.ino()) == (self.dev, self.ino) {
                        let _ = fs::remove_file(&self.path);
                    }
                }
            }
        }
        let _cleanup = Cleanup {
            path: socket.into(),
            dev: meta.dev(),
            ino: meta.ino(),
        };
        let mut dropped = false;
        for incoming in listener.incoming().take(if max_requests == 0 {
            usize::MAX
        } else {
            max_requests
        }) {
            let mut stream = incoming?;
            stream.set_read_timeout(Some(Duration::from_secs(1)))?;
            stream.set_write_timeout(Some(Duration::from_secs(1)))?;
            let exchange = (|| -> Result<()> {
                let request = read_frame(&mut stream)?;
                let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
                let reply = self.handle(&request, now)?;
                if drop_first_commit_reply
                    && !dropped
                    && Call::parse(&request)?.operation == 3
                    && verify_response(&reply, &request, self.key.verifying_key().to_bytes())?.stage
                        == 3
                {
                    dropped = true;
                    return Ok(());
                }
                write_frame(&mut stream, &reply)
            })();
            if exchange.is_err() {
                continue;
            }
        }
        Ok(())
    }
}
pub fn read_frame(stream: &mut impl Read) -> Result<Vec<u8>> {
    let mut size = [0; 4];
    stream.read_exact(&mut size)?;
    let n = u32::from_be_bytes(size) as usize;
    if n == 0 || n > MAX_FRAME {
        return Err("frame exceeds TEST bound".into());
    }
    let mut b = vec![0; n];
    stream.read_exact(&mut b)?;
    Ok(b)
}
pub fn write_frame(stream: &mut impl Write, b: &[u8]) -> Result<()> {
    if b.is_empty() || b.len() > MAX_FRAME {
        return Err("frame bound".into());
    }
    stream.write_all(&(b.len() as u32).to_be_bytes())?;
    stream.write_all(b)?;
    Ok(())
}
