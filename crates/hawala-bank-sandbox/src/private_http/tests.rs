use super::*;
use std::cell::Cell;

const P: &[u8] = br#"{"accounts":[{"account_id":"acc_scope","type":"depository","balances":{"available":10.25,"iso_currency_code":"USD"}}]}"#;
const A: &[u8] = br#"{"id":"acc_scope","type":"depository","status":"open","currency":"USD","name":"ignored synthetic datum","links":{"balances":"https://untrusted.invalid/"}}"#;
const B: &[u8] = br#"{"account_id":"acc_scope","available":"10.25","ledger":"55.00"}"#;

fn grant(provider: Provider) -> BalanceGrant {
    BalanceGrant {
        provider,
        account_id: Zeroizing::new("acc_scope".into()),
        business_id: [1; 32],
        authorized_agent: [2; 32],
        threshold_minor: 1025,
        expires_at_unix: 200,
    }
}

fn plaid_credentials() -> SandboxCredentials {
    SandboxCredentials::Plaid {
        client_id: Zeroizing::new("fixture-client-id".into()),
        secret: Zeroizing::new("fixture-secret".into()),
        access_token: Zeroizing::new("access-sandbox-fictional".into()),
    }
}

fn teller_credentials() -> SandboxCredentials {
    SandboxCredentials::TellerTestToken(Zeroizing::new("test_token_fictional".into()))
}

fn connector(grant: BalanceGrant, creds: SandboxCredentials) -> SandboxConnector {
    SandboxConnector::activate_with(grant, creds, || Ok(100), Https::new).unwrap()
}

fn read(
    c: &SandboxConnector,
    exchange: impl Fn(Request) -> Result<Vec<u8>, ReadUnavailable>,
) -> Result<bool, ReadUnavailable> {
    c.read_with([1; 32], [2; 32], || Ok(100), || Duration::ZERO, exchange)
}

#[test]
fn plaid_posts_for_only_the_granted_account() {
    let c = connector(grant(Provider::Plaid), plaid_credentials());
    assert_eq!(
        read(&c, |r| {
            assert_eq!(r.method(), Method::POST);
            assert_eq!(
                r.url().as_str(),
                "https://sandbox.plaid.com/accounts/balance/get"
            );
            assert_eq!(r.timeout(), Some(&READ_BUDGET));
            assert!(r.headers()["plaid-client-id"].is_sensitive());
            assert!(r.headers()["plaid-secret"].is_sensitive());
            assert_eq!(r.headers()["plaid-client-id"], "fixture-client-id");
            assert_eq!(r.headers()["plaid-secret"], "fixture-secret");
            assert_eq!(r.headers()[CONTENT_TYPE], "application/json");
            assert_eq!(r.headers()[CACHE_CONTROL], "no-cache, no-store");
            assert!(!r.headers().contains_key(AUTHORIZATION));
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(r.body().unwrap().as_bytes().unwrap())
                    .unwrap(),
                serde_json::json!({"access_token": "access-sandbox-fictional", "options": {"account_ids": ["acc_scope"]}})
            );
            Ok(P.to_vec())
        }),
        Ok(true)
    );
}

#[test]
fn teller_reads_fixed_metadata_and_balance_paths_and_ignores_links() {
    let c = connector(grant(Provider::Teller), teller_credentials());
    let calls = Cell::new(0);
    assert_eq!(
        read(&c, |r| {
            assert_eq!(r.method(), Method::GET);
            assert!(r.body().is_none());
            assert_eq!(r.url().host_str(), Some(TELLER_HOST));
            assert_eq!(r.url().scheme(), "https");
            assert!(r.url().query().is_none());
            assert!(r.headers()[AUTHORIZATION].is_sensitive());
            let auth = r.headers()[AUTHORIZATION]
                .to_str()
                .unwrap()
                .strip_prefix("Basic ")
                .unwrap();
            assert_eq!(STANDARD.decode(auth).unwrap(), b"test_token_fictional:");
            let response = match calls.get() {
                0 => {
                    assert_eq!(r.url().path(), "/accounts/acc_scope");
                    A
                }
                1 => {
                    assert_eq!(r.url().path(), "/accounts/acc_scope/balances");
                    B
                }
                _ => panic!("unexpected extra read"),
            };
            calls.set(calls.get() + 1);
            Ok(response.to_vec())
        }),
        Ok(true)
    );
    assert_eq!(calls.get(), 2);
}

#[test]
fn activation_rejects_unsafe_grants_and_tokens_before_http_initialization() {
    let reject = |g, k| {
        let result = SandboxConnector::activate_with(
            g,
            k,
            || Ok(100),
            || panic!("invalid grant reached HTTP setup"),
        );
        assert!(matches!(result, Err(ReadUnavailable)));
    };
    reject(grant(Provider::Teller), plaid_credentials());
    for token in [
        "token_live_fixture",
        "test_token_",
        "test_token_bad:password",
        "test_token_bad\n",
    ] {
        reject(
            grant(Provider::Teller),
            SandboxCredentials::TellerTestToken(Zeroizing::new(token.into())),
        );
    }
    for account_id in [
        "",
        "../other",
        "acc%2fother",
        "acc?x=1",
        "acc#fragment",
        "acc/other",
    ] {
        let mut g = grant(Provider::Teller);
        g.account_id = Zeroizing::new(account_id.into());
        reject(g, teller_credentials());
    }
    let mut g = grant(Provider::Plaid);
    g.expires_at_unix = 100;
    reject(g, plaid_credentials());
    let mut g = grant(Provider::Plaid);
    g.authorized_agent = [0; 32];
    reject(g, plaid_credentials());
    let mut wrong_env = plaid_credentials();
    if let SandboxCredentials::Plaid { access_token, .. } = &mut wrong_env {
        *access_token = Zeroizing::new("access-production-fictional".into());
    }
    reject(grant(Provider::Plaid), wrong_env);
}

#[test]
fn denies_wrong_business_agent_expired_and_revoked_grants_before_egress() {
    let c = connector(grant(Provider::Plaid), plaid_credentials());
    for (business, agent, now) in [([3; 32], [2; 32], 100), ([1; 32], [3; 32], 100)] {
        assert_eq!(
            c.read_with(
                business,
                agent,
                || Ok(now),
                || Duration::ZERO,
                |_| panic!("unexpected egress")
            ),
            Err(ReadUnavailable)
        );
    }
    c.revoke();
    c.revoke();
    assert_eq!(
        read(&c, |_| panic!("unexpected egress")),
        Err(ReadUnavailable)
    );
    assert!(c.credentials.lock().unwrap().is_none());

    let c = connector(grant(Provider::Plaid), plaid_credentials());
    assert_eq!(
        c.read_with(
            [1; 32],
            [2; 32],
            || Ok(200),
            || Duration::ZERO,
            |_| panic!("unexpected egress")
        ),
        Err(ReadUnavailable)
    );
    assert!(c.credentials.lock().unwrap().is_none());
    assert_eq!(
        read(&c, |_| panic!(
            "expiry must remain fail-closed after clock rollback"
        )),
        Err(ReadUnavailable)
    );
}

#[test]
fn teller_revocation_stops_second_read_and_suppresses_in_flight_predicate() {
    let c = connector(grant(Provider::Teller), teller_credentials());
    assert_eq!(
        read(&c, |r| {
            assert_eq!(r.url().path(), "/accounts/acc_scope");
            c.revoke();
            Ok(A.to_vec())
        }),
        Err(ReadUnavailable)
    );

    let c = connector(grant(Provider::Teller), teller_credentials());
    assert_eq!(
        read(&c, |r| {
            if r.url().path().ends_with("/balances") {
                c.revoke();
                return Ok(B.to_vec());
            }
            Ok(A.to_vec())
        }),
        Err(ReadUnavailable)
    );
}

#[test]
fn expiry_and_total_timeout_are_checked_at_each_read_and_release() {
    let now = Cell::new(100);
    let elapsed = Cell::new(Duration::ZERO);
    let run = |exchange: &dyn Fn(Request) -> Result<Vec<u8>, ReadUnavailable>| {
        let c = connector(grant(Provider::Teller), teller_credentials());
        now.set(100);
        elapsed.set(Duration::ZERO);
        c.read_with(
            [1; 32],
            [2; 32],
            || Ok(now.get()),
            || elapsed.get(),
            exchange,
        )
    };
    assert_eq!(
        run(&|r| {
            assert_eq!(r.url().path(), "/accounts/acc_scope");
            now.set(200);
            Ok(A.to_vec())
        }),
        Err(ReadUnavailable)
    );
    assert_eq!(
        run(&|r| {
            assert_eq!(r.url().path(), "/accounts/acc_scope");
            elapsed.set(READ_BUDGET);
            Ok(A.to_vec())
        }),
        Err(ReadUnavailable)
    );
    assert_eq!(
        run(&|r| {
            if r.url().path().ends_with("/balances") {
                assert_eq!(r.timeout(), Some(&Duration::from_secs(5)));
                elapsed.set(READ_BUDGET);
                return Ok(B.to_vec());
            }
            elapsed.set(Duration::from_secs(30));
            Ok(A.to_vec())
        }),
        Err(ReadUnavailable)
    );
    assert_eq!(
        run(&|r| {
            if r.url().path().ends_with("/balances") {
                now.set(200);
                return Ok(B.to_vec());
            }
            Ok(A.to_vec())
        }),
        Err(ReadUnavailable)
    );
}

#[test]
fn malformed_ineligible_cross_account_and_currency_metadata_stop_teller_before_balance() {
    let c = connector(grant(Provider::Teller), teller_credentials());
    for metadata in [
        "{".to_string(),
        String::from_utf8_lossy(A).replace("open", "closed"),
        String::from_utf8_lossy(A).replace("depository", "credit"),
        String::from_utf8_lossy(A).replace("acc_scope", "acc_other"),
        String::from_utf8_lossy(A).replace("USD", "CAD"),
    ] {
        assert_eq!(
            read(&c, |r| {
                assert_eq!(r.url().path(), "/accounts/acc_scope");
                Ok(metadata.as_bytes().to_vec())
            }),
            Err(ReadUnavailable)
        );
    }
}

#[test]
fn private_reads_release_only_boolean_or_opaque_refusal() {
    let plaid = connector(grant(Provider::Plaid), plaid_credentials());
    let teller = connector(grant(Provider::Teller), teller_credentials());
    let plaid_payload = String::from_utf8_lossy(P);
    let teller_payload = String::from_utf8_lossy(B);
    assert_eq!(
        read(&plaid, |_| Ok(plaid_payload
            .replace("10.25", "10.24")
            .into_bytes())),
        Ok(false)
    );
    assert_eq!(
        read(&teller, |r| Ok(if r.url().path().ends_with("/balances") {
            teller_payload.replace("10.25", "10.24").into_bytes()
        } else {
            A.to_vec()
        })),
        Ok(false)
    );
    for payload in [
        plaid_payload.replace("10.25", "null"),
        plaid_payload.replace("10.25", "92233720368547758.08"),
        plaid_payload.replace(
            "\"available\":10.25",
            "\"available\":10.25,\"available\":9.00",
        ),
        plaid_payload.replace("acc_scope", "acc_other"),
        "x".repeat(MAX_JSON + 1),
    ] {
        assert_eq!(
            read(&plaid, |_| Ok(payload.as_bytes().to_vec())),
            Err(ReadUnavailable)
        );
    }
    for payload in [
        teller_payload.replace("\"10.25\"", "null"),
        teller_payload.replace("10.25", "92233720368547758.08"),
        teller_payload.replace("acc_scope", "acc_other"),
        r#"{"account_id":"acc_scope","available":"10.25","currency":"CAD"}"#.into(),
    ] {
        assert_eq!(
            read(&teller, |r| Ok(if r.url().path().ends_with("/balances") {
                payload.as_bytes().to_vec()
            } else {
                A.to_vec()
            })),
            Err(ReadUnavailable)
        );
    }
    assert_eq!(read(&plaid, |_| Err(ReadUnavailable)), Err(ReadUnavailable));
}

#[test]
fn non_success_http_statuses_and_oversized_bodies_fail_closed() {
    let mut headers = reqwest::header::HeaderMap::new();
    for status in [
        StatusCode::NO_CONTENT,
        StatusCode::MOVED_PERMANENTLY,
        StatusCode::TEMPORARY_REDIRECT,
        StatusCode::BAD_REQUEST,
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::GONE,
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::BAD_GATEWAY,
    ] {
        assert_eq!(
            check_response(status, Some(20), headers.get_all(AGE)),
            Err(ReadUnavailable)
        );
    }
    assert_eq!(
        check_response(StatusCode::OK, None, headers.get_all(AGE)),
        Ok(())
    );
    assert_eq!(
        check_response(
            StatusCode::OK,
            Some(MAX_JSON as u64 + 1),
            headers.get_all(AGE)
        ),
        Err(ReadUnavailable)
    );
    headers.insert(AGE, HeaderValue::from_static("0"));
    assert_eq!(
        check_response(StatusCode::OK, None, headers.get_all(AGE)),
        Ok(())
    );
    for age in ["1", "invalid"] {
        headers.append(AGE, HeaderValue::from_static(age));
    }
    assert_eq!(
        check_response(StatusCode::OK, None, headers.get_all(AGE)),
        Err(ReadUnavailable)
    );
}
