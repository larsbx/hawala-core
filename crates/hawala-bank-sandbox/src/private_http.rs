//! Read-only HTTP for the private connector process. Public reads release only
//! the scoped predicate or an opaque refusal. The raw transport stays private.

use super::{
    read_threshold, valid_account_id, valid_threshold, Endpoint, PrivateTransport, Provider,
    MAX_JSON,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use bytes::Bytes;
use reqwest::header::{HeaderValue, ACCEPT, AGE, AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE};
use reqwest::{Client, Method, Request, StatusCode, Url};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::runtime::Runtime;
use zeroize::Zeroizing;

const PLAID_HOST: &str = "sandbox.plaid.com";
const TELLER_HOST: &str = "api.teller.io";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
// Plaid's synchronous refresh can take up to 30 seconds or more. This budget
// covers the whole operation, including both requests for Teller and body reads.
const READ_BUDGET: Duration = Duration::from_secs(35);

/// Injected by the private runtime from its secret store. These values must come
/// from the approved sandbox enrollment for the grant's selected account.
pub enum SandboxCredentials {
    Plaid {
        client_id: Zeroizing<String>,
        secret: Zeroizing<String>,
        access_token: Zeroizing<String>,
    },
    TellerTestToken(Zeroizing<String>),
}

/// A private runtime grant created after explicit sandbox enrollment consent
/// and verification of the representative's authority for the business. A grant
/// authorizes only this account and one fixed USD balance threshold, in cents.
pub struct BalanceGrant {
    pub provider: Provider,
    pub account_id: Zeroizing<String>,
    pub business_id: [u8; 32],
    pub authorized_agent: [u8; 32],
    pub threshold_minor: u64,
    pub expires_at_unix: u64,
}

/// Safe to return outside the connector. No provider details are included.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadUnavailable;

/// The connector's trusted local caller owns this type. Invoke on a synchronous
/// worker. Pass the authenticated business and representative from the local
/// session, rather than forwarding client-supplied identity fields.
pub struct SandboxConnector {
    grant: BalanceGrant,
    credentials: Mutex<Option<SandboxCredentials>>,
    https: Https,
}

impl SandboxConnector {
    pub fn activate(
        grant: BalanceGrant,
        credentials: SandboxCredentials,
    ) -> Result<Self, ReadUnavailable> {
        Self::activate_with(grant, credentials, unix_now, Https::new)
    }

    fn activate_with(
        grant: BalanceGrant,
        credentials: SandboxCredentials,
        now: impl Fn() -> Result<u64, ReadUnavailable>,
        https: impl FnOnce() -> Result<Https, ReadUnavailable>,
    ) -> Result<Self, ReadUnavailable> {
        if !valid_account_id(&grant.account_id)
            || !valid_threshold(grant.threshold_minor)
            || grant.business_id == [0; 32]
            || grant.authorized_agent == [0; 32]
            || now()? >= grant.expires_at_unix
            || !credentials.matches_sandbox(grant.provider)
        {
            return Err(ReadUnavailable);
        }
        Ok(Self {
            grant,
            credentials: Mutex::new(Some(credentials)),
            https: https()?,
        })
    }

    /// Permanently revoke this grant and drop retained credentials. An already
    /// admitted request may finish, and its result is discarded after revocation.
    pub fn revoke(&self) {
        // Recover the guard even if a local runtime thread previously panicked.
        self.credentials
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .take();
    }

    /// Returns an unverified sandbox predicate for the grant's fixed threshold.
    pub fn meets_threshold(
        &self,
        authenticated_business: [u8; 32],
        authenticated_agent: [u8; 32],
    ) -> Result<bool, ReadUnavailable> {
        let started = Instant::now();
        self.read_with(
            authenticated_business,
            authenticated_agent,
            unix_now,
            || started.elapsed(),
            |request| self.https.exchange(request),
        )
    }

    fn active(
        &self,
        now: &impl Fn() -> Result<u64, ReadUnavailable>,
    ) -> Result<MutexGuard<'_, Option<SandboxCredentials>>, ReadUnavailable> {
        let mut guard = self.credentials.lock().map_err(|_| ReadUnavailable)?;
        if now()? >= self.grant.expires_at_unix {
            guard.take();
        }
        if guard.is_none() {
            return Err(ReadUnavailable);
        }
        Ok(guard)
    }

    fn read_with(
        &self,
        business: [u8; 32],
        agent: [u8; 32],
        now: impl Fn() -> Result<u64, ReadUnavailable>,
        elapsed: impl Fn() -> Duration,
        exchange: impl Fn(Request) -> Result<Vec<u8>, ReadUnavailable>,
    ) -> Result<bool, ReadUnavailable> {
        if business != self.grant.business_id || agent != self.grant.authorized_agent {
            return Err(ReadUnavailable);
        }
        let transport = ScopedTransport(|endpoint, account_id: &str| {
            let request = {
                let guard = self.active(&now)?;
                let credentials = guard.as_ref().ok_or(ReadUnavailable)?;
                let remaining = READ_BUDGET
                    .checked_sub(elapsed())
                    .filter(|time| !time.is_zero())
                    .ok_or(ReadUnavailable)?;
                credentials.request(endpoint, account_id, remaining)?
            };
            exchange(request)
        });
        let meets = read_threshold(
            &transport,
            self.grant.provider,
            &self.grant.account_id,
            *b"USD",
            self.grant.threshold_minor,
        )
        .map_err(|_| ReadUnavailable)?;

        // Linearize result release with local revocation. Recheck after the
        // final response so an expired grant cannot release an in-flight result.
        let _active = self.active(&now)?;
        if elapsed() >= READ_BUDGET {
            return Err(ReadUnavailable);
        }
        Ok(meets)
    }
}

fn unix_now() -> Result<u64, ReadUnavailable> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .map_err(|_| ReadUnavailable)
}

struct ScopedTransport<F>(F);

impl<F> PrivateTransport for ScopedTransport<F>
where
    F: Fn(Endpoint, &str) -> Result<Vec<u8>, ReadUnavailable>,
{
    fn read(&self, endpoint: Endpoint, account_id: &str) -> Result<Vec<u8>, ()> {
        self.0(endpoint, account_id).map_err(|_| ())
    }
}

impl SandboxCredentials {
    fn matches_sandbox(&self, provider: Provider) -> bool {
        match (self, provider) {
            (
                Self::Plaid {
                    client_id,
                    secret,
                    access_token,
                },
                Provider::Plaid,
            ) => {
                credential_ascii(client_id)
                    && credential_ascii(secret)
                    && access_token
                        .strip_prefix("access-sandbox-")
                        .is_some_and(credential_ascii)
            }
            // Teller shares this host across environments. Limit this transport
            // to its documented test token form and never add an mTLS identity.
            (Self::TellerTestToken(token), Provider::Teller) => token
                .strip_prefix("test_token_")
                .is_some_and(credential_ascii),
            _ => false,
        }
    }

    fn request(
        &self,
        endpoint: Endpoint,
        account_id: &str,
        timeout: Duration,
    ) -> Result<Request, ReadUnavailable> {
        if !valid_account_id(account_id) {
            return Err(ReadUnavailable);
        }
        let (method, host, path) = match endpoint {
            Endpoint::PlaidBalance => (Method::POST, PLAID_HOST, "/accounts/balance/get".into()),
            Endpoint::TellerAccount => {
                (Method::GET, TELLER_HOST, format!("/accounts/{account_id}"))
            }
            Endpoint::TellerBalance => (
                Method::GET,
                TELLER_HOST,
                format!("/accounts/{account_id}/balances"),
            ),
        };
        let url = Url::parse(&format!("https://{host}{path}")).map_err(|_| ReadUnavailable)?;
        let mut request = Request::new(method, url);
        *request.timeout_mut() = Some(timeout);
        request
            .headers_mut()
            .insert(ACCEPT, HeaderValue::from_static("application/json"));
        request.headers_mut().insert(
            CACHE_CONTROL,
            HeaderValue::from_static("no-cache, no-store"),
        );

        match (self, endpoint) {
            (
                Self::Plaid {
                    client_id,
                    secret,
                    access_token,
                },
                Endpoint::PlaidBalance,
            ) => {
                let headers = request.headers_mut();
                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                headers.insert("plaid-client-id", sensitive_header(client_id)?);
                headers.insert("plaid-secret", sensitive_header(secret)?);
                // All interpolated values use the validated ASCII alphabet.
                let json = format!(
                    r#"{{"access_token":"{access_token}","options":{{"account_ids":["{account_id}"]}}}}"#,
                    access_token = access_token.as_str()
                );
                *request.body_mut() =
                    Some(Bytes::from_owner(SecretBody(Zeroizing::new(json))).into());
            }
            (Self::TellerTestToken(token), Endpoint::TellerAccount | Endpoint::TellerBalance) => {
                // Byte-compatible HTTP Basic auth with an empty password.
                let basic = Zeroizing::new(format!("{}:", token.as_str()));
                let mut auth =
                    Zeroizing::new(String::with_capacity(6 + 4 * basic.len().div_ceil(3)));
                auth.push_str("Basic ");
                STANDARD.encode_string(basic.as_bytes(), &mut auth);
                request
                    .headers_mut()
                    .insert(AUTHORIZATION, sensitive_header(&auth)?);
            }
            _ => return Err(ReadUnavailable),
        }
        Ok(request)
    }
}

fn credential_ascii(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn sensitive_header(value: &str) -> Result<HeaderValue, ReadUnavailable> {
    let mut header = HeaderValue::from_str(value).map_err(|_| ReadUnavailable)?;
    header.set_sensitive(true);
    Ok(header)
}

struct SecretBody(Zeroizing<String>);

impl AsRef<[u8]> for SecretBody {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

struct Https {
    client: Client,
    runtime: Runtime,
}

impl Https {
    fn new() -> Result<Self, ReadUnavailable> {
        // Use the bundled public trust store, with hostname verification. No
        // client identity, TLS key logging, or caller-configured TLS settings.
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| ReadUnavailable)?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let client = Client::builder()
            .tls_backend_preconfigured(tls)
            .https_only(true)
            .http1_only()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .referer(false)
            .retry(reqwest::retry::never())
            .connection_verbose(false)
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(READ_BUDGET)
            .build()
            .map_err(|_| ReadUnavailable)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|_| ReadUnavailable)?;
        Ok(Self { client, runtime })
    }

    fn exchange(&self, request: Request) -> Result<Vec<u8>, ReadUnavailable> {
        self.runtime.block_on(async {
            let mut response = self
                .client
                .execute(request)
                .await
                .map_err(|_| ReadUnavailable)?;
            check_response(
                response.status(),
                response.content_length(),
                response.headers().get_all(AGE),
            )?;

            let mut body = Zeroizing::new(Vec::with_capacity(MAX_JSON));
            while let Some(chunk) = response.chunk().await.map_err(|_| ReadUnavailable)? {
                if chunk.len() > MAX_JSON - body.len() {
                    return Err(ReadUnavailable);
                }
                body.extend_from_slice(&chunk);
            }
            Ok(std::mem::take(&mut *body))
        })
    }
}

fn check_response(
    status: StatusCode,
    length: Option<u64>,
    age: reqwest::header::GetAll<'_, HeaderValue>,
) -> Result<(), ReadUnavailable> {
    if status != StatusCode::OK
        || length.is_some_and(|n| n > MAX_JSON as u64)
        || age.iter().any(|value| value != "0")
    {
        return Err(ReadUnavailable);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
