# Cold and owner-only storage tiers: giving `Payload::Encrypted` a real construction

**Decisions:** D-0434 (see `docs/DECISION_LOG.md`)
**Status:** Implemented for local, single-process sealing/opening. No network transport of sealed objects is designed or wired here.
**Refs:** roadmap [#34](../../issues/34) (Phase 4.6, "Storage privacy: cold storage, owner-only storage, encryption"); `docs/STATUS.md`'s "not started — cold/owner-only storage tiers (roadmap Phase 4)" line; `crates/mini-store/src/cache.rs`'s existing `CacheTier::PrivateOnly`/`PinnedByOwner` (founder decision, 2026-07-07); `crates/mini-objects/src/object.rs`'s `Payload::Encrypted` variant (present in the wire format since the object model's inception, never constructed by any crate until this decision).

## The gap this closes

`mini_objects::Payload` has carried an `Encrypted(Vec<u8>)` variant since the object model was first written. Every reader in the workspace (mini-social, mini-forge, mini-media, mini-search-federation, mini-sync's ingest, mini-store's own `apply_head`) already has a match arm for it — uniformly to *reject* it ("not public," "bad head," "bad manifest"). No crate anywhere constructs one with real ciphertext. `docs/STATUS.md` has carried "not started — cold/owner-only storage tiers" opposite every other Phase 4 item, which are all shipped, since the tracker was created.

Meanwhile `mini-store::cache`'s `CacheTier` already solves half of "owner-only": `PrivateOnly` ("never advertised, regardless of any policy") and `PinnedByOwner` ("never auto-downgraded") already exist, and the module's own doc comment already states the frozen rule that "encrypted content can never be promoted past `PrivateOnly`." What was missing was the other half: an actual way to *produce* encrypted content in the first place, and a tier for content the owner wants held indefinitely without ever being read back in the ordinary course of operation (a "cold" archive, as opposed to `PinnedByOwner`'s implicit "the owner is actively using this").

## What this adds

1. **`mini_crypto`-composed sealed-box encryption** (`mini-store::owner_seal`, new module): an `OwnerSealingKey`/`OwnerSealingPublicKey` pair (thin, purpose-named wrappers around `mini_crypto::agreement::AgreementSecretKey`/`AgreementPublicKey`), and `seal_for_owner`/`open_as_owner`, which produce and consume the exact bytes that go inside `Payload::Encrypted`.
2. **`CacheTier::ColdArchive`**: a new tier, alongside the existing five, for sealed content the owner wants retained indefinitely without treating it as "currently in active use" the way `PinnedByOwner` implies. Like `PrivateOnly`, it never advertises. Unlike `PrivateOnly`, `Store::note_view` never demotes *or* promotes it — it is set once, explicitly, and stays until the owner explicitly changes it.

## The sealed-box construction (composition of prior art, not new cryptography)

This is the NaCl/libsodium "sealed box" construction (`crypto_box_seal`), a published, peer-reviewed, real-world-deployed design, composed entirely from primitives this workspace's `mini-crypto` already exposes and has already reviewed for other purposes (`mini-bearer::Channel`'s handshake, `mini-relay`'s per-hop encryption):

```text
seal_for_owner(recipient_public, plaintext, aad):
  1. ephemeral = AgreementSecretKey::generate()          # fresh per call, never reused
  2. shared    = ephemeral.agree(recipient_public)        # X25519 ECDH
  3. key       = KdfSuite::HkdfSha256.derive_aead_key_from_shared(
                   salt = None,
                   shared,
                   info = b"mini-store/owner-seal/v1",    # domain separation from
                                                            # mini-bearer's channel-key
                                                            # derivation and any other
                                                            # HKDF use of a shared secret
                   suite = AeadSuite::ChaCha20Poly1305,
                 )
  5. nonce     = AeadNonce::generate()                    # fresh per call
  6. ciphertext = key.encrypt(nonce, plaintext, aad)
  7. return ephemeral.public_key() || nonce || ciphertext  # canonical sealed bytes

open_as_owner(owner_secret, sealed_bytes, aad):
  1. parse ephemeral_public || nonce || ciphertext from sealed_bytes (bounds-checked)
  2. shared = owner_secret.agree(ephemeral_public)
  3. key    = KdfSuite::HkdfSha256.derive_aead_key_from_shared(None, shared, info, suite)
  4. return key.decrypt(nonce, ciphertext, aad)
```

No new curve arithmetic, no new AEAD, no new KDF — every cryptographic operation is an existing `mini-crypto` call. The only workspace-original decision is the wire layout (ephemeral public key, then nonce, then ciphertext, length-prefixed and bounded) and the domain-separation `info` string, neither of which is a cryptographic design choice.

### Why a separate sealing keypair, not a device's existing Ed25519 KEL key

`did-mini`'s `Controller`/KEL model is Ed25519 signing only — there is no X25519 agreement key anywhere in the identity layer, and no delegation/capability bit for "may decrypt." Converting an existing Ed25519 secret into an X25519 secret (the libsodium `crypto_sign_ed25519_sk_to_curve25519` transform: SHA-512 the seed, clamp, use as the X25519 scalar) is itself a distinct published construction, but pulling it in here would mean this decision is simultaneously introducing a second cryptographic transform *and* reaching into `did-mini`'s core KEL model — a bigger, more invasive change than the problem requires, and exactly the kind of scope creep Directive 14 ("simplicity is security... prefer the smaller, well-trodden construction") warns against.

Instead, `OwnerSealingKey` is generated independently (`AgreementSecretKey::generate()`) and is the caller's own responsibility to persist (in whatever secure keystore already holds their `SigningKey` seeds — `Controller::export_current_and_next_keys_for_storage` already establishes that a caller legitimately handles raw key material for its own storage). This is a real, honestly-stated limitation, not an oversight: a sealing key has **no relationship to KEL rotation, delegation, or recovery** in this first slice. Losing the sealing key means the sealed content is unrecoverable, exactly like losing any other local-only secret; there is no not-yet-built path to recover it via KEL social recovery the way a signing key theoretically could gain one.

## What `CacheTier::ColdArchive` does and does not do

- Extends `mini_store::cache::CacheTier` with a sixth variant. Wire tag `5` (the five existing variants use `0..=4`).
- `advertises()` returns `false` for it, exactly like `PrivateOnly` — the frozen invariant in `cache.rs`'s own module doc ("encrypted content can never be promoted past `PrivateOnly`... regardless of policy") is preserved verbatim; `ColdArchive` is *at or below* that bound, never above it.
- `Store::note_view` (seed-on-view policy) does not touch `ColdArchive` at all — a caller sets it explicitly via `Store::set_cache_tier`, the same entry point `PinnedByOwner` already uses, and it stays until the caller explicitly changes it again. This mirrors `PinnedByOwner`'s "never auto-downgraded" contract but drops the "owner is actively pinning this because they use it" connotation `PinnedByOwner`'s name carries — `ColdArchive` is for content the owner wants to *keep* without *using*.
- Does **not** imply the payload is actually sealed. `ColdArchive` is an availability/retention tier, exactly like the other five; nothing in `mini-store` enforces that a `ColdArchive`-tiered object's payload is `Payload::Encrypted` (a caller could tier a `Payload::Public` object as `ColdArchive` too, e.g. a public archival document they simply don't want auto-evicted). The two features compose but are independent — this keeps the tier enum's job ("what does this local cache do with this object") separate from the payload's own job ("what does this object's content mean").
- Does **not** add any actual eviction/pruning logic. No cache tier in this crate today ever auto-deletes an object; `ColdArchive` inherits that — it is a *retention signal* for a future pruning policy to honor, not a pruning mechanism itself. (This matches the existing tiers' scope precisely; none of the other five evict anything either.)

## Constitutional and authority impact

No frozen invariant is touched. The voice/value wall (P1, Directive 16) is untouched — `mini-store` gains a dependency only on `mini-crypto` (already a dependency) and adds no new crate dependency at all. No generic `encrypt(bytes)`/`decrypt(bytes)` surface: `seal_for_owner`/`open_as_owner` take a concrete `OwnerSealingPublicKey`/`OwnerSealingKey`, not an arbitrary key blob, satisfying the typed-domain rule the same way every other signing/sealing entry point in this workspace does. This does not touch identity, governance, consensus, or value crates in any direction. `CacheTier::ColdArchive` cannot be promoted to an advertising tier by any policy, preserving the existing frozen behavior.

## Honest limits (what this is not)

- **Not integrated with KEL identity.** The sealing keypair is independent of a device's signing key and its own rotation/recovery story; losing it loses access to everything sealed to it. A future decision may bind a sealing key to KEL delegation the way signing authority already is — not attempted here.
- **Not a network protocol.** No wire message, peer exchange, or transport carries a sealed object anywhere in this change; `mini-sync`'s ingest pipeline already rejects `Payload::Encrypted` objects outright (unrelated existing behavior, unchanged here) — a sealed object stored locally does not yet propagate anywhere, by design, since who is even allowed to *hold* someone else's sealed bytes (without being able to read them) is a distinct question this decision does not answer.
- **Not forward-secret against a compromised owner sealing key.** Every sealed object under one `OwnerSealingKey` becomes readable if that key is later compromised — there is no per-object key rotation, no key ratchet, no re-sealing mechanism. Ephemeral per-seal keys protect against a compromised *ephemeral* secret revealing anything beyond that one seal; they do not protect against the long-term recipient key itself being compromised, which is inherent to any recipient-decryptable sealing scheme, not specific to this one.
- **Not integrity-bound to any device/identity claim.** `seal_for_owner` does not sign anything — it is confidentiality only. A caller wanting "this content came from a specific signed source" still wraps the *result* in an ordinary signed `mini_objects::Object` (`Payload::Encrypted(sealed_bytes)`, signed by the caller's usual `ObjectBuilder`), exactly the same two-layer pattern (content confidentiality separate from signature authenticity) every other payload type in this workspace already follows.
- **No huge-file story.** Sealing operates on one bounded plaintext
  (`mini_store::MAX_OWNER_SEAL_PLAINTEXT_BYTES`: the 8 MiB object-payload
  ceiling minus the ephemeral-key, nonce, and AEAD-tag overhead); large sealed
  content composes with `mini-media`'s existing superblock/manifest chunking
  (D-0419) exactly the way any other payload type already would, not a new
  mechanism.
- **Not an economic-incentive or cold-storage-provider design.** Roadmap #33 (storage economic incentive review) is untouched; this decision is about the *capability* to hold encrypted-at-rest content locally, not about paying anyone to hold it for you.

## Tests

Adversarial coverage in `crates/mini-store/tests/owner_seal.rs`:

- round trip (seal then open recovers the exact plaintext);
- wrong owner key fails to open (a different `OwnerSealingKey` cannot decrypt another's sealed bytes);
- tampered ciphertext/AAD/ephemeral-public-key bytes each fail closed (AEAD authentication catches all three independently);
- truncated/oversized sealed-byte inputs are rejected before any allocation past the bound;
- two seals of the same plaintext to the same recipient produce different ciphertext bytes (fresh ephemeral key + nonce per call, proving no accidental key/nonce reuse);
- `CacheTier::ColdArchive` round-trips through the tier wire codec and reports `advertises() == false`, matching `PrivateOnly`;
- `Store::note_view` never changes a `ColdArchive`-tiered object's tier, mirroring the existing `PinnedByOwner` non-demotion test.

## Required follow-up

- Bind a sealing keypair to KEL delegation/rotation once a real device-loss/recovery story for owner-only content is designed — not guessed at here.
- Design whether/how a `ColdArchive`-tiered sealed object should ever leave the local machine (e.g. an owner's own second device pulling it back) — today nothing transports `Payload::Encrypted` objects at all.
- Wire an actual pruning/retention policy that reads the `ColdArchive` signal — the tier exists as a signal now; no consumer of that signal exists yet.
- Roadmap #33 (storage economic incentive review) remains separate, untouched work.

## Supersedes / superseded by

New ground — no prior decision addressed `Payload::Encrypted` construction or a sixth `CacheTier` variant. Builds on and does not modify `cache.rs`'s existing five-tier model or its frozen `PrivateOnly`-ceiling rule (2026-07-07 founder decision); builds on and does not modify `mini-crypto`'s existing `agreement`/`kdf`/`aead` modules (all reused unchanged).
