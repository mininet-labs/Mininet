//! Wire-size limits for did:mini decoders.
//!
//! These constants are centralized so every parser enforces the same allocation
//! caps before trusting peer-supplied bytes.
//!
//! # Why some of them are public
//!
//! An identity's key set and signature list are not private to this crate:
//! every crate that carries a `did-mini`-signed object has to decode a
//! signature list too, and each one that invents its own cap invents a new way
//! to be wrong. A cap *below* [`MAX_SIGNATURES`] is the dangerous direction — it
//! makes a legitimate threshold identity able to sign an object, verify it in
//! memory, and then fail to decode its own encoding, which looks like corruption
//! and is really a limit mismatch. Downstream decoders should reference these
//! rather than restate them.

/// Largest scid string a decoder will allocate for.
pub(crate) const MAX_SCID_BYTES: usize = 128;

/// Largest `did:mini:<scid>` string a decoder will allocate for.
pub const MAX_DID_BYTES: usize = 256;

pub(crate) const MAX_PRIOR_BYTES: usize = 128;
pub(crate) const MAX_MULTIHASH_BYTES: usize = 128;
pub(crate) const MAX_KEY_BYTES: usize = 256;

/// Largest single signature body, covering every suite this protocol admits
/// (ML-DSA-65 is the largest at roughly 3.3 KiB).
pub const MAX_SIGNATURE_BYTES: usize = 4096;

/// Most keys one establishment event may carry, and therefore the most keys a
/// threshold identity can hold at once.
pub const MAX_KEYS: usize = 32;

pub(crate) const MAX_NEXT: usize = 32;
pub(crate) const MAX_WITNESSES: usize = 64;

/// Most detached signatures a decoder will accept over one message.
///
/// A `MAX_KEYS`-key identity produces one signature per current key, and a
/// message co-signed by several devices carries more, so this is deliberately
/// above [`MAX_KEYS`]. **Downstream crates must not set a lower cap**; see the
/// module docs.
pub const MAX_SIGNATURES: usize = 64;

pub(crate) const MAX_ANCHORS: usize = 128;
pub(crate) const MAX_SEALS: usize = 128;
