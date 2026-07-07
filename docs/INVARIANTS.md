# Invariants — frozen/tunable register, mapped to code

This is the working mirror of the Constitution's canonical register (SPEC-00 §12).
The Constitution governs; if this file and SPEC-00 ever disagree, **SPEC-00 wins**
and this file is in error.

The sprint's Definition of Done requires that frozen invariants are *"encoded as
checks, not conventions."* The **Enforced by** column tracks exactly where each
one becomes code. `pending` means the owning crate/module is not in the tree yet.

## Tier F — Frozen (unamendable by any vote)

| # | Frozen invariant | Source | Enforced by |
|---|---|---|---|
| P1 | No balance maps to governance or validator vote weight | SPEC-00 P1 | `pending` — `gov-count` / chain module isolation |
| P2 | One verified human, one **equal** vote; early grants no extra | SPEC-00 P2 | partial — `did-mini` binds many devices to one human-root with capability scoping that **cannot** create extra votes (`Capabilities`, test `capabilities_are_a_narrowing_bitset`); the equal-vote **tally** itself is `pending` (nullifier, personhood + gov) |
| P3 | No owner/admin key; public-domain license; no off switch | SPEC-00 P3 | `LICENSE` (CC0); `pending` — genesis & release pipeline |
| P4 | Slow, presence-conditioned vesting; never a lump sum | SPEC-00 P4 | partial — `mini-reward` accrual is rate-capped per window, diversity-weighted, and maturation-delayed before vesting (tests `per_window_rate_cap_slows_accrual`, `contributions_vest_only_after_maturation`, `diversity_beats_repetition`); the on-chain vesting module is `pending` |
| P5 | No protocol requirement for raw personal data; ZK attestation only | SPEC-00 P5 | partial — `mini-crypto` keeps secrets on-device; `mini-bearer` gives an anonymous, forward-secret channel whose handshake carries no identity (test `distinct_sessions_have_distinct_bindings`, `handshake_agrees_on_binding_and_keys`); ZK personhood attestation still `pending` |
| P6 | No forced replication; no compelled decryption; device-only honored | SPEC-00 P6 | `pending` — storage fabric |
| — | Crypto-agility: no signature, DH, AEAD, or KDF algorithm hard-wired for life | SPEC-01 §13 + D-0014 | ✅ `mini-crypto::suite`, `::agreement`, `::aead`, `::kdf` (versioned suite tags for signatures, X25519, ChaCha20-Poly1305, HKDF-SHA256) |
| — | Strong-hash content addressing; never SHA-1 | SPEC-11 | ✅ `mini-crypto::hash` / `::multihash` (no SHA-1 variant; `0x11` rejected) |
| — | Keys never leave the device; no custodial recovery | SPEC-01 G1 | ✅ `mini-crypto::keys` / `::agreement` / `::aead` (export only via explicit methods; `Debug` redacts signing, DH, shared-secret, and AEAD key material) + `did-mini::Controller` (secrets never enter any wire form; `Debug` redacts) |
| — | Self-certifying identifier; no central registry to verify | SPEC-01 §3/G8 | ✅ `did-mini` (`Kel::verify` re-derives the SCID from inception; tests `scid_is_deterministic_and_self_certifying`, `tampered_identifier_is_rejected`) |
| — | Security-critical key events are pre-rotation-protected & anchored | SPEC-01 §16 | ✅ pre-rotation in `did-mini` (`Kel::verify` reveal check; test `rotation_reveals_precommitted_keys_and_verifies`). On-chain anchoring `pending` (chain) |
| — | Many devices provably one human; mutual, revocable, capability-scoped | SPEC-01 §6/G3 | ✅ `did-mini::verify_delegation` (device `dip` commits to root **and** root seals the device; both required); revocation + last-write-wins capabilities (tests `two_devices_one_human_with_capabilities`, `revocation_removes_a_device`, `device_claiming_wrong_root_is_rejected`) |
| — | Co-presence is range-bound and mutually signed; relay can't fake it | SPEC-02/SPEC-03 | partial — `mini-presence` requires proximity transport, delegated `ATTEST` device, distinct-key signatures, channel binding, fresh nonces, RTT under policy (tests `valid_presence_names_both_humans`, `revoked_device_is_rejected`, `non_proximity_and_range_failures_are_rejected`); a tighter BLE / Wi-Fi round-trip timing bound (no ranging radio; a software bound) is `pending` |
| — | Core software bootstrap and updates cannot rely on external services | SPEC-11 + D-0011 | partial — documented in `docs/BOOTSTRAP_AND_UPDATE.md`; code `pending` in `mini-bootstrap` / `mini-update` / release registry |
| — | Bluetooth-only identity + genesis/update chunk exchange must work with no internet | SPEC-03 keystone + D-0012 | partial — protocol documented in `docs/BOOTSTRAP_AND_UPDATE.md`; Pack 1 primitives in `mini-crypto::{agreement,kdf,aead}`; code `pending` in `mini-bearer` / `mini-bootstrap` |
| — | No forced auto-update / no off switch | SPEC-00 P3 + SPEC-11 | partial — update acceptance rules documented; code `pending` in `mini-update` |
| — | KEL/device-delegation wire decoders reject malformed, oversized, and ambiguous input before verification | SPEC-01 + D-0013 | ✅ `did-mini` decoder caps, SCID validation, strict multihash lengths, duplicate-key/threshold validation, unknown capability-bit rejection |
| — | Local encrypted channel primitives reject ambiguous or weak peer input | SPEC-03 + D-0014/D-0015 | ✅ `mini-crypto::agreement` rejects all-zero X25519 shared secrets and exact-length-checks public keys; `mini-crypto::aead` authenticates associated data; `mini-crypto::kdf` suite-tags HKDF; `mini-bearer::Channel` caps plaintext/ciphertext before crypto and rejects small-order handshakes |
| — | Public profiles/walls do not create privilege | SPEC-09 §6.1 + D-0033 | ✅ `mini-social::PublicWall` — publishing a `WALL`/`WALL_LINKAGE` object requires only `Capabilities::POST`, never `VOTE`; no wall registry exists, so an unknown wall is `None`, not a new registration (tests `public_wall_never_needs_or_implies_a_vote_capability`, `multiple_walls_are_unlinkable_by_default_and_unknown_walls_are_not_registered`) |
| — | Base devices do not create governance weight | SPEC-01 §6 + D-0033 | ✅ `did-mini::BaseDeviceRole` carries no `Capabilities` bit and cannot grant one (test `base_device_role_never_requires_or_implies_capabilities`); declaring a base device is advisory only |
| — | Storage/seeding earns value, never voice | SPEC-00 P1 + D-0033 | partial — `mini-store::CacheTier`/`Store::note_view` never touch `mini-forge`/`mini-reward` (no such crate dependency exists); the reward-side accrual for committed storage is `pending` (chain) |
| — | Seed-on-view is user-controlled and policy-bound | SPEC-00 P6 + D-0033 | ✅ `mini-store::Store::note_view` — disabled by `BaseDeviceRole::seed_on_view_enabled`, gated by battery/availability-window policy and by metered-connection/storage-budget conditions; encrypted content is never promoted past `CacheTier::PrivateOnly` (tests in `crates/mini-store/tests/cache.rs`) |
| — | Radio/LoRa is not part of Mininet | D-0009 (amended) + D-0033 | ✅ documentation-enforced: the connectivity core is BLE + local Wi-Fi/hotspot/mDNS + optional internet relay + store-and-forward/DTN sync, permanently — no radio/LoRa bearer exists or is planned |
| — | Core implementation language is Rust | D-0001 + D-0008 | ✅ the entire workspace (`Cargo.toml` members) is Rust; the future chain adapts proven BFT concepts in Rust, not Go/Cosmos |
| — | AI may draft sensitive code, but human review is mandatory | SPEC-11 §2 + D-0033 | partial — `mini-forge::governance::PROTOCOL_MIN_APPROVALS` / `valid_policy_for_protocol_repo` enforce a 2-approval floor with no 1-of-1 canonical merge path for protocol-critical repos; a dedicated "AI-assisted" flag on commits/PRs is `pending` |

## Tier T — Tunable within limits (one-human-one-vote + timelock + bounds-check)

These are *parameters*, changeable only within frozen floors/ceilings. Recorded
here so no module silently treats one as frozen or unbounded.

- Current **default signature/DH/AEAD/KDF suites** (must remain migratable) — see D-0003 and D-0014.
- Content-address default algorithm (within the strong-hash set) — see D-0004.
- Personhood thresholds / decay; verification tier rates / dwell windows / K
  attesters (within frozen safety floors) — `pending`.
- Reward-curve constants; fee value targets; epoch length; committee size;
  timelock durations; treasury signer-set size — `pending` (chain).
- Pinned toolchain version; K independent builders (within a frozen minimum) —
  see D-0006.

## Tier O — Organic (permissionless; no vote)

App surfaces, feed-ranking plugins, client software, new bearers, new storage
clients, new application modules, moderation filter lists. Constrained only in
that they may not cause a Tier-F violation.

---

### How to use this file in review

When a PR adds or changes a frozen-domain behavior, it should:
1. Point to the SPEC-00 §12 line it implements.
2. Move the relevant **Enforced by** cell from `pending` to the concrete
   module path, ideally with a test name.
3. Add a `D-00xx` decision-log entry if a \[FREEZE\] choice was made.

A frozen invariant should be impossible to express in code (Layer 1) wherever we
can manage it, and rejected on validation (Layer 2) everywhere else — never left
to a reviewer's memory.
