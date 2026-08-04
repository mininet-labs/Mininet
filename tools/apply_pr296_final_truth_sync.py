#!/usr/bin/env python3
"""Final truth-sync for PR #296: state exact residual limits and real merge gates."""
from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:140]!r}")
    p.write_text(text.replace(old, new), encoding="utf-8")


planning = "docs/planning/privacy-transport-runtime-convergence.md"
replace_once(
    planning,
    """`AuthenticatedConnection`. The provider pseudonym is domain-separated from the
verified `TransportEndpointId`, so routing-key rotation also rotates the label.
""",
    """`AuthenticatedConnection`. The provider pseudonym is domain-separated from the
verified `TransportEndpointId` and exact CH1 binding, so routing-key rotation or
opening a new channel rotates the label.
""",
)
replace_once(
    planning,
    """| Authenticated F6 provider labeling | **PASS for the named API** | Typed `SearchQuery` proof, private `AuthenticatedQueryResults` fields, channel-scoped endpoint+CH1 provider pseudonym, and sealed merge path. | Anonymous/legacy APIs intentionally retain caller-owned labeling; provider identity does not prove result truth or continuity across sessions. |
""",
    """| Authenticated F6 provider labeling | **PASS for the named API** | Typed `SearchQuery` proof, private `AuthenticatedQueryResults` fields, channel-scoped endpoint+CH1 provider pseudonym, and sealed merge path. | Anonymous/legacy APIs intentionally retain caller-owned labeling; provider identity does not prove result truth or continuity across sessions. |
| Provider authentication without requester identity | **FAIL** | Anonymous F6 remains available, and the named path can use a pairwise requester identity. | The current `AuthenticatedConnection` exchange is mutual: obtaining peer-bound provider provenance also discloses a requester identity. No server-only authenticated F6 connection exists. |
""",
)
replace_once(
    planning,
    """| Relay role separation at route build | **PARTIAL** | Three live, same-network verified records must differ by endpoint id, routing key, visible root, and device; the destination key must differ from every relay routing key. | One hidden operator can control several valid pairwise roots, devices, prefixes, or ASNs. |
""",
    """| Relay role separation at route build | **PARTIAL** | Three live, same-network verified records must differ by endpoint id, routing key, visible root, and device; the destination key must differ from every relay routing key. | One hidden operator can control several valid pairwise roots, devices, prefixes, or ASNs. The builder accepts a caller-supplied destination key and does not itself prove a destination identity. |
""",
)
replace_once(
    planning,
    """| Relay and destination replay defense | **PASS in-process** | Onion v2 uses v2 key domains, encrypts expiry/replay tokens for every relay and destination, bounds lifetime with explicit clock-skew tolerance, retains a monotonic local time high-water mark so wall-clock rollback cannot resurrect expired tokens, never evicts live entries, records only after inner validation, and fails closed at capacity. | Hosts must persist equivalent replay state and its time high-water mark across restart; authenticated packet floods can still exhaust bounded capacity and require rate/resource controls. |
""",
    """| Relay and destination replay defense | **PASS in-process** | Onion v2 uses v2 key domains, encrypts expiry/replay tokens for every relay and destination, bounds lifetime with explicit clock-skew tolerance, retains a monotonic local time high-water mark so wall-clock rollback cannot resurrect expired tokens, never evicts live entries, records only after inner validation, and fails closed at capacity. | The concrete replay caches expose no persistence/import API, so restart durability is unimplemented—not merely deployment configuration. Authenticated floods can still exhaust bounded capacity. |
""",
)
replace_once(
    planning,
    """- Named F6 proves endpoint control on one exact channel, not index honesty.
  Provider labels intentionally rotate across channels; privacy-preserving
  durable continuity remains undesigned.
""",
    """- Named F6 proves endpoint control on one exact channel, not index honesty.
  Provider labels intentionally rotate across channels; privacy-preserving
  durable continuity remains undesigned. The named exchange is mutual, so there
  is no provider-authenticated/requester-anonymous mode yet; use a pairwise
  requester identity or the explicitly weaker anonymous path.
- `build_verified_onion_route` verifies relay records, not the caller-supplied
  destination key's identity. Endpoint destinations need a future typed wrapper
  if that binding becomes a product requirement.
- `ReplayCache` and `OnionReplayCache` are in-memory concrete types with no
  persistence/export surface. Restart-surviving replay protection needs a
  reviewed persistent cache design rather than a documentation instruction.
""",
)
replace_once(
    planning,
    """pass formatting, strict Clippy, complete workspace tests, dependency policy,
governance, reproducibility, Android, Android reproducibility, CodeQL, and
navigation checks. Human approval is mandatory; AI-authored code and evidence
carry zero approval weight.
""",
    """pass formatting, strict Clippy, complete workspace tests, dependency policy,
governance, reproducibility, Android, Android reproducibility, and navigation
checks. No CodeQL workflow exists in this repository, so this PR makes no CodeQL
claim. Any future hosted or self-hosted scanner is supplemental evidence, never
review authority. Human approval is mandatory; AI-authored code and evidence
carry zero approval weight.
""",
)

readme = "crates/mini-transport-security/README.md"
replace_once(
    readme,
    """- `build_verified_onion_route` accepts three live same-network verified endpoints and
  rejects visible endpoint, routing-key, root, or device reuse before building;
  the lower onion constructor also rejects using any relay routing key as the
  destination key, so no relay can become the destination by caller mistake,
  then builds the `Entry -> Rendezvous -> Delivery` onion in `mini-relay`.
  Permanent integration tests start with signed advertisements and local selection, then
""",
    """- `build_verified_onion_route` accepts three live same-network verified
  endpoints and rejects visible endpoint, routing-key, root, or device reuse.
  The lower onion constructor also rejects using any relay routing key as the
  destination key, so no relay becomes the destination by caller mistake, then
  builds the `Entry -> Rendezvous -> Delivery` onion in `mini-relay`. The
  destination key itself remains caller-supplied rather than identity-verified.
  Permanent integration tests start with signed advertisements and local selection, then
""",
)
replace_once(
    readme,
    """- Endpoint authentication protects identity binding, not traffic shape or IP
  metadata. It can increase linkability when a global root is presented.
""",
    """- Endpoint authentication protects identity binding, not traffic shape or IP
  metadata. It can increase linkability when a global root is presented.
  Authentication is mutual: responder-first ordering protects the initiator from
  a redirect, but any initiator reaching the service receives the responder's
  named proof before proving itself. Use pairwise service identities where that
  disclosure matters. No server-only authenticated connection exists yet.
""",
)
replace_once(
    readme,
    """  defeat a global timing/volume observer; crash persistence and flood controls
  remain deployment responsibilities.
""",
    """  defeat a global timing/volume observer. The concrete replay caches are
  in-memory and expose no persistence/import API; crash persistence is therefore
  unimplemented, while flood controls remain a deployment responsibility.
""",
)

f6 = "docs/design/f6-private-query-transport.md"
replace_once(
    f6,
    """- The anonymous `remote_query`/`serve_query` and legacy
  `merge_remote_results` APIs remain available. Identity disclosure is optional,
  not made mandatory for search.
""",
    """- The anonymous `remote_query`/`serve_query` and legacy
  `merge_remote_results` APIs remain available. Identity disclosure is optional
  for search as a whole, but the authenticated provider-provenance path uses the
  mutual `AuthenticatedConnection` exchange. There is no mode that proves only
  the provider while leaving the requester entirely unnamed.
""",
)
replace_once(
    f6,
    """privacy-conscious continuity design. The anonymous legacy path can still be
caller-mislabeled because that is its explicit contract.
""",
    """privacy-conscious continuity design. The anonymous legacy path can still be
caller-mislabeled because that is its explicit contract. The named path also
requires requester authentication: pairwise identity limits linkage, but true
server-only provider authentication is not implemented.
""",
)
replace_once(
    f6,
    """- Decide whether a future privacy-preserving continuity proof should link rotating authenticated provider labels without turning one global provider identity into a tracking or ranking authority.
""",
    """- Design a server-only authenticated connection for callers that need peer-bound provider provenance without disclosing a requester identity; it must preserve anonymous CH1 and avoid a CA or global service identity requirement.
- Decide whether a future privacy-preserving continuity proof should link rotating authenticated provider labels without turning one global provider identity into a tracking or ranking authority.
""",
)

threat = "docs/THREAT_MODEL.md"
replace_once(
    threat,
    """| **Redirected genuine endpoint collecting initiator identity** | Runtime authentication is responder-first: the dialed peer proves the exact advertised endpoint before the initiator sends its claim. | **Closed for named runtime sessions.** The contacted address still observes source-IP/network metadata. |
""",
    """| **Redirected genuine endpoint collecting initiator identity** | Runtime authentication is responder-first: the dialed peer proves the exact advertised endpoint before the initiator sends its claim. | **Closed for named runtime sessions.** The contacted address still observes source-IP/network metadata; responder-first also exposes the responder's named proof to any initiator that can reach it. |
""",
)
replace_once(
    threat,
    """| **Relay-cache eviction or clock rollback re-enabling replay** | Onion v2 uses separate v2 cryptographic domains, stores `(token, expiry)` through a clock-skew-bounded encrypted window, advances a monotonic local time high-water mark, prunes only expired entries, records only after the whole local inner structure validates, and fails closed at capacity. | **Closed in-process.** Restart persistence of both tokens and the time high-water mark, plus flood/rate controls, remain host responsibilities. |
""",
    """| **Relay-cache eviction or clock rollback re-enabling replay** | Onion v2 uses separate v2 cryptographic domains, stores `(token, expiry)` through a clock-skew-bounded encrypted window, advances a monotonic local time high-water mark, prunes only expired entries, records only after the whole local inner structure validates, and fails closed at capacity. | **Closed in-process.** The concrete cache has no persistence/import surface, so restart durability is unimplemented; flood/rate controls are also absent. |
""",
)
replace_once(
    threat,
    """| **Post-delivery envelope replay** | Onion v2 puts a separate expiry and replay token inside destination encryption; `open_onion_destination` requires time and replay state. | **Closed in-process.** A destination that discards replay state on restart loses that guarantee. |
""",
    """| **Post-delivery envelope replay** | Onion v2 puts a separate expiry and replay token inside destination encryption; `open_onion_destination` requires time and replay state. | **Closed in-process.** Restart-safe protection cannot be claimed until a persistent replay-cache API exists. |
""",
)
replace_once(
    threat,
    """| **Forged F6 provider label after authenticated query** | `SearchQuery`-purpose `AuthenticatedConnection`, channel-scoped endpoint+CH1 provider pseudonym, private `AuthenticatedQueryResults` fields, and `merge_authenticated_remote_results`. | **Closed for the named API.** Anonymous/legacy merge is intentionally caller-labeled; endpoint control does not prove result truth or cross-session continuity. |
""",
    """| **Forged F6 provider label after authenticated query** | `SearchQuery`-purpose `AuthenticatedConnection`, channel-scoped endpoint+CH1 provider pseudonym, private `AuthenticatedQueryResults` fields, and `merge_authenticated_remote_results`. | **Closed for the named API.** Anonymous/legacy merge is intentionally caller-labeled; endpoint control does not prove result truth or cross-session continuity. |
| **Requester identity disclosure when authenticating an F6 provider** | A caller can use a pairwise identity, or choose anonymous F6 without peer-bound provenance. | **Open.** The authenticated F6 path is mutual; no server-only proof lets an entirely unnamed requester authenticate the provider. |
""",
)

log = "docs/DECISION_LOG.md"
replace_once(
    log,
    """`AuthenticatedConnection`, derives a rotating `ProviderPseudonym` from the
verified `TransportEndpointId`, returns an `AuthenticatedQueryResults` with
""",
    """`AuthenticatedConnection`, derives a rotating `ProviderPseudonym` from the
verified `TransportEndpointId` plus exact CH1 binding, returns an
`AuthenticatedQueryResults` with
""",
)
replace_once(
    log,
    """Known TCP endpoints remain blockable/fingerprintable; NAT traversal, reconnect,
private bridge distribution, multipath migration, and real camouflage adapters
are absent. The three-hop onion remains correlatable by a global timing/volume
observer; Mixed/Burst stay closed.
""",
    """Known TCP endpoints remain blockable/fingerprintable; NAT traversal, reconnect,
private bridge distribution, multipath migration, and real camouflage adapters
are absent. The named F6 path is mutual authentication—there is no server-only
provider proof for an unnamed requester. Relay selection verifies relay records,
not the caller-supplied destination key's identity. Replay caches are in-memory
concrete types with no persistence/import API. The three-hop onion remains
correlatable by a global timing/volume observer; Mixed/Burst stay closed.
""",
)
replace_once(
    log,
    """private bridge distribution and reviewed adapters under the existing
`mini-bridge` boundary; privacy-conscious provider continuity if a real product
needs cross-rotation reputation; and the externally reviewed D-0305 mix executor
""",
    """private bridge distribution and reviewed adapters under the existing
`mini-bridge` boundary; a server-only authenticated connection for anonymous F6
requesters; a persistent replay-cache design; an optional typed destination-
identity wrapper; privacy-conscious provider continuity if a real product needs
cross-rotation reputation; and the externally reviewed D-0305 mix executor
""",
)
replace_once(
    log,
    """dependency, governance, reproducibility, Android, CodeQL, and human-review checks
remain the merge floor.
""",
    """dependency, governance, reproducibility, Android, and human-review checks
remain the merge floor. No CodeQL workflow exists in the repository, so no
CodeQL result is claimed.
""",
)

print("PR 296 final honest-limits truth sync applied")
