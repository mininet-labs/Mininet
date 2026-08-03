#!/usr/bin/env python3
"""Apply final API binding and truth-sync changes for PR #292.

Temporary branch-local helper. The verification job deletes this file in the
same tested commit that carries its changes.
"""

from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old!r}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


def append_once(path: str, marker: str, block: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    if marker in text:
        raise SystemExit(f"{path}: marker already present: {marker}")
    separator = "" if text.endswith("\n") else "\n"
    target.write_text(text + separator + block, encoding="utf-8")


# ---------------------------------------------------------------------------
# The signed advertisement and live channel proof must be one structural API,
# not a caller convention that can be forgotten after dialing a redirected
# endpoint.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-transport-security/src/auth.rs",
    "use crate::{ReplayCache, Result, TransportSecurityError};\n",
    "use crate::{ReplayCache, Result, TransportSecurityError, VerifiedPeerAdvertisement};\n",
)

replace_once(
    "crates/mini-transport-security/src/auth.rs",
    """    pub fn to_bytes(&self) -> Result<Vec<u8>> {
""",
    """    /// Verify this live CH1 proof against the exact signed endpoint the
    /// caller selected and dialed. This closes the discovery-to-session seam:
    /// a second genuine endpoint at a redirected address cannot substitute its
    /// own valid identity after the advertisement has been accepted.
    #[allow(clippy::too_many_arguments)]
    pub fn verify_advertised(
        &self,
        advertisement: &VerifiedPeerAdvertisement,
        expected_role: SessionRole,
        expected_purpose: TransportPurpose,
        channel_binding: &[u8; 32],
        now_ms: u64,
        root_kel: &Kel,
        device_kel: &Kel,
        freshness: &mut FreshnessPins,
        replay: &mut ReplayCache,
    ) -> Result<AuthenticatedPeer> {
        if advertisement.root() != &self.root || advertisement.device() != &self.device {
            return Err(TransportSecurityError::IdentityMismatch);
        }
        if advertisement.endpoint_id() != self.endpoint_id {
            return Err(TransportSecurityError::EndpointMismatch);
        }
        if advertisement.routing_key() != self.routing_key {
            return Err(TransportSecurityError::RoutingKeyMismatch);
        }
        self.verify(
            expected_role,
            expected_purpose,
            channel_binding,
            now_ms,
            root_kel,
            device_kel,
            freshness,
            replay,
        )
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
""",
)

# ---------------------------------------------------------------------------
# Bearer/network docs: anonymous CH1 remains deliberate, but optional endpoint
# authentication and signed discovery are no longer future-only claims.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-bearer/README.md",
    """A future upgrade can add endpoint *pseudonym* authentication (a SIGMA/Noise-XX
step keyed by a per-session pairwise pseudonym) without changing this crate's
shape or the anonymity property.
""",
    """D-0377 adds optional endpoint authentication in the separate
`mini-transport-security` crate without changing this anonymous base. A signed
`did:mini` root/device proof binds one role, typed purpose, and rotating X25519
routing key to this channel's 32-byte binding. Pairwise identities remain valid;
a caller that does not need a named peer keeps anonymous CH1 unchanged.
""",
)

replace_once(
    "crates/mini-bearer/README.md",
    """Real BLE and local-Wi-Fi/mDNS **radio** bearer adapters (device-specific,
need real hardware); a reliability/reassembly layer for bearers that drop
or reorder frames; a pairwise-pseudonym authenticated handshake variant;
rekeying for very long sessions. These build on the same trait and channel.
""",
    """Real BLE and local-Wi-Fi/mDNS **radio** bearer adapters (device-specific,
need real hardware); a reliability/reassembly layer for bearers that drop
or reorder frames; transcript camouflage/pluggable transports; rekeying for
very long sessions. These build on the same trait and channel. Endpoint
identity remains an optional layer above this crate, not a mandatory handshake
field that would destroy anonymous onion/mix hops.
""",
)

replace_once(
    "crates/mini-net/README.md",
    """## Honest limits
""",
    """## Secure discovery alternative (D-0377 proposal)

`mini-net::pex` remains a legacy unauthenticated availability exchange and must
never be treated as identity authority. `mini-transport-security` adds signed,
expiring, network-bound `PeerAdvertisement` records, a bounded
`SecurePexResponse`, and locally seeded prefix-diverse dial planning. The live
CH1 connection must then prove the same endpoint id and X25519 routing key via
`SessionAuthClaim::verify_advertised`; a signed record alone is still only a
dial hint.

This raises first-contact eclipse cost without creating a canonical bootstrap
list, certificate authority, hosted directory, trusted first peer, or
majority-by-download rule. It does not prove independent ASN/operator ownership
and does not implement NAT traversal.

## Honest limits
""",
)

# ---------------------------------------------------------------------------
# Planning document: implementation is complete in the draft; exact-head and
# human review remain the merge boundary.
# ---------------------------------------------------------------------------
replace_once(
    "docs/planning/privacy-transport-security.md",
    "**Status:** implementation in progress on `codex/privacy-transport-security`.\n",
    "**Status:** P0/P1 implementation complete in draft PR #292; no merge, release, production, or anonymity-certification claim until exact-head workflows and human review pass.\n",
)

replace_once(
    "docs/planning/privacy-transport-security.md",
    """## Merge floor
""",
    """## Implemented evidence

- `mini-transport-security` provides canonical bounded codecs for channel-bound
  delegated-device claims, signed peer advertisements, secure PEX responses,
  local-seeded prefix-diverse dial plans, and the runtime privacy-tier gate.
- `SessionAuthClaim::verify_advertised` structurally binds the signed endpoint
  selected during discovery to the identity/routing key proved by the live CH1
  transcript. A different genuine endpoint cannot replace it after redirect.
- `mini-relay` provides exactly three independent X25519/AEAD layers, distinct
  public hop identifiers, padded opaque next-hop tokens, per-hop expiry/replay
  checks, fixed-size destination payloads, and destination-only decryption.
- Permanent adversarial tests cover truncation, canonical re-encoding, wrong
  role/purpose/channel/network, KEL rollback/revocation, redirect, expiry,
  replay, duplicate/prefix concentration, wrong relay, route/key reuse,
  tampering, cross-hop identifier separation, and payload-size bounds.
- Real TCP tests prove mutual delegated-device authentication over one CH1 and
  three independent relay sockets forwarding only layered ciphertext before the
  destination opens the application payload.
- Focused evidence at commit `7009828`: 53 `mini-relay` unit tests, two relay
  real-socket tests, 11 `mini-transport-security` unit tests, one authenticated
  real-socket test, and strict Clippy all passed. Exact-head workspace evidence
  remains the merge gate.

## Merge floor
""",
)

# ---------------------------------------------------------------------------
# Governance record.
# ---------------------------------------------------------------------------
append_once(
    "docs/DECISION_LOG.md",
    "### D-0377 — Optional channel-bound peer authentication, secure PEX, and three-hop onion transport",
    r'''

### D-0377 — Optional channel-bound peer authentication, secure PEX, and three-hop onion transport  ·  *Proposed*

**Date:** 2026-08-03 · **Refs:** D-0015 (anonymous CH1), D-0301
(transport-policy tiers), D-0305 (Sphinx/Loopix research profile), D-0306
(`mini-relay` vocabulary), issues #291/#24/#27/#72, PR #292; Directives
2/5/6/16.

**Decision:** preserve `mini-bearer::Channel` as an anonymous,
forward-secret base and add optional self-certifying endpoint authentication in
a separate `mini-transport-security` crate. A `SessionAuthClaim` binds one exact
CH1 channel transcript to a `did:mini` root/device delegation, endpoint role,
typed purpose, rotating X25519 routing key, bounded validity window, and replay
nonce. Signed `PeerAdvertisement` records bind network id, dial address,
endpoint id, and the same routing key; `verify_advertised` requires the dialed
record and live channel proof to name the same endpoint. Local peer selection is
bounded, caller-seeded, input-order-independent, duplicate-resistant, and capped
per IPv4 `/24` or IPv6 `/48`.

For `PrivacyTier::Relayed`, extend `mini-relay` with exactly three independently
encrypted layers (`Entry -> Rendezvous -> Delivery`) around a destination-
encrypted fixed-size payload. Each public hop uses an independent random
connection id; next-hop tokens are opaque and padded; every layer binds role,
hop index, size class, ephemeral routing key, nonce, expiry, and replay token.
The compact format is explicitly not Sphinx and carries no global-observer
anonymity claim. `Mixed` and `Burst` remain runtime-fail-closed until the exact
D-0305 executor receives independent review under #72.

**Reason:** anonymous encryption alone does not authenticate a peer, unsigned
PEX permits redirect/eclipse attacks, and the previous live relay path exposed
application plaintext at the entry relay. Solving those concrete gaps must not
turn a certificate authority, hosted directory, canonical relay list, trusted
first peer, or administrative unmasking key into a new control point. Keeping
identity optional also prevents direct-session hardening from destroying
pairwise/anonymous onion and future mix hops.

**Constitutional impact:** strengthens Directive 2 by avoiding mandatory
operators; Directive 5 by keeping keys and identity proof local; Directive 6 by
making authenticated identity optional and pairwise-compatible; and Directive
16 by separating availability/routing from truth and governance. No balance,
payment, storage, bandwidth, provider revenue, or service metric enters route,
identity, personhood, validator, review, or governance authority. No admin,
law-enforcement, recovery, traffic-master, or escrowed unmasking key exists.

**Implementation status:** complete in draft PR #292. Permanent code covers
bounded canonical claims/advertisements/PEX, KEL rollback pins, delegated
capability checks, replay/expiry, structurally bound dial+session verification,
local prefix-diverse selection, the Direct/Relayed execution gate, and three-hop
destination-encrypted onion forwarding. Focused formatting, 66 unit tests, four
real-socket tests, and strict Clippy passed before truth sync; exact-head
workspace/governance/reproducibility/Android workflows remain the merge floor.

**Failure point:** a first-contact verifier cannot know about a later unseen KEL
revocation without witness/gossip freshness; IP-prefix diversity does not prove
independent ASN/operator/jurisdiction; public relay addresses remain blockable;
TCP transcript shape remains fingerprintable; NAT traversal/reconnect are absent;
and three-hop onion routing does not defeat global timing, volume,
intersection, predecessor, or congestion correlation. `Mixed`/`Burst` are not
operational and fail closed rather than inheriting a false anonymity label.

**Required follow-up:** complete #24's authenticated NAT traversal and relay
fallback; implement private bridge distribution and self-hostable pluggable
bearers from #27's review; add witness/gossip KEL freshness; build and externally
review the exact D-0305 Sphinx/Loopix executor under #72 before opening the
Mixed/Burst gate; run hostile-network and weakest-device measurements. None of
these may introduce a mandatory checkpoint, CA, bridge directory, cloud front,
or unmasking authority.

**Supersedes / superseded by:** tightens D-0015's former "future endpoint
authentication" status and extends D-0306 from relay vocabulary/hop-by-hop
envelopes to real layered execution. It does not supersede D-0305: the mixnet
profile remains separate and externally gated.
''',
)

# ---------------------------------------------------------------------------
# Living status and civilization threat model addenda.
# ---------------------------------------------------------------------------
append_once(
    "docs/STATUS.md",
    "## Privacy and transport security — D-0377 proposal",
    r'''

## Privacy and transport security — D-0377 proposal

- **implemented in PR #292** — optional channel-bound endpoint authentication:
  `SessionAuthClaim` proves one delegated `did:mini` device, typed purpose,
  endpoint role, and X25519 routing key on one exact anonymous CH1 transcript.
  Caller-held KELs, `FreshnessPins`, expiry, and bounded replay state verify the
  proof; `verify_advertised` also requires it to match the signed endpoint that
  was selected and dialed.
- **implemented in PR #292** — signed secure discovery: network-bound,
  expiring `PeerAdvertisement` records and bounded `SecurePexResponse` framing;
  locally seeded input-order-independent dial planning rejects duplicate
  endpoint/routing keys and caps IPv4 `/24` or IPv6 `/48` concentration.
  Records are availability hints, never truth or governance authority.
- **implemented in PR #292** — real Tier-1 onion execution: independent
  Entry/Rendezvous/Delivery X25519+AEAD layers, independent public hop ids,
  padded opaque routing tokens, per-hop expiry/replay checks, fixed-size
  destination-encrypted payloads, and a real three-socket convergence test.
  No relay receives application plaintext or both endpoint identities.
- **fail-closed** — `PrivacyTier::Mixed` and `Burst` have no operational
  executor. `mini_transport_security::executable_transport` refuses them until
  the exact D-0305 Sphinx/Loopix implementation receives #72's independent
  review. Policy vocabulary is not treated as implementation evidence.
- **open exact limits** — first-contact KEL freshness/witness gossip; independent
  ASN/operator/jurisdiction evidence; NAT traversal and reconnect; private
  bridge operations; pluggable/camouflaged bearers; ISP-throttling resistance;
  and global timing/volume/intersection protection. See
  `docs/audits/issue-27-censorship-resistance-review.md`.

**Authority boundary:** anonymous CH1 remains available; pairwise identities
remain valid; there is no CA, canonical relay/bootstrap registry, hosted
identity directory, trusted first peer, majority-by-download rule, admin or
unmasking key, or value-to-routing/voice path.
''',
)

append_once(
    "docs/THREAT_MODEL.md",
    "## Transport/privacy addendum — D-0377",
    r'''

## Transport/privacy addendum — D-0377

| Threat | Current mechanism | Status / exact failure |
|---|---|---|
| **Unsigned discovery redirect** | Signed, expiring, network-bound `PeerAdvertisement` plus `SessionAuthClaim::verify_advertised`, which binds the dialed record to the live CH1 identity/routing key. | **Closed for the verified path.** Legacy `mini-net::pex` remains unauthenticated and is documented as availability-only; callers that use it directly retain redirect risk. |
| **Endpoint impersonation on encrypted CH1** | Optional delegated-device signature over channel binding, role, typed purpose, pairwise/root identity, rotating X25519 key, expiry, and replay nonce. | **Closed within caller-supplied KEL freshness.** First-contact unseen revocation remains open until witness/gossip freshness exists. |
| **Bootstrap eclipse** | Caller-local seeded ordering, endpoint/routing-key deduplication, bounded timeouts, IPv4 `/24` and IPv6 `/48` caps. | **Partial.** One adversary can acquire diverse prefixes/ASNs or control all discovery sources; address diversity is not operator independence. |
| **Entry-relay plaintext exposure** | Three independent Entry/Rendezvous/Delivery X25519+AEAD layers around a destination-encrypted fixed-size payload. | **Closed for payload content.** Each relay still observes its local predecessor, timing, volume class, and opaque next-hop token. |
| **Cross-hop clear identifier correlation** | Every relay layer has an independent random public connection id; the destination id exists only inside destination encryption. | **Closed for explicit circuit ids.** Timing/volume correlation remains open. |
| **Public relay/bridge blocking** | No canonical relay registry; signed endpoints and local route rotation permit many independent relays. | **Partial.** Known TCP addresses remain enumerable and blockable; private bridge distribution and pluggable bearers are unbuilt. |
| **DPI/protocol fingerprinting** | Payload encryption and fixed destination size classes. | **Open/critical.** CH1 handshake, TCP framing, size classes, timing, and connection behavior remain recognizable; no camouflage profile exists. |
| **ISP throttling/blackholing** | Local BLE/Wi-Fi fallback and bounded timeouts. | **Open/critical.** Internet TCP can be delayed or dropped; multipath migration and progress-aware resumable routing are unbuilt. |
| **Global timing/volume/intersection observer** | Relayed payload separation only; Mixed/Burst runtime gate. | **Open/critical.** Three-hop onion is not a mixnet. The D-0305 Sphinx/Loopix executor and external review remain mandatory. |
| **Administrative unmasking/capture** | No traffic master key, escrow key, CA, hosted directory, canonical relay list, or identity-correlation service; pairwise identity is valid. | **Closed structurally in D-0377's types.** Future adapters must preserve this dependency wall. |

The complete state-censorship assessment and concrete mitigation sequence are in
`docs/audits/issue-27-censorship-resistance-review.md`. This addendum does not
claim NAT traversal, camouflage, or global anonymity exists.
''',
)

print("applied PR #292 stage-two binding and truth sync")
