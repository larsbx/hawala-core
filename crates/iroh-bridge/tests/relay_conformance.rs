//! Explicit local-relay gate. Missing prerequisites fail this test target.
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use ed25519_dalek::SigningKey;
use hawala_identity_core::{IdentityId, ReplayGuard};
use hawala_iroh_bridge::{handshake, IDENTITY_ALPN};
use iroh::{
    endpoint::presets, tls::CaTlsConfig, Endpoint, EndpointAddr, RelayMode, RelayUrl, Watcher,
};

const DEADLINE: Duration = Duration::from_secs(20);

#[derive(Default)]
struct Nonces(Mutex<HashSet<(IdentityId, IdentityId, [u8; 32])>>);

impl ReplayGuard for Nonces {
    fn consume_once(
        &self,
        verifier: IdentityId,
        claimant: IdentityId,
        nonce: [u8; 32],
    ) -> hawala_identity_core::Result<bool> {
        Ok(self.0.lock().unwrap().insert((verifier, claimant, nonce)))
    }
}

struct Relay {
    process: Child,
    directory: tempfile::TempDir,
    url: RelayUrl,
    root: Vec<u8>,
}

impl Relay {
    async fn start(auth_url: Option<&str>) -> Self {
        let binary = std::env::var_os("HAWALA_TEST_RELAY_BIN")
            .expect("set HAWALA_TEST_RELAY_BIN to the verified iroh-relay 1.4.1 executable");
        let directory = tempfile::tempdir().unwrap();
        let cert = directory.path().join("relay.pem");
        let private_key = directory.path().join("relay.key");
        let der = directory.path().join("relay.der");
        let generated = Command::new("openssl")
            .args([
                "req",
                "-x509",
                "-newkey",
                "ec",
                "-pkeyopt",
                "ec_paramgen_curve:P-256",
                "-nodes",
                "-days",
                "1",
                "-subj",
                "/CN=localhost",
                "-addext",
                "subjectAltName=IP:127.0.0.1,DNS:localhost",
                "-addext",
                "basicConstraints=critical,CA:FALSE",
                "-addext",
                "keyUsage=critical,digitalSignature",
                "-addext",
                "extendedKeyUsage=serverAuth",
                "-keyout",
            ])
            .arg(&private_key)
            .arg("-out")
            .arg(&cert)
            .output()
            .expect("openssl is required for the local TLS fixture");
        assert!(
            generated.status.success(),
            "certificate generation: {:?}",
            generated
        );
        assert!(Command::new("openssl")
            .args(["x509", "-in"])
            .arg(&cert)
            .args(["-outform", "DER", "-out"])
            .arg(&der)
            .status()
            .unwrap()
            .success());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let access = auth_url
            .map(|url| format!("access.http.url = {url:?}\n"))
            .unwrap_or_default();
        let config = directory.path().join("relay.toml");
        std::fs::write(
            &config,
            format!(
                "http_bind_addr = \"127.0.0.1:0\"\nenable_metrics = false\n{access}\n[tls]\n\
             https_bind_addr = \"{address}\"\ncert_mode = \"Manual\"\n\
             manual_cert_path = {cert:?}\nmanual_key_path = {private_key:?}\n"
            ),
        )
        .unwrap();
        let log = std::fs::File::create(directory.path().join("relay.log")).unwrap();
        let process = Command::new(binary)
            .arg("--config-path")
            .arg(&config)
            .env_remove("IROH_RELAY_ACCESS_TOKEN")
            .env_remove("IROH_RELAY_HTTP_BEARER_TOKEN")
            .env_remove("HTTP_PROXY")
            .env_remove("HTTPS_PROXY")
            .env_remove("ALL_PROXY")
            .env_remove("http_proxy")
            .env_remove("https_proxy")
            .env_remove("all_proxy")
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .expect("launch the local relay");
        let mut relay = Self {
            process,
            root: std::fs::read(der).unwrap(),
            directory,
            url: format!("https://{address}/").parse().unwrap(),
        };
        tokio::time::timeout(DEADLINE, async {
            loop {
                assert!(
                    relay.process.try_wait().unwrap().is_none(),
                    "relay exited: {}",
                    std::fs::read_to_string(relay.directory.path().join("relay.log")).unwrap()
                );
                if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("relay startup exceeded its deadline");
        relay
    }

    async fn endpoint(&self, trust_fixture: bool) -> Endpoint {
        let tls = if trust_fixture {
            CaTlsConfig::custom_roots([self.root.clone().into()])
        } else {
            CaTlsConfig::default()
        };
        Endpoint::builder(presets::Minimal)
            .clear_ip_transports()
            .clear_address_lookup()
            .relay_mode(RelayMode::custom([self.url.clone()]))
            .ca_tls_config(tls)
            .alpns(vec![IDENTITY_ALPN.to_vec()])
            .bind()
            .await
            .unwrap()
    }

    async fn online(&self, endpoint: &Endpoint) {
        if tokio::time::timeout(DEADLINE, endpoint.online())
            .await
            .is_err()
        {
            panic!(
                "relay did not connect: {:?}; relay log: {}",
                endpoint.home_relay_status().get(),
                std::fs::read_to_string(self.directory.path().join("relay.log")).unwrap()
            );
        }
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verified_tls_relay_carries_the_mutual_handshake_without_any_ip_path() {
    let relay = Relay::start(None).await;
    let alice = relay.endpoint(true).await;
    let bob = relay.endpoint(true).await;
    tokio::join!(relay.online(&alice), relay.online(&bob));
    assert!(alice.bound_sockets().is_empty());
    assert!(bob.bound_sockets().is_empty());
    let target = EndpointAddr::new(bob.id()).with_relay_url(relay.url.clone());
    let alice_key = SigningKey::from_bytes(&[1; 32]);
    let bob_key = SigningKey::from_bytes(&[2; 32]);
    let alice_id = IdentityId::from_signing_key(&alice_key);
    let bob_id = IdentityId::from_signing_key(&bob_key);
    let alice_nonces = Nonces::default();
    let bob_nonces = Nonces::default();
    tokio::time::timeout(DEADLINE, async {
        tokio::join!(
            async {
                let connection = alice.connect(target, IDENTITY_ALPN).await.unwrap();
                let peer =
                    handshake::initiate(&alice, &connection, &alice_key, bob_id, &alice_nonces)
                        .await
                        .unwrap();
                assert_eq!(peer.endpoint_key(), *bob.id().as_bytes());
                assert!(connection.paths().iter().any(|path| path.is_relay()));
                assert!(connection.paths().iter().all(|path| !path.is_ip()));
            },
            async {
                let connection = bob.accept().await.unwrap().await.unwrap();
                let peer = handshake::accept(&bob, &connection, &bob_key, alice_id, &bob_nonces)
                    .await
                    .unwrap();
                assert_eq!(peer.endpoint_key(), *alice.id().as_bytes());
                assert!(connection.paths().iter().any(|path| path.is_relay()));
            }
        );
    })
    .await
    .expect("relay-only handshake exceeded its deadline");
    alice.close().await;
    bob.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_untrusted_relay_certificate_is_rejected_while_the_trusted_control_connects() {
    let relay = Relay::start(None).await;
    let trusted = relay.endpoint(true).await;
    relay.online(&trusted).await;
    let untrusted = relay.endpoint(false).await;
    // An endpoint may not select a home relay after its initial TLS probe fails.
    // Use the identical CA configuration for a direct TLS diagnostic as well.
    let config = CaTlsConfig::default()
        .client_config(iroh::tls::default_provider())
        .unwrap();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let address = format!("127.0.0.1:{}", relay.url.port().unwrap());
    let result = tokio::time::timeout(DEADLINE, async {
        let socket = tokio::net::TcpStream::connect(address).await.unwrap();
        connector
            .connect("localhost".try_into().unwrap(), socket)
            .await
    })
    .await
    .expect("TLS diagnostic exceeded its deadline");
    let error = result.expect_err("system trust must reject the ephemeral relay certificate");
    assert!(
        error.to_string().contains("UnknownIssuer"),
        "expected certificate refusal: {error:?}"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(250), untrusted.online())
            .await
            .is_err()
    );
    untrusted.close().await;
    trusted.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn relay_loss_during_identity_exchange_finishes_within_the_frame_deadline() {
    let mut relay = Relay::start(None).await;
    let alice = relay.endpoint(true).await;
    let bob = relay.endpoint(true).await;
    tokio::join!(relay.online(&alice), relay.online(&bob));
    let target = EndpointAddr::new(bob.id()).with_relay_url(relay.url.clone());
    let (left, right) = tokio::time::timeout(DEADLINE, async {
        tokio::join!(
            async { alice.connect(target, IDENTITY_ALPN).await.unwrap() },
            async { bob.accept().await.unwrap().await.unwrap() }
        )
    })
    .await
    .expect("authenticated TLS connections before the relay fault");
    assert!(left.paths().iter().all(|path| path.is_relay()));
    relay.process.kill().unwrap();
    relay.process.wait().unwrap();
    let alice_key = SigningKey::from_bytes(&[1; 32]);
    let bob_key = SigningKey::from_bytes(&[2; 32]);
    let alice_nonces = Nonces::default();
    let bob_nonces = Nonces::default();
    let (initiator, acceptor) = tokio::time::timeout(Duration::from_secs(25), async {
        tokio::join!(
            handshake::initiate(
                &alice,
                &left,
                &alice_key,
                IdentityId::from_signing_key(&bob_key),
                &alice_nonces
            ),
            handshake::accept(
                &bob,
                &right,
                &bob_key,
                IdentityId::from_signing_key(&alice_key),
                &bob_nonces
            )
        )
    })
    .await
    .expect("the existing 20-second frame deadline must bound relay loss");
    assert!(matches!(
        initiator,
        Err(handshake::HandshakeError::Timeout | handshake::HandshakeError::Transport)
    ));
    assert!(matches!(
        acceptor,
        Err(handshake::HandshakeError::Timeout | handshake::HandshakeError::Transport)
    ));
    assert!(alice_nonces.0.lock().unwrap().is_empty());
    assert!(bob_nonces.0.lock().unwrap().is_empty());
    alice.close().await;
    bob.close().await;
}

struct AuthService {
    url: String,
    requests: Arc<Mutex<Vec<(String, String)>>>,
    allow: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl AuthService {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/auth", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let allow = Arc::new(AtomicBool::new(true));
        let stop = Arc::new(AtomicBool::new(false));
        let (thread_requests, thread_allow, thread_stop) =
            (requests.clone(), allow.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        socket
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut bytes = Vec::new();
                        while !bytes.ends_with(b"\r\n\r\n") {
                            assert!(bytes.len() < 8192, "bounded auth fixture request");
                            let mut byte = [0];
                            socket.read_exact(&mut byte).unwrap();
                            bytes.push(byte[0]);
                        }
                        let request = String::from_utf8(bytes).unwrap();
                        assert!(request.starts_with("POST /auth HTTP/1.1\r\n"));
                        let header = |name: &str| {
                            request
                                .lines()
                                .find_map(|line| {
                                    let (key, value) = line.split_once(':')?;
                                    key.eq_ignore_ascii_case(name)
                                        .then(|| value.trim().to_string())
                                })
                                .unwrap_or_default()
                        };
                        let pair = (header("X-Iroh-Endpoint-Id"), header("X-Iroh-NodeId"));
                        let valid = pair.0.parse::<iroh::EndpointId>().is_ok() && pair.0 == pair.1;
                        thread_requests.lock().unwrap().push(pair);
                        let body = if valid && thread_allow.load(Ordering::SeqCst) {
                            "true"
                        } else {
                            "false"
                        };
                        write!(socket, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("auth fixture listener: {error}"),
                }
            }
        });
        Self {
            url,
            requests,
            allow,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for AuthService {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn relay_http_auth_sends_matching_endpoint_headers_and_denies_refused_peers() {
    let auth = AuthService::start();
    let relay = Relay::start(Some(&auth.url)).await;
    let accepted = relay.endpoint(true).await;
    relay.online(&accepted).await;
    let accepted_id = accepted.id().to_string();
    assert!(auth
        .requests
        .lock()
        .unwrap()
        .contains(&(accepted_id.clone(), accepted_id)));
    auth.allow.store(false, Ordering::SeqCst);
    let denied = relay.endpoint(true).await;
    let denied_id = denied.id().to_string();
    tokio::time::timeout(DEADLINE, async {
        loop {
            if auth
                .requests
                .lock()
                .unwrap()
                .contains(&(denied_id.clone(), denied_id.clone()))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("observe the rejected relay connection and its auth request");
    assert!(
        tokio::time::timeout(Duration::from_millis(250), denied.online())
            .await
            .is_err()
    );
    denied.close().await;
    accepted.close().await;
}
