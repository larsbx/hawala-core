# Iroh transport regression gate

The bridge pins `iroh = "=1.4.1"`. The workspace lock resolves
`iroh-relay 1.4.1`, `noq 1.4.0`, `rustls 0.23.46` and
`rustls-webpki 0.103.16`. Iroh 1.4.0 includes the fixes for
RUSTSEC-2026-0346 through 0349. The 1.4.1 upstream lockfile update includes
RUSTSEC-2026-0382's distrusted-intermediate fix. Each separately built relay
and DNS service needs its own artifact evidence.

Upstream references, checked November 6, 2026

- [Iroh 1.4.1 release and archive digests](https://github.com/n0-computer/iroh/releases/tag/v1.4.1)
- [TLS advisory batch](https://www.iroh.computer/blog/iroh-1-4-0)
- [Distrusted intermediates](https://rustsec.org/advisories/RUSTSEC-2026-0382.html)
- [Relay auth header migration](https://github.com/n0-computer/iroh/pull/4586)
- [Android upload and segmentation offload report](https://github.com/n0-computer/iroh/issues/4647)
- [Cellular path selection report](https://github.com/n0-computer/iroh/issues/4644)
- [Relay descriptor exhaustion report](https://github.com/n0-computer/iroh/issues/4636)

## Automated checks

| Target | Required behavior |
| --- | --- |
| `transport_regressions` | TLS refuses the wrong endpoint key at a reachable socket. A negotiated foreign ALPN and either QR expectation mismatch refuse identity admission. Proofs for an old endpoint reject after reconnect, while the fresh proof succeeds once. |
| `transport_regressions` bulk transfer | Eight MiB arrives intact over a direct connection with segmentation offload enabled and disabled. |
| `relay_conformance` trusted control | A local HTTPS relay with an explicitly trusted generated root carries the mutual QR handshake. IP transports and global address lookup are disabled. Assertions require actual relay paths. |
| `relay_conformance` certificate refusal | The same live relay is refused by a client that does not trust the fixture root. The test requires a certificate error and a successful trusted control. |
| `relay_conformance` relay loss | Killing the sole relay before the identity exchange produces a transport error or the existing 20-second frame timeout. Neither side obtains a verified peer or consumes a proof nonce. |
| `relay_conformance` HTTP admission | The callback sees identical new and deprecated endpoint headers. Its allow response permits connectivity. Its deny response prevents a fresh endpoint from becoming online. |
| `atomic transport_recovery::` | After a synthetic claim, the receiver durably records a signed report and closes before sending its application acknowledgment. After journal reopen and fresh endpoint keys, identical replay is recognized. All four human/machine modes preserve the financial projection, original claimed attempt, one report, and Held result. |

Run the direct and durable checks with the committed lockfile

```sh
cargo test -p hawala-iroh-bridge --locked --test transport_regressions
cargo test -p hawala-commit-sim --locked --test atomic transport_recovery::
```

The relay target is explicitly enabled because it launches external processes.
It requires OpenSSL and `HAWALA_TEST_RELAY_BIN` pointing to an Iroh 1.4.1 relay.
Missing prerequisites fail the enabled target. The dedicated
`Iroh transport regression checks` workflow downloads the release archive,
checks its pinned SHA-256, prints the binary version and runs every relay case.
Each case generates an ephemeral certificate and key. Certificate verification
stays enabled throughout.

```sh
export HAWALA_TEST_RELAY_BIN=/absolute/path/to/iroh-relay
cargo test -p hawala-iroh-bridge --locked --features relay-conformance \
  --test relay_conformance -- --test-threads=1
```

## Deployment acceptance still required

The local checks exercise a controlled Linux relay and loopback QUIC. Track
the following environment-dependent results separately when running a pilot.

| Environment | Acceptance check |
| --- | --- |
| Actual relay/DNS deployment | Record executable hash, build revision and dependency lock. Confirm the October TLS patches in each deployed artifact. The new self-signed fixture does not exercise an actual emSign distrust chain. |
| Android over cellular CGNAT | Repeat upload and signed observation traffic for both offload settings. Record device/OS/carrier, selected paths, path changes, throughput and bounded failure. Compare against upstream #4647 without assuming its workaround is needed on every device. |
| Dual-stack cellular and network handoff | Exercise IPv4/IPv6 under load, Wi-Fi-to-cellular handoff, UDP blocking and relay failure. Reconnect with a fresh QR binding and replay original observation/attempt IDs. Require a bounded application outcome and durable Held state after ambiguous delivery. |
| Relay-free LAN | Change the interface/address with WAN access blocked. Confirm the 1.4.0 address-refresh behavior through the chosen lookup provider. The current bridge accepts an established connection and supplies no deployed discovery service. |
| Self-hosted relay capacity | Lower the file-descriptor limit, exhaust connections and release descriptors. Require bounded CPU/logging and recovery. Upstream #4636 remains a reported availability concern. |

Iroh's stable 1.x wire compatibility allows mixed minor versions, subject to
application protocol compatibility. Public relay support for 0.9x and 1.0
release candidates ended September 30, 2026. The 0.35.x deadline is
December 31, 2026. Inventory mobile/FFI peers before a rollout.
See [upstream support and compatibility](https://www.iroh.computer/blog/v1).

The bridge continues to require completed TLS and the dedicated identity ALPN.
Private signed observations are accepted only after QR identity binding and
their existing report admission checks. Transport reconnection never creates
a new payout attempt. These tests operate on synthetic state.
