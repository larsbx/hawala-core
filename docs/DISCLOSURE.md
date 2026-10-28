# Disclosure policy for cross-authority payments (v1)

Status: **reference specification and pure implementation (`hawala-disclosure`);
no encryption transport.** Addresses ADR 0005 gap **G7**. A policy's digest
is the manifest's `permitted_disclosure_policy_digest`
([`CROSS_AUTHORITY_V1`](CROSS_AUTHORITY_V1.md)).

## Roles and field classes

| Class | What it covers |
| --- | --- |
| Correlation | `network_id`, `business_id`, `business_revision`, `attempt_id` |
| State | The typed payment state: `Proposed` … `Settled`, `Held`, `Aborted` |
| Parties | Coordinator key and epoch; each leg's authority key, scope and epoch, and payee key |
| Amounts | Each leg's currency, amount and fee |
| Destination | Each leg's allocation, obligation, funding lot and destination digests |
| Policy | Terms, consent, disclosure, finality, recovery and custody-profile digests; participant count; prepare deadline |
| Terms | The accepted dual-signed terms and participant consent objects themselves |
| Evidence | Signed evidence envelopes (kinds 1–15) |

`MANIFEST_FIELDS` classifies every manifest field in specification order. A
test parses the manifest block of `CROSS_AUTHORITY_V1.md` and fails if the
two diverge.

| Role | v1 minimal grant |
| --- | --- |
| Participant (consenting device), Coordinator, Authority | Everything |
| Payee (merchant or recipient business node) | Everything |
| LegAgent (execution observer, settlement attestor) | Everything (see the v1 finding) |
| StatusObserver (a non-party instance, such as a delivery node) | Correlation, State |
| RoutingPeer | Nothing |

## Rules every policy must satisfy

`validate` checks these rules, and both `decode` and `project` call it:

1. **Routing peers see nothing.** Financial messages are end-to-end
   encrypted to their recipient. A routing peer handles ciphertext and
   next-hop metadata only, both outside the record.
2. **Status observers see only Correlation and State.** They never see
   amounts, counterparties, destinations, terms or evidence. They verify
   nothing, so **they may display progress but never advance a workflow.**
   The thresholds in `CROSS_AUTHORITY_V1` §1 bind only verifying roles.
3. **Verifiability.** Every evidence envelope signs `H(manifest bytes)`. A
   role that receives Evidence must therefore receive every manifest class.
   A partial manifest cannot be verified, so granting one is refused.
4. **Signers see what they sign.** Participants, coordinators and
   authorities hold every class.

`project(policy, role, record)` returns only the items the role may see. Any
item it cannot classify (an unknown field, or a per-leg field without a leg)
rejects the whole projection.

Encoding: `"hawala-disclosure-policy/v1\0"` followed by one class bitmask
byte per role, in `Role` order. Bit *i* is the *i*-th class, so all eight
bits are assigned. The digest is SHA-256. The minimal profile's digest is
pinned in the tests: `94248705…15186ecf`.

## Finding: v1 forces leg agents to over-disclose

Rule 3 has a cost in v1. Because evidence signs the digest of the *whole*
manifest, an execution observer or settlement attestor for leg A must
receive leg B's payee key, amount, fee and destination to verify anything.
The same applies to a payee verifying its own settlement. v1 has no
selective disclosure for verifiers.

**Proposed v2 fix, not implemented, for the manifest's owners to decide:**
commit to each leg as `leg_digest_i = H(leg_i bytes)` and include only those
digests in the manifest. A leg agent then verifies with the global fields,
its own leg's bytes and the other leg's digest, and learns nothing else
about the other leg. Adding a per-field salt would also stop the digest of a
small field, such as an amount, being brute-forced. Until such a format
exists, deployments should treat leg agents as fully informed parties.

## Not covered

- The end-to-end encryption of routed messages that rule 1 presumes.
- Hiding Correlation identifiers from status observers. They are opaque,
  but they still link records.
- Disclosure of private books (`NODE_BOOKS_TIGERBEETLE_SPEC`).
- Public reputation aggregates (`secured-payment-protocol` §8).
