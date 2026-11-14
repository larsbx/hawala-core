//! Private-node sandbox balance predicates.

use serde::Deserialize;
use serde_json::value::RawValue;
use std::borrow::Cow;
use zeroize::Zeroizing;

#[cfg(feature = "private-http")]
pub mod private_http;

pub const MAX_JSON: usize = 128 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Plaid,
    Teller,
}

/// A private transport must limit origins to permitted sandbox hosts.
/// PlaidBalance is POST /accounts/balance/get (fresh, not cached accounts/get).
/// TellerAccount and TellerBalance are GET /accounts/:id and /balances.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endpoint {
    PlaidBalance,
    TellerAccount,
    TellerBalance,
}

/// The transport must NEVER forward raw responses or credentials to Biz-kit.
/// Errors are sanitized and are not allowed to contain customer information.
pub trait PrivateTransport {
    // Keep the existing opaque transport error contract.
    #[allow(clippy::result_unit_err)]
    fn read(&self, endpoint: Endpoint, account_id: &str) -> std::result::Result<Vec<u8>, ()>;
}
/// Synthetic local consent signal, NOT an authenticated authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalConsent {
    GrantedUntil(u64),
    Revoked,
}

/// Private input: intentionally not Debug/Serialize.
pub struct BalanceRequest {
    pub provider: Provider,
    pub account_id: String,
    pub business_id: [u8; 32],
    pub nonce: [u8; 32],
    pub threshold_minor: u64,
    pub currency: [u8; 3],
    pub consent: LocalConsent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provenance {
    UnverifiedSandboxPayload,
}
/// Contains neither account ID, real balance nor raw bank response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preview {
    pub business_id: [u8; 32],
    pub nonce: [u8; 32],
    pub currency: [u8; 3],
    pub threshold_minor: u64,
    pub meets_threshold: bool,
    pub provenance: Provenance,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidRequest,
    ConsentInvalid,
    Transport,
    Oversized,
    Malformed,
    AccountMismatch,
    AccountIneligible,
    CurrencyMismatch,
    AvailableMissing,
    AmountInvalid,
}
type Result<T> = std::result::Result<T, Error>;

fn parse<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T> {
    if bytes.len() > MAX_JSON {
        return Err(Error::Oversized);
    }
    serde_json::from_slice(bytes).map_err(|_| Error::Malformed)
}

/// USD cents, no binary floating point. The raw JSON lexeme retains precision.
/// V1 deliberately rejects exponent, sub-cent, overflow.
fn usd_minor(raw: &str) -> Result<i64> {
    let (negative, t) = raw.strip_prefix('-').map_or((false, raw), |x| (true, x));
    let mut parts = t.split('.');
    let whole = parts.next().ok_or(Error::AmountInvalid)?;
    let frac = parts.next();
    if parts.next().is_some() || whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::AmountInvalid);
    }
    let cents: i128 = match frac {
        None => 0,
        Some(s) if (1..=2).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit()) => {
            let p = s.parse::<i128>().map_err(|_| Error::AmountInvalid)?;
            if s.len() == 1 {
                p * 10
            } else {
                p
            }
        }
        _ => return Err(Error::AmountInvalid),
    };
    let dollars = whole.parse::<i128>().map_err(|_| Error::AmountInvalid)?;
    let absolute = dollars
        .checked_mul(100)
        .and_then(|n| n.checked_add(cents))
        .ok_or(Error::AmountInvalid)?;
    if absolute > i64::MAX as i128 {
        return Err(Error::AmountInvalid);
    }
    Ok((if negative { -absolute } else { absolute }) as i64)
}
#[derive(Deserialize)]
struct PlaidResponse<'a> {
    #[serde(borrow)]
    accounts: Vec<PlaidAccount<'a>>,
}

#[derive(Deserialize)]
struct PlaidAccount<'a> {
    #[serde(borrow)]
    account_id: Cow<'a, str>,
    #[serde(borrow, rename = "type")]
    kind: Option<Cow<'a, str>>,
    #[serde(borrow)]
    balances: Option<PlaidBalances<'a>>,
}

#[derive(Deserialize)]
struct PlaidBalances<'a> {
    #[serde(borrow)]
    iso_currency_code: Option<Cow<'a, str>>,
    #[serde(borrow)]
    available: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct TellerAccount<'a> {
    #[serde(borrow)]
    id: Cow<'a, str>,
    #[serde(borrow, rename = "type")]
    kind: Cow<'a, str>,
    #[serde(borrow)]
    status: Cow<'a, str>,
    #[serde(borrow)]
    currency: Cow<'a, str>,
}

#[derive(Deserialize)]
struct TellerBalance<'a> {
    #[serde(borrow)]
    account_id: Cow<'a, str>,
    #[serde(borrow)]
    available: Option<Cow<'a, str>>,
    #[serde(borrow)]
    currency: Option<Cow<'a, str>>,
}

fn plaid(bytes: &[u8], account_id: &str, currency: [u8; 3]) -> Result<i64> {
    let response: PlaidResponse<'_> = parse(bytes)?;
    let mut found = response
        .accounts
        .into_iter()
        .filter(|a| a.account_id == account_id);
    let a = found.next().ok_or(Error::AccountMismatch)?;
    if found.next().is_some() {
        return Err(Error::Malformed);
    }
    if a.kind.as_deref() != Some("depository") {
        return Err(Error::AccountIneligible);
    }
    let balances = a.balances.ok_or(Error::Malformed)?;
    let iso = balances.iso_currency_code.ok_or(Error::Malformed)?;
    if iso.as_bytes() != currency {
        return Err(Error::CurrencyMismatch);
    }
    let raw = balances.available.ok_or(Error::AvailableMissing)?.get();
    if !raw.starts_with(|c: char| c == '-' || c.is_ascii_digit()) {
        return Err(Error::AvailableMissing);
    }
    usd_minor(raw)
}

fn teller_account(bytes: &[u8], account_id: &str, currency: [u8; 3]) -> Result<()> {
    let a: TellerAccount<'_> = parse(bytes)?;
    if a.id != account_id {
        return Err(Error::AccountMismatch);
    }
    if a.kind != "depository" || a.status != "open" {
        return Err(Error::AccountIneligible);
    }
    if a.currency.as_bytes() != currency {
        return Err(Error::CurrencyMismatch);
    }
    Ok(())
}

fn teller_balance(bytes: &[u8], account_id: &str, currency: [u8; 3]) -> Result<i64> {
    let b: TellerBalance<'_> = parse(bytes)?;
    if b.account_id != account_id {
        return Err(Error::AccountMismatch);
    }
    if b.currency.is_some_and(|iso| iso.as_bytes() != currency) {
        return Err(Error::CurrencyMismatch);
    }
    usd_minor(b.available.as_deref().ok_or(Error::AvailableMissing)?)
}

fn valid_account_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn valid_threshold(threshold_minor: u64) -> bool {
    (1..=i64::MAX as u64).contains(&threshold_minor)
}

fn read_threshold(
    t: &impl PrivateTransport,
    provider: Provider,
    account_id: &str,
    currency: [u8; 3],
    threshold_minor: u64,
) -> Result<bool> {
    let read = |endpoint| {
        t.read(endpoint, account_id)
            .map(Zeroizing::new)
            .map_err(|_| Error::Transport)
    };
    let available = match provider {
        Provider::Plaid => plaid(&read(Endpoint::PlaidBalance)?, account_id, currency)?,
        Provider::Teller => {
            teller_account(&read(Endpoint::TellerAccount)?, account_id, currency)?;
            teller_balance(&read(Endpoint::TellerBalance)?, account_id, currency)?
        }
    };
    Ok(i128::from(available) >= i128::from(threshold_minor))
}

/// Unverified local preview only: positive does NOT mean paid, verified,
/// bank-origin authenticated, safely funded, or settlement-final.
pub fn evaluate(t: &impl PrivateTransport, req: &BalanceRequest, now: u64) -> Result<Preview> {
    if !valid_account_id(&req.account_id)
        || req.business_id == [0; 32]
        || req.nonce == [0; 32]
        || !valid_threshold(req.threshold_minor)
        || req.currency != *b"USD"
    {
        return Err(Error::InvalidRequest);
    }
    match req.consent {
        LocalConsent::GrantedUntil(u) if now < u => {}
        _ => return Err(Error::ConsentInvalid),
    }
    let meets_threshold = read_threshold(
        t,
        req.provider,
        &req.account_id,
        req.currency,
        req.threshold_minor,
    )?;
    Ok(Preview {
        business_id: req.business_id,
        nonce: req.nonce,
        currency: req.currency,
        threshold_minor: req.threshold_minor,
        meets_threshold,
        provenance: Provenance::UnverifiedSandboxPayload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const P: &str = r#"{"accounts":[{"account_id":"plaid_demo","type":"depository","balances":{"available":12500.25,"current":13000,"iso_currency_code":"USD"}}]}"#;
    const A: &str = r#"{"id":"teller_demo","type":"depository","status":"open","currency":"USD","last_four":"0000"}"#;
    const B: &str = r#"{"account_id":"teller_demo","available":"12500.25","ledger":"13000.00"}"#;
    struct Fixture {
        p: String,
        a: String,
        b: String,
        fail: bool,
    }
    impl Default for Fixture {
        fn default() -> Self {
            Self {
                p: P.into(),
                a: A.into(),
                b: B.into(),
                fail: false,
            }
        }
    }
    impl PrivateTransport for Fixture {
        fn read(&self, ep: Endpoint, _: &str) -> std::result::Result<Vec<u8>, ()> {
            if self.fail {
                return Err(());
            }
            Ok(match ep {
                Endpoint::PlaidBalance => &self.p,
                Endpoint::TellerAccount => &self.a,
                Endpoint::TellerBalance => &self.b,
            }
            .as_bytes()
            .to_vec())
        }
    }
    fn r(provider: Provider) -> BalanceRequest {
        BalanceRequest {
            provider,
            account_id: match provider {
                Provider::Plaid => "plaid_demo",
                Provider::Teller => "teller_demo",
            }
            .into(),
            business_id: [1; 32],
            nonce: [2; 32],
            threshold_minor: 1_000_000,
            currency: *b"USD",
            consent: LocalConsent::GrantedUntil(200),
        }
    }
    #[test]
    fn same_private_result_both_providers() {
        for provider in [Provider::Plaid, Provider::Teller] {
            let p = evaluate(&Fixture::default(), &r(provider), 100).unwrap();
            assert!(p.meets_threshold);
            assert_eq!(p.provenance, Provenance::UnverifiedSandboxPayload);
            assert_eq!(p.threshold_minor, 1_000_000);
        }
    }
    #[test]
    fn insufficient_or_negative_funds_never_succeed() {
        let f = Fixture {
            p: P.replace("12500.25", "9999.99"),
            b: B.replace("12500.25", "-5.00"),
            ..Fixture::default()
        };
        assert!(
            !evaluate(&f, &r(Provider::Plaid), 100)
                .unwrap()
                .meets_threshold
        );
        assert!(
            !evaluate(&f, &r(Provider::Teller), 100)
                .unwrap()
                .meets_threshold
        );
    }
    #[test]
    fn rejects_unavailable_cross_account_credit_closed_currency_mismatch() {
        let mut f = Fixture {
            p: P.replace("plaid_demo", "other"),
            ..Fixture::default()
        };
        assert_eq!(
            evaluate(&f, &r(Provider::Plaid), 100),
            Err(Error::AccountMismatch)
        );
        f.p = P.replace("depository", "credit");
        assert_eq!(
            evaluate(&f, &r(Provider::Plaid), 100),
            Err(Error::AccountIneligible)
        );
        f.p = P.replace("USD", "EUR");
        assert_eq!(
            evaluate(&f, &r(Provider::Plaid), 100),
            Err(Error::CurrencyMismatch)
        );
        f.p = P.replace("12500.25", "null");
        assert_eq!(
            evaluate(&f, &r(Provider::Plaid), 100),
            Err(Error::AvailableMissing)
        );
        f.a = A.replace("open", "closed");
        assert_eq!(
            evaluate(&f, &r(Provider::Teller), 100),
            Err(Error::AccountIneligible)
        );
        f.a = A.into();
        f.b = B.replace("teller_demo", "wrong");
        assert_eq!(
            evaluate(&f, &r(Provider::Teller), 100),
            Err(Error::AccountMismatch)
        );
    }
    #[test]
    fn consent_path_and_transport_fail_closed() {
        let f = Fixture::default();
        let mut request = r(Provider::Teller);
        request.consent = LocalConsent::Revoked;
        assert_eq!(evaluate(&f, &request, 100), Err(Error::ConsentInvalid));
        request.consent = LocalConsent::GrantedUntil(100);
        assert_eq!(evaluate(&f, &request, 100), Err(Error::ConsentInvalid));
        request.consent = LocalConsent::GrantedUntil(200);
        request.account_id = "../credentials".into();
        assert_eq!(evaluate(&f, &request, 100), Err(Error::InvalidRequest));
        assert_eq!(
            evaluate(
                &Fixture {
                    fail: true,
                    ..Fixture::default()
                },
                &r(Provider::Plaid),
                100
            ),
            Err(Error::Transport)
        );
    }
    #[test]
    fn exact_decimal_bounds() {
        assert_eq!(usd_minor("0.01"), Ok(1));
        assert_eq!(usd_minor("0.1"), Ok(10));
        assert_eq!(usd_minor("-3.50"), Ok(-350));
        assert_eq!(usd_minor("92233720368547758.07"), Ok(i64::MAX));
        for invalid in ["1e4", "0.001", ".25", "1.", "", "92233720368547758.08"] {
            assert_eq!(usd_minor(invalid), Err(Error::AmountInvalid));
        }
        let f = Fixture {
            p: P.replace("12500.25", "0.001"),
            ..Fixture::default()
        };
        assert_eq!(
            evaluate(&f, &r(Provider::Plaid), 100),
            Err(Error::AmountInvalid)
        );
    }
    #[test]
    fn duplicate_account_malformed_and_oversized_json_refused() {
        let mut f = Fixture {
            p: "x".repeat(MAX_JSON + 1),
            ..Fixture::default()
        };
        assert_eq!(
            evaluate(&f, &r(Provider::Plaid), 100),
            Err(Error::Oversized)
        );
        f.p = "{broken".into();
        assert_eq!(
            evaluate(&f, &r(Provider::Plaid), 100),
            Err(Error::Malformed)
        );
        let acc = r#"{"account_id":"plaid_demo","type":"depository","balances":{"available":10000,"iso_currency_code":"USD"}}"#;
        f.p = format!("{{\"accounts\":[{acc},{acc}]}}");
        assert_eq!(
            evaluate(&f, &r(Provider::Plaid), 100),
            Err(Error::Malformed)
        );
    }
}
