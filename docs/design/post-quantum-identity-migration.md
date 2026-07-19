# Post-quantum identity signature migration (issue #15, D-0095/D-0320)

**Status:** Phase 0 (research + design), Phase 1 (verify-only primitive in
`mini-crypto`), and Phase 2 (ML-DSA-65 key generation + isolated signing
in `mini-crypto`) shipped. Everything past that — KEL activation,
recovery/witness/device migration — is deferred.

**Full research:** `docs/research/
PQ15_POST_QUANTUM_MIGRATION_RESEARCH_20260715.md` (founder-supplied,
2026-07-15). This document does not reproduce that report — it records
what was actually built from it and why, and links back for the full
threat model, migration-option analysis, and phased rollout the report
itself lays out.

## Decision

`mini-crypto::SignatureSuite` already reserved wire tag `0x02` for
ML-DSA-65 (FIPS 204) as SPEC-01 §13's crypto-agility invariant's intended
migration target. This decision makes that primitive real, composing the
externally-maintained `fips204` crate rather than implementing ML-DSA's
lattice math in-house, per CLAUDE.md's no-new-cryptography rule. This is
**Phase 1 only** — parse and verify ML-DSA-65 keys/signatures. It does
**not** implement the research report's recommended dual-authorised KEL
hybrid-rotation protocol (Option C, §7-8), key generation, or any change
to `did-mini`'s identity/rotation logic. `SignatureSuite::DEFAULT` stays
`Ed25519`.

## What's implemented

- `SignatureSuite::MlDsa65` — a real, `#[non_exhaustive]`-compatible
  enum variant, wire tag `0x02`, with `public_key_len()` (1952 bytes) and
  `signature_len()` (3309 bytes) sourced from `fips204::ml_dsa_65`'s own
  constants rather than hand-copied numbers.
- `VerifyingKey`/`Signature` are now suite-polymorphic internally (a
  private `KeyMaterial` enum; `Vec<u8>` instead of a fixed `[u8; 64]`
  array for signature bytes) so they can hold either suite's key/signature
  material. `to_bytes()` on both types now returns `Vec<u8>` instead of a
  fixed-size array — audited every call site in the workspace before this
  change; all of them already treat the return value as a byte slice
  (`Writer::bytes()`/`extend_from_slice()`), so this was a compatible
  change everywhere except `did-mini::Controller`'s internal `SigningKey`
  storage, which needed its `#[derive(Clone)]` restored after the
  rewrite (caught by the full-workspace build, fixed same PR).
- `VerifyingKey::from_suite_bytes`/`verify` and `Signature::from_suite_bytes`
  handle `MlDsa65` end to end: parse real ML-DSA-65 public keys and
  signatures, verify a real signature produced by `fips204` itself against
  the parsed key and message, and correctly reject tampered signatures,
  wrong keys, wrong lengths, and suite mismatches between a key and a
  signature.
- **Phase 2 (D-0320):** `SigningKey::generate_ml_dsa_65()`/
  `sign_ml_dsa_65()` — real ML-DSA-65 key generation and signing in
  production code, composing `fips204`'s `try_keygen_with_rng`/
  `try_sign_with_rng` with `rand_core::OsRng` for entropy. Explicitly
  named, suite-specific methods alongside the existing Ed25519-only
  `generate()`/`sign()`, which stay completely unchanged. Secret
  zeroization on drop is structural (`fips204`'s own `ZeroizeOnDrop`
  derive), not reimplemented. No storage export/import for `MlDsa65`
  secrets, no benchmarks, no mobile/WASM testing — all named honestly as
  still-deferred below.

## Honest limit found during implementation

FIPS 204's public-key encoding is packed polynomial coefficients with no
additional structural validity check (unlike Ed25519, which rejects a
byte string that isn't a valid compressed curve point). An all-zero
"public key" of the correct length parses successfully — it simply never
verifies a real signature, since it corresponds to no real keypair. This
crate cannot add a stronger check without inventing its own validity
criterion outside FIPS 204, so the honest boundary is documented in
`keys.rs` and exercised by a test
(`an_all_zero_ml_dsa_65_key_parses_but_never_verifies_a_real_signature`)
rather than silently assumed.

## What's deferred

Everything the research report sequences as Phase 2's remaining items
onward:

- **Benchmarks and mobile/WASM testing** (still Phase 2) — no
  benchmarking harness or mobile toolchain exists in this environment.
- **ML-DSA-65 secret-key storage export/import** — not part of Phase 2's
  named scope, and `fips204`'s API gives no way to derive a `PublicKey`
  back from a raw seed the way Ed25519's `to_seed_bytes`/`from_seed`
  does, so a generated `MlDsa65` `SigningKey` today only lives for the
  process's lifetime.
- **KEL hybrid migration protocol** (Phase 3) — the actual identity
  migration mechanism: PQ pre-commitment via the existing next-key
  commitment, a dual-authorised rotation event signed by both the current
  Ed25519 key and the activated ML-DSA-65 key, downgrade prevention, and
  legacy-client stale-head handling. This is `did-mini`'s work, not
  `mini-crypto`'s, and is not started.
- **Recovery, delegated-device, and witness migration** (Phase 4).
- **Network opt-in and eventual default change** (Phase 5-8) — nothing
  here changes `SignatureSuite::DEFAULT`, and no future PR may change it
  before the report's own readiness gates (broad verifier support, mobile
  readiness, recovery readiness, **external cryptographic review**) pass.
- **ML-KEM-768 hybrid session establishment** — a separate track per the
  report; `mini-bearer`'s X25519 channel handshake is untouched.
- **SLH-DSA** — named as a future recovery/checkpoint option, not
  implemented.

## Hard rule: no production migration before external review

Per the research report §25 and CLAUDE.md's D-0047 external-audit gate:
no KEL activation, no default change, and no production use of
`SignatureSuite::MlDsa65` for real identity authority may land before an
external cryptographic review of the suite wrapper, the eventual hybrid
transition semantics, and the selected implementation. This decision adds
a verify-only primitive under active development scrutiny (this repo's
own review process) — that is not a substitute for the external review
gate real identity migration requires.

## What this does not provide

No KEL migration mechanism, no downgrade prevention, no legacy-client
capability negotiation, no recovery/device/witness migration path, and no
post-quantum confidentiality (ML-DSA is a signature scheme; `mini-bearer`'s
channel handshake remains X25519-only). See the research report's own
"Reject" list (§30) for migration approaches this decision explicitly
does not take.
