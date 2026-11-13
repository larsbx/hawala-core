//! Plaid/Teller private-node synthetic sandbox adapter.
//! No real HTTP transport, banking credentials, authentic bank provenance,
//! zero-knowledge proving, funds reservation, payment or financial authority.

use serde_json::Value;
pub const MAX_JSON: usize = 128 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider { Plaid, Teller }

/// The future private transport must limit origins to permitted sandbox hosts.
/// PlaidBalance is POST /accounts/balance/get (fresh, not cached accounts/get).
/// TellerAccount and TellerBalance are GET /accounts/:id and /balances.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endpoint { PlaidBalance, TellerAccount, TellerBalance }

/// The transport must NEVER forward raw responses or credentials to Biz-kit.
/// Errors are sanitized and are not allowed to contain customer information.
pub trait PrivateTransport {
    fn read(&self, endpoint: Endpoint, account_id: &str) -> std::result::Result<Vec<u8>, ()>;
}
/// Synthetic local consent signal, NOT an authenticated authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalConsent { GrantedUntil(u64), Revoked }

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
pub enum Provenance { UnverifiedSandboxPayload }
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
    InvalidRequest, ConsentInvalid, Transport, Oversized, Malformed,
    AccountMismatch, AccountIneligible, CurrencyMismatch,
    AvailableMissing, AmountInvalid,
}
type Result<T> = std::result::Result<T, Error>;

fn parsed(bytes: &[u8]) -> Result<Value> {
    if bytes.len() > MAX_JSON { return Err(Error::Oversized); }
    serde_json::from_slice(bytes).map_err(|_| Error::Malformed)
}
fn field<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v.get(k).and_then(Value::as_str).ok_or(Error::Malformed)
}

/// USD cents, no binary floating point. arbitrary_precision retains the JSON
/// numeric lexeme. V1 deliberately rejects exponent, sub-cent, overflow.
fn usd_minor(raw: &str) -> Result<i64> {
    let (negative, t) = raw.strip_prefix('-').map_or((false, raw), |x| (true, x));
    let mut parts = t.split('.');
    let whole = parts.next().ok_or(Error::AmountInvalid)?;
    let frac = parts.next();
    if parts.next().is_some() || whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit()) { return Err(Error::AmountInvalid); }
    let cents: i128 = match frac {
        None => 0,
        Some(s) if (1..=2).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit()) => {
            let p = s.parse::<i128>().map_err(|_| Error::AmountInvalid)?;
            if s.len() == 1 { p * 10 } else { p }
        },
        _ => return Err(Error::AmountInvalid),
    };
    let dollars = whole.parse::<i128>().map_err(|_| Error::AmountInvalid)?;
    let absolute = dollars.checked_mul(100).and_then(|n| n.checked_add(cents))
        .ok_or(Error::AmountInvalid)?;
    if absolute > i64::MAX as i128 { return Err(Error::AmountInvalid); }
    Ok((if negative { -absolute } else { absolute }) as i64)
}
fn plaid(v: &Value, req: &BalanceRequest) -> Result<i64> {
    let accounts = v.get("accounts").and_then(Value::as_array).ok_or(Error::Malformed)?;
    let mut found = accounts.iter().filter(|v| v.get("account_id").and_then(Value::as_str) == Some(req.account_id.as_str()));
    let a = found.next().ok_or(Error::AccountMismatch)?;
    if found.next().is_some() { return Err(Error::Malformed); }
    if field(a, "type")? != "depository" { return Err(Error::AccountIneligible); }
    let balances = a.get("balances").ok_or(Error::Malformed)?;
    if field(balances, "iso_currency_code")?.as_bytes() != req.currency {
        return Err(Error::CurrencyMismatch);
    }
    let n = balances.get("available").and_then(Value::as_number).ok_or(Error::AvailableMissing)?;
    usd_minor(&n.to_string())
}
fn teller(a: &Value, b: &Value, req: &BalanceRequest) -> Result<i64> {
    if field(a, "id")? != req.account_id || field(b, "account_id")? != req.account_id {
        return Err(Error::AccountMismatch);
    }
    if field(a, "type")? != "depository" || field(a, "status")? != "open" {
        return Err(Error::AccountIneligible);
    }
    if field(a, "currency")?.as_bytes() != req.currency { return Err(Error::CurrencyMismatch); }
    usd_minor(b.get("available").and_then(Value::as_str).ok_or(Error::AvailableMissing)?)
}

/// Unverified local preview only: positive does NOT mean paid, verified,
/// bank-origin authenticated, safely funded, or settlement-final.
pub fn evaluate(t: &impl PrivateTransport, req: &BalanceRequest, now: u64) -> Result<Preview> {
    if req.account_id.is_empty() || req.account_id.len() > 128
        || !req.account_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        || req.business_id == [0; 32] || req.nonce == [0; 32]
        || req.threshold_minor == 0 || req.threshold_minor > i64::MAX as u64 || req.currency != *b"USD"
    { return Err(Error::InvalidRequest); }
    match req.consent {
        LocalConsent::GrantedUntil(u) if now < u => {},
        _ => return Err(Error::ConsentInvalid),
    }
    let available = match req.provider {
        Provider::Plaid => {
            let response = t.read(Endpoint::PlaidBalance, &req.account_id).map_err(|_| Error::Transport)?;
            plaid(&parsed(&response)?, req)?
        },
        Provider::Teller => {
            let account = t.read(Endpoint::TellerAccount, &req.account_id).map_err(|_| Error::Transport)?;
            let balance = t.read(Endpoint::TellerBalance, &req.account_id).map_err(|_| Error::Transport)?;
            teller(&parsed(&account)?, &parsed(&balance)?, req)?
        },
    };
    Ok(Preview {
        business_id: req.business_id, nonce: req.nonce,
        currency: req.currency, threshold_minor: req.threshold_minor,
        meets_threshold: i128::from(available) >= i128::from(req.threshold_minor),
        provenance: Provenance::UnverifiedSandboxPayload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const P: &str = r#"{"accounts":[{"account_id":"plaid_demo","type":"depository","balances":{"available":12500.25,"current":13000,"iso_currency_code":"USD"}}]}"#;
    const A: &str = r#"{"id":"teller_demo","type":"depository","status":"open","currency":"USD","last_four":"0000"}"#;
    const B: &str = r#"{"account_id":"teller_demo","available":"12500.25","ledger":"13000.00"}"#;
    struct Fixture { p: String, a: String, b: String, fail: bool }
    impl Default for Fixture {
        fn default() -> Self { Self { p: P.into(), a: A.into(), b: B.into(), fail: false } }
    }
    impl PrivateTransport for Fixture {
        fn read(&self, ep: Endpoint, _: &str) -> std::result::Result<Vec<u8>, ()> {
            if self.fail { return Err(()); }
            Ok(match ep { Endpoint::PlaidBalance => &self.p, Endpoint::TellerAccount => &self.a, Endpoint::TellerBalance => &self.b }.as_bytes().to_vec())
        }
    }
    fn r(provider: Provider) -> BalanceRequest {
        BalanceRequest { provider,
            account_id: match provider { Provider::Plaid => "plaid_demo", Provider::Teller => "teller_demo" }.into(),
            business_id: [1; 32], nonce: [2; 32], threshold_minor: 1_000_000,
            currency: *b"USD", consent: LocalConsent::GrantedUntil(200),
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
        let mut f = Fixture::default();
        f.p = P.replace("12500.25", "9999.99");
        assert!(!evaluate(&f, &r(Provider::Plaid), 100).unwrap().meets_threshold);
        f.b = B.replace("12500.25", "-5.00");
        assert!(!evaluate(&f, &r(Provider::Teller), 100).unwrap().meets_threshold);
    }
    #[test]
    fn rejects_unavailable_cross_account_credit_closed_currency_mismatch() {
        let mut f = Fixture::default();
        f.p = P.replace("plaid_demo", "other");
        assert_eq!(evaluate(&f, &r(Provider::Plaid), 100), Err(Error::AccountMismatch));
        f.p = P.replace("depository", "credit");
        assert_eq!(evaluate(&f, &r(Provider::Plaid), 100), Err(Error::AccountIneligible));
        f.p = P.replace("USD", "EUR");
        assert_eq!(evaluate(&f, &r(Provider::Plaid), 100), Err(Error::CurrencyMismatch));
        f.p = P.replace("12500.25", "null");
        assert_eq!(evaluate(&f, &r(Provider::Plaid), 100), Err(Error::AvailableMissing));
        f.a = A.replace("open", "closed");
        assert_eq!(evaluate(&f, &r(Provider::Teller), 100), Err(Error::AccountIneligible));
        f.b = B.replace("teller_demo", "wrong");
        assert_eq!(evaluate(&f, &r(Provider::Teller), 100), Err(Error::AccountMismatch));
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
        assert_eq!(evaluate(&Fixture { fail: true, ..Fixture::default() }, &r(Provider::Plaid), 100), Err(Error::Transport));
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
        let mut f = Fixture::default();
        f.p = P.replace("12500.25", "0.001");
        assert_eq!(evaluate(&f, &r(Provider::Plaid), 100), Err(Error::AmountInvalid));
    }
    #[test]
    fn duplicate_account_malformed_and_oversized_json_refused() {
        let mut f = Fixture::default();
        f.p = "x".repeat(MAX_JSON + 1);
        assert_eq!(evaluate(&f, &r(Provider::Plaid), 100), Err(Error::Oversized));
        f.p = "{broken".into();
        assert_eq!(evaluate(&f, &r(Provider::Plaid), 100), Err(Error::Malformed));
        let acc = r#"{"account_id":"plaid_demo","type":"depository","balances":{"available":10000,"iso_currency_code":"USD"}}"#;
        f.p = format!("{{\"accounts\":[{acc},{acc}]}}");
        assert_eq!(evaluate(&f, &r(Provider::Plaid), 100), Err(Error::Malformed));
    }
}
