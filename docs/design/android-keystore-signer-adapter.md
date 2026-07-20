# Android Keystore signer adapter + device delegation ceremony (issue #197, D-0334)

**Status:** Phase 0 (design only). No code in this PR. Follows the same
discipline used for `docs/design/kel-witness-receipts-and-duplicity-gossip.md`
Phase 0 and `docs/design/post-quantum-identity-migration.md`: freeze what an
opaque device signer is and is not allowed to do before writing the trait,
so the crate holding secret material (`mini-crypto`) doesn't get a rushed
change under beta-deadline pressure.

**Tracking:** hub issue #196 (Android beta roadmap), slice issue #197, draft
PR #179 (Android app foundation this stacks on top of — `docs/mobile/
ANDROID_FOUNDATION.md` and `mini-ffi`/`app/android/` don't exist on `main`
yet, only on that branch).

## The gap this closes

`docs/mobile/ANDROID_FOUNDATION.md`'s custody model already states the target
plainly: "each phone holds a separate revocable device key... private-key
bytes remain inside the custody adapter and are never returned through
UniFFI." Today nothing in this workspace can satisfy that for real: every
delegated device controller (`did_mini::Controller::incept_device`) is
constructed from a `Vec<mini_crypto::SigningKey>`, and `SigningKey` always
holds real secret bytes in-process (`SecretKeyMaterial::Ed25519(Box<
DalekSigningKey>)` or `SecretKeyMaterial::MlDsa65{secret, ..}` —
`crates/mini-crypto/src/keys.rs`). There is no path today for a device's
signing key to live somewhere `mini-ffi`/Rust never sees the raw bytes at
all, which is what Android Keystore's non-exportable hardware-backed keys
are for.

## What's actually true about `Controller`'s pre-rotation design (good news)

Read closely, `Controller::incept_inner` commits to the **next** key set by
hashing only the *verifying* key (`event::key_commitment(&k.verifying_key())`
— `crates/did-mini/src/controller.rs:194`), never the secret scalar.
KERI-style pre-rotation "reveals" a committed key at the next rotation by
disclosing its public half was already fixed; it never needed the secret
bytes to leave storage for that purpose. Concretely: **the mechanism that
makes an opaque signer possible already exists in `Controller`'s design** —
nothing about establishment/rotation event construction ever needs to read a
private scalar back out, only `.verifying_key()` and a signature over the
event bytes. This means an opaque signer adapter is a much smaller structural
change than it first looks: `Controller` doesn't need a redesign, only a way
to hold *something that can sign and report its public key* instead of
requiring a `SigningKey` value specifically.

## What genuinely cannot work with an opaque signer (and why that's fine)

- `SigningKey::to_seed_bytes()` and `Controller::incept_pairwise_pseudonym`
  (`crates/did-mini/src/controller.rs:107-132`) both require reading the raw
  32-byte Ed25519 seed back out to HKDF-derive a child root. This is
  fundamentally incompatible with a non-exportable key by construction — an
  opaque signer cannot support pairwise-pseudonym derivation, full stop.
  This is not a blocker: pairwise pseudonyms are a **root** operation
  ("this human runs many pseudonym identities from one root"), never
  performed by a delegated **device** controller. The device's own
  `Controller` (created via `incept_device`) never calls
  `incept_pairwise_pseudonym` on itself in any existing call site. The
  constraint should be enforced structurally (an opaque-signer-backed
  controller simply has no `to_seed_bytes`/`incept_pairwise_pseudonym`
  available to call), not left as a runtime panic waiting to be hit.
- `Controller::rotate_with_next`/`recover_from_kel` accept caller-supplied
  key material directly; an opaque-signer-backed device would need its own
  narrower rotation entry point that asks the platform adapter for a
  *freshly generated* next key's public half (Android Keystore can generate
  a new key pair and hand back only the public key immediately — no export
  required) rather than accepting `Vec<SigningKey>`.

## The open question this Phase 0 does not resolve: which signature suite

Android Keystore's **hardware-backed** (StrongBox/TEE) key generation has
historically had inconsistent Ed25519 support across devices and API
levels — unlike NIST P-256/P-384 and RSA, which every Android Keystore HAL
has supported since API 23. `SignatureSuite` in `mini-crypto` today only
has two variants: `Ed25519` (default, signing) and `MlDsa65` (verify-only,
Phase 1/2 of the PQ migration, D-0322). Composing Android Keystore for a
device key means one of:

1. **Extend `SignatureSuite` with a signing-capable EC suite** (e.g.
   P-256/ECDSA, an already-standardized, already-reviewed primitive — no new
   cryptography per CLAUDE.md) specifically for opaque hardware-backed
   device keys, verified against Ed25519 elsewhere unchanged; or
2. **Accept Ed25519-only Keystore support** where the target API level/HAL
   actually offers it (API 33+ on some devices), and fail closed (report no
   hardware backing, per `ANDROID_FOUNDATION.md`'s existing honesty rule)
   everywhere else; or
3. **Software-only Keystore wrapping**: generate the Ed25519 device key
   in-process as today, but have Android Keystore wrap/encrypt it at rest
   (a symmetric AES key inside Keystore protects the Ed25519 secret on
   disk) rather than performing the signing operation in hardware at all.
   This is not what "hardware-backed signing key" means and must never be
   reported as such — `ANDROID_FOUNDATION.md` already says as much
   ("Hardware backing must be proven by the future Keystore adapter from
   the generated key's security properties; it must not be inferred").

This is a founder-facing crypto-suite decision, not something to pick
unilaterally in a follow-up implementation PR without it being named here
first. Option 3 is the safest default (matches Ed25519 everywhere, no suite
change, defers true hardware-backed signing) but under-delivers on the
"non-exportable phone device key" acceptance criterion literally — a
software-wrapped key is still exportable in principle if the wrapping key
is ever compromised, whereas a Keystore-generated signing key genuinely
never leaves hardware. Recorded here as open; not decided.

## Proposed shape for the next (real code) PR

- A `did_mini::DeviceSigner` trait (name provisional): `fn verifying_key(&self)
  -> VerifyingKey`, `fn sign(&self, msg: &[u8]) -> Signature`, `fn suite(&self)
  -> SignatureSuite` — deliberately narrower than `SigningKey`'s full API
  (no `to_seed_bytes`, no raw export of any kind).
- `Controller` gains a device-only constructor path that accepts `Box<dyn
  DeviceSigner>` (current) plus a *second* `DeviceSigner` provider closure
  for generating the next key, instead of `Vec<SigningKey>` — scoped to the
  delegated-device path only; the human-root path is unchanged and
  continues to use real in-memory `SigningKey`s (recovery/pairwise-pseudonym
  remain root-only operations).
- `mini_crypto::SigningKey` implements `DeviceSigner` trivially (today's
  behavior, zero change for every existing non-Android caller).
- The actual Android Keystore implementation of `DeviceSigner` lives in
  Kotlin, called through a narrow UniFFI callback interface (`mini-ffi`
  already uses UniFFI 0.32.0, which supports foreign callbacks) — Rust
  never holds Android Keystore key bytes because there are none to hold;
  every sign operation crosses the FFI boundary as (message in, signature
  out).
- The delegation ceremony (create root via recovery flow → generate
  non-exportable phone key → delegate only required capabilities → restart
  and recover without exposing key bytes → revoke from a second device) is
  the acceptance test named in `ANDROID_FOUNDATION.md` and hub issue #196's
  slice #197; it becomes an integration test once the trait exists.

## Division of labor (per hub issue #196)

This environment: the `DeviceSigner` trait, `Controller`'s new
device-signer constructor path, and deterministic tests against a
fake/in-memory `DeviceSigner` (no Android dependency needed to test the
Rust-side contract). Codex/founder's local machine: the real
`android.security.keystore` `KeyGenParameterSpec` implementation, the UniFFI
Kotlin callback wiring, Gradle build, and the actual delegation-ceremony
walkthrough on a device/emulator — this environment cannot verify any of
that, having no JDK/Android SDK/NDK/Gradle/emulator.

## Non-goals (this PR and the next)

- Persistence across process death (hub issue #196 slice #198, depends on
  this).
- Multi-device enrollment/revocation UI flow (slice #199).
- A production security/custody claim of any kind — this remains
  pre-external-review (D-0047 gate, hub issue #196 item 10).
- Any change to `mini-crypto::SignatureSuite`'s default or to any existing
  non-device `Controller` call site.
