#!/usr/bin/env python3
"""Apply the permanent fixes requested by PR #292 review.

The validating workflow deletes this helper before committing the resulting
source and documentation changes.
"""

from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")


def replace_exact(path: str, old: str, new: str, expected: int = 1) -> None:
    text = read(path)
    count = text.count(old)
    if count != expected:
        raise SystemExit(
            f"{path}: expected {expected} matches, found {count}: {old[:120]!r}"
        )
    write(path, text.replace(old, new))


def insert_before(path: str, marker: str, block: str) -> None:
    text = read(path)
    count = text.count(marker)
    if count != 1:
        raise SystemExit(f"{path}: expected one marker, found {count}: {marker!r}")
    write(path, text.replace(marker, block + marker, 1))


advertisement = "crates/mini-transport-security/src/advertisement.rs"
auth = "crates/mini-transport-security/src/auth.rs"
selection = "crates/mini-transport-security/src/selection.rs"
advertised_binding = "crates/mini-transport-security/tests/advertised_binding.rs"
authenticated_tcp = "crates/mini-transport-security/tests/authenticated_tcp.rs"
audit = "docs/audits/issue-27-censorship-resistance-review.md"
planning = "docs/planning/privacy-transport-security.md"

# The public issuance APIs own nonce generation. A normal caller can no longer
# accidentally copy deterministic fixture bytes into a security-critical replay
# nonce. Entropy failures propagate through the existing Crypto error variant.
replace_exact(
    advertisement,
    """        issued_at_ms: u64,
        expires_at_ms: u64,
        nonce: [u8; 32],
    ) -> Result<Self> {
""",
    """        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> Result<Self> {
""",
)
replace_exact(
    advertisement,
    """        validate_address(address)?;
        validate_window(issued_at_ms, expires_at_ms, issued_at_ms)?;
        let endpoint_id = TransportEndpointId::derive(&device.did(), &routing_key);
""",
    """        validate_address(address)?;
        validate_window(issued_at_ms, expires_at_ms, issued_at_ms)?;
        let nonce = mini_crypto::random_32()?;
        let endpoint_id = TransportEndpointId::derive(&device.did(), &routing_key);
""",
)
replace_exact(
    advertisement,
    "        replay.check_and_record(self.replay_id())?;\n",
    """        replay.check_and_record(self.replay_id(), self.expires_at_ms, now_ms)?;
""",
)
replace_exact(advertisement, "            [8; 32],\n", "", expected=3)
replace_exact(selection, "            [seed + 5; 32],\n", "")
replace_exact(advertised_binding, "        [30; 32],\n", "")

replace_exact(
    auth,
    """        issued_at_ms: u64,
        expires_at_ms: u64,
        nonce: [u8; 32],
    ) -> Result<Self> {
""",
    """        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> Result<Self> {
""",
)
replace_exact(
    auth,
    """        validate_window(issued_at_ms, expires_at_ms, issued_at_ms)?;
        let endpoint_id = TransportEndpointId::derive(&device.did(), &routing_key);
""",
    """        validate_window(issued_at_ms, expires_at_ms, issued_at_ms)?;
        let nonce = mini_crypto::random_32()?;
        let endpoint_id = TransportEndpointId::derive(&device.did(), &routing_key);
""",
)
replace_exact(
    auth,
    "        replay.check_and_record(self.replay_id(channel_binding))?;\n",
    """        replay.check_and_record(
            self.replay_id(channel_binding),
            self.expires_at_ms,
            now_ms,
        )?;
""",
)
replace_exact(auth, "            [9; 32],\n", "", expected=3)
replace_exact(auth, "            [10; 32],\n", "")
replace_exact(advertised_binding, "        [60; 32],\n", "")
replace_exact(advertised_binding, "        [61; 32],\n", "")
replace_exact(authenticated_tcp, "            [91; 32],\n", "")
replace_exact(authenticated_tcp, "        [90; 32],\n", "")

# Permanent regression tests prove both issue APIs independently obtain fresh
# OS randomness even when every caller-supplied field is identical.
advertisement_test = r'''    #[test]
    fn issue_generates_fresh_nonce_internally() {
        let (root, device) = identity(10);
        let routing = AgreementSecretKey::from_seed(&[20; 32]).public_key();
        let first = PeerAdvertisement::issue(
            [7; 32],
            &root.did(),
            &device,
            routing,
            "127.0.0.1:9000".parse().unwrap(),
            1_000,
            2_000,
        )
        .unwrap();
        let second = PeerAdvertisement::issue(
            [7; 32],
            &root.did(),
            &device,
            routing,
            "127.0.0.1:9000".parse().unwrap(),
            1_000,
            2_000,
        )
        .unwrap();

        assert_ne!(first.nonce, second.nonce);
        assert_ne!(first.replay_id(), second.replay_id());
    }

'''
insert_before(
    advertisement,
    "    #[test]\n    fn signed_advertisement_round_trips_and_verifies() {\n",
    advertisement_test,
)

auth_test = r'''    #[test]
    fn issue_generates_fresh_nonce_internally() {
        let (root, device) = identity();
        let routing = AgreementSecretKey::from_seed(&[8; 32]).public_key();
        let binding = binding();
        let first = SessionAuthClaim::issue(
            &root.did(),
            &device,
            SessionRole::Initiator,
            TransportPurpose::Relay,
            routing,
            &binding,
            1_000,
            2_000,
        )
        .unwrap();
        let second = SessionAuthClaim::issue(
            &root.did(),
            &device,
            SessionRole::Initiator,
            TransportPurpose::Relay,
            routing,
            &binding,
            1_000,
            2_000,
        )
        .unwrap();

        assert_ne!(first.nonce, second.nonce);
        assert_ne!(first.replay_id(&binding), second.replay_id(&binding));
    }

'''
insert_before(
    auth,
    "    #[test]\n    fn claim_round_trips_and_authenticates_the_exact_session() {\n",
    auth_test,
)

# Point camouflage work at the existing pluggable-transport crate rather than
# creating a second transport authority or duplicate process manager.
replace_exact(
    audit,
    "| Protocol fingerprinting and DPI resistance | **FAIL** | CH1 and onion payloads are encrypted, but handshake lengths, TCP framing, packet-size classes, timing, and connection behavior remain recognizable. Encryption does not equal camouflage. | Implement a self-hostable pluggable-bearer interface with independently reviewed padding/camouflage profiles, randomized handshakes, and ordinary-protocol-shaped adapters. Avoid dependence on one commercial domain-fronting provider. |",
    "| Protocol fingerprinting and DPI resistance | **FAIL** | CH1 and onion payloads are encrypted, but handshake lengths, TCP framing, packet-size classes, timing, and connection behavior remain recognizable. Encryption does not equal camouflage. | Extend the existing `mini-bridge` pluggable-transport interface with independently reviewed padding/camouflage profiles, randomized handshakes, and ordinary-protocol-shaped adapters. Avoid dependence on one commercial domain-fronting provider. |",
)
replace_exact(
    audit,
    """**Required design:** pluggable bearer adapters with explicit transcript-shape
profiles, bounded padding, timing jitter, and independent review. Camouflage
must remain optional and swappable; one cloud/CDN front is not acceptable as a
new root dependency.
""",
    """**Required design:** implement explicit transcript-shape profiles, bounded
padding, timing jitter, and independently reviewed adapters through the existing
`mini-bridge` pluggable-transport process manager. Camouflage must remain
optional and swappable; one cloud/CDN front is not acceptable as a new root
dependency, and `mini-transport-security` must not duplicate bridge management.
""",
)
replace_exact(
    planning,
    """- NAT traversal, reconnect, bridge/pluggable transports, background daemon
  supervision, and hostile-country censorship measurements remain separate
  deployment work.
""",
    """- NAT traversal, reconnect, background daemon supervision, and hostile-country
  censorship measurements remain separate deployment work. The existing
  `mini-bridge` crate is the required home for pluggable transport and camouflage
  adapters; extend and independently review it rather than duplicating bridge
  management in `mini-transport-security`.
""",
)
replace_exact(
    planning,
    """- `mini-transport-security` provides canonical bounded codecs for channel-bound
  delegated-device claims, signed peer advertisements, secure PEX responses,
  local-seeded prefix-diverse dial plans, and the runtime privacy-tier gate.
""",
    """- `mini-transport-security` provides canonical bounded codecs for channel-bound
  delegated-device claims, signed peer advertisements, secure PEX responses,
  local-seeded prefix-diverse dial plans, and the runtime privacy-tier gate.
  Both public issue APIs generate replay nonces internally from the OS CSPRNG;
  callers cannot accidentally supply deterministic fixture bytes.
""",
)

print("applied PR #292 review fixes")
