# Private banking sandbox V1

The Rust crate exposes a read-only, unverified USD balance-threshold predicate. The private HTTPS transport is opt-in through the `private-http` Cargo feature. The fixture interface remains available in default builds.

| Provider | Allowed request | Authentication and account scope |
| --- | --- | --- |
| Plaid | `POST https://sandbox.plaid.com/accounts/balance/get` | Sandbox client ID and secret headers, sandbox Item access token in the body, and exactly one `options.account_ids` entry |
| Teller | `GET https://api.teller.io/accounts/{id}` followed by `GET https://api.teller.io/accounts/{id}/balances` | HTTP Basic with a documented `test_token_…` sandbox token and empty password. The API host is shared across environments. This client ships without a Teller client certificate, which Teller requires in development and production. |

Reference documentation for [Plaid Balance](https://plaid.com/docs/api/products/signal/#accountsbalanceget), [Teller authentication](https://teller.io/docs/api/authentication/), [Teller accounts](https://teller.io/docs/api/accounts/), and [Teller balances](https://teller.io/docs/api/account/balances).

## Private runtime

1. Establish the business and representative from an authenticated private session, confirm the representative's role, and obtain explicit consent for the fixed purpose, account, USD threshold, and expiration. Obtain the selected account and credentials from the matching provider sandbox enrollment. Use Plaid Sandbox Link or its sandbox token workflow. For Teller use the documented test-token workflow. Enrollment, role verification, durable grants, and signed provider webhook handling belong to the trusted runtime that supplies these inputs.
2. In the private process, build one `BalanceGrant` and matching `SandboxCredentials` from the authenticated record and secret store. Call `SandboxConnector::activate` on a synchronous worker. Credentials and account identifiers stay in process memory. Use a private OS identity and disable request capture, dependency wire tracing, core dumps, and shared telemetry in this process. Keep secrets out of arguments, environment dumps, and application configuration.
3. Pass the current authenticated business and representative to `meets_threshold`. The grant is checked before each request and before releasing the result. Expiry, revoked or missing grants, provider refusals, and unusable responses all return `ReadUnavailable`. Route local consent revocation, representative role removal, or a verified provider disconnect to `revoke`, which clears retained credentials. An already admitted read can finish, but a revoked grant cannot release its result after revocation. Rebuild only from current trusted authorization records after a restart.
4. Keep the predicate inside this sandbox workflow. The production transport is private to the module and the public read returns only a boolean or an opaque refusal. There is no listener, event writer, settlement call, or Biz-kit integration. Retained secret strings and owned raw response buffers are zeroized on drop. HTTP/TLS libraries can keep transient copies in process memory, so process isolation remains necessary.

The client verifies provider certificates and hostnames using bundled public roots and TLS 1.2 or newer. It prohibits redirects, automatic proxies, TLS session key logging, and HTTP retries. The connect timeout is five seconds, and the whole read has a 35-second budget across both Teller requests and their bodies. Each body is bounded at 128 KiB. Non-200 responses are refused without reading their body or reflecting provider error text. Requests ask caches to revalidate and skip storage, and responses with a positive or invalid HTTP `Age` are refused. These checks bound request age and reject advertised cache hits. They cannot prove the underlying bank observation's age.

Plaid available balances use their original JSON numeric lexeme. Teller uses the available decimal string. Missing balances, wrong account IDs, ineligible or closed accounts, currency mismatches, duplicate decision fields, sub-cent amounts, and overflow fail closed. Teller metadata must pass before the second request. The balance response must bind to the same account. Its two reads are independent observations and carry no atomic snapshot guarantee.

## Acceptance

```sh
cargo test -p hawala-bank-sandbox --locked
cargo test -p hawala-bank-sandbox --features private-http --locked
cargo test --workspace --all-targets --locked
```

The feature tests exercise the actual request builders with deterministic mock exchanges. CI needs only synthetic inputs. An opt-in sandbox run requires a private worker and approved sandbox credentials provisioned through its secret store. Record only success/refusal by provider and whether local scope, revocation, and credential cleanup passed. Revoke the local grant, drop the worker, and retire the test enrollment through the provider's sandbox controls.

| Evidence | What it establishes |
| --- | --- |
| Synthetic exchange | Request construction, local authorization checks, and predicate behavior for controlled responses |
| Credentialed sandbox read | The fixed provider API accepted the private sandbox credentials and returned usable simulated data over HTTPS |
| Independent bank provenance | Requires separately authenticated bank source evidence. Tracked with proof soundness in [issue #59](https://github.com/larsbx/hawala-core/issues/59). |
| Zero-knowledge proof | Requires an independently verified proof bound to an authenticated source and scoped public claim. |
| Financial settlement | Requires an authoritative settlement record. The predicate carries no financial authority. |
