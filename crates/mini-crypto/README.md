# mini-crypto

The cryptographic foundation for Mininet. Small, `#![forbid(unsafe_code)]`,
crypto-agile, and built to be audited.

Everything else in the on-device core — `did-mini` identity, the Bluetooth bearer,
presence attestations, personhood, the forge, and the update path — signs, hashes,
derives session keys, encrypts frames, and addresses content through this crate.

## What it provides

- **Suite-tagged keys & signatures** (`SignatureSuite`, `SigningKey`,
  `VerifyingKey`, `Signature`). Ed25519 today; the tag travels with every key and
  signature so a post-quantum suite (ML-DSA-65) can be added with no wire-format
  or call-site changes.
- **Suite-tagged key agreement** (`KeyAgreementSuite`, `AgreementSecretKey`,
  `AgreementPublicKey`, `SharedSecret`). X25519 today, for anonymous encrypted
  sessions over BLE/local Wi-Fi.
- **Suite-tagged authenticated encryption** (`AeadSuite`, `AeadKey`,
  `AeadNonce`). ChaCha20-Poly1305 today, with associated-data authentication for
  framed bearer traffic.
- **Suite-tagged key derivation** (`KdfSuite`). HKDF-SHA256 today, including a
  helper that derives AEAD keys directly from X25519 shared secrets and rejects
  oversized HKDF output before allocation.
- **Strong-hash content addressing** (`HashAlgorithm`, `Multihash`). BLAKE3 by
  default, SHA-256 for Git interop. **No SHA-1** — there is no variant for it and
  the decoder rejects its multicodec, non-canonical digest lengths, and
  non-canonical varint encodings.
- **Multibase** identifier encoding (`encoding`) — base58btc and hex today.

The DH/AEAD/HKDF primitives deliberately adapt vetted RustCrypto/dalek crates;
Mininet adds only the small suite-tagged API, length checks, all-zero X25519
shared-secret rejection, and secret-redacting wrappers.

## Frozen and security-critical invariants enforced as code

| Constitutional source | Invariant | How it is enforced here |
|---|---|---|
| SPEC-01 §13 \[FREEZE\] | The crypto layer stays agile; no algorithm hard-wired for life | Signature, DH, AEAD, and KDF suites are versioned and tagged; defaults are tunable |
| SPEC-11 \[FREEZE\] | Canonical addressing uses a strong hash, never SHA-1 | `HashAlgorithm` has no SHA-1 variant; `Multihash::from_bytes` rejects code `0x11`, non-canonical digest lengths, and non-canonical varints |
| SPEC-01 G1 | Secret keys never leave the device | secret export only via explicit methods; `Debug` redacts signing, agreement, shared-secret, and AEAD key material |
| SPEC-03 / D-0012 | Bluetooth/local channels must be encryptable without internet | X25519 + HKDF-SHA256 + ChaCha20-Poly1305 provide the primitives for the `mini-bearer` CH1 anonymous channel |

## Build & test

```sh
cargo test -p mini-crypto
```

Tests are deterministic where possible (fixed seeds), so they reproduce
identically anywhere. Random generation paths are covered by type/API tests in
later bearer crates.
