//! # mini-crypto
//!
//! The cryptographic foundation for Mininet: a small, auditable, **crypto-agile**
//! primitive layer that every higher crate (`did-mini`, personhood, presence, the
//! forge) builds on.
//!
//! Constitution-frozen invariants are enforced here **structurally** — as code
//! that cannot express the forbidden state, not as a convention a reviewer must
//! remember:
//!
//! 1. **Crypto-agility** (SPEC-01 §13 \[FREEZE\]): signature, key-agreement,
//!    AEAD, and KDF primitives are tagged with versioned suite ids, so the system
//!    can migrate without changing every caller. The *current defaults* are tunable.
//!
//! 2. **Strong-hash content addressing** (SPEC-11 \[FREEZE\]): [`HashAlgorithm`]
//!    offers only BLAKE3 and SHA-256. There is no SHA-1 variant, and the multihash
//!    decoder rejects the SHA-1 multicodec, so a collision-broken content address
//!    cannot be constructed or accepted through this API.
//!
//! 3. **Bluetooth/local encrypted channels** (SPEC-03 keystone + D-0012): X25519,
//!    HKDF-SHA256, and ChaCha20-Poly1305 are available without pulling protocol
//!    code into identity crates. `mini-bearer` composes these into the CH1
//!    anonymous channel; a full Noise/SIGMA variant can layer on later.
//!
//! See `docs/INVARIANTS.md` for the full frozen/tunable register and the mapping
//! from each constitutional invariant to the code that enforces it.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod aead;
pub mod agreement;
pub mod encoding;
pub mod error;
pub mod hash;
pub mod kdf;
pub mod keys;
pub mod multihash;
pub mod suite;

pub use aead::{AeadKey, AeadNonce, AeadSuite};
pub use agreement::{AgreementPublicKey, AgreementSecretKey, KeyAgreementSuite, SharedSecret};
pub use error::{CryptoError, Result};
pub use hash::{HashAlgorithm, DEFAULT_HASH};
pub use kdf::KdfSuite;
pub use keys::{Signature, SigningKey, VerifyingKey};
pub use multihash::Multihash;
pub use suite::SignatureSuite;
