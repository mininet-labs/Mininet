//! Wire-size limits for did:mini decoders.
//!
//! These constants are centralized so every parser enforces the same allocation
//! caps before trusting peer-supplied bytes.

pub(crate) const MAX_SCID_BYTES: usize = 128;
pub(crate) const MAX_DID_BYTES: usize = 256;
pub(crate) const MAX_PRIOR_BYTES: usize = 128;
pub(crate) const MAX_MULTIHASH_BYTES: usize = 128;
pub(crate) const MAX_KEY_BYTES: usize = 256;
pub(crate) const MAX_SIGNATURE_BYTES: usize = 4096;
pub(crate) const MAX_KEYS: usize = 32;
pub(crate) const MAX_NEXT: usize = 32;
pub(crate) const MAX_WITNESSES: usize = 64;
pub(crate) const MAX_SIGNATURES: usize = 64;
pub(crate) const MAX_ANCHORS: usize = 128;
pub(crate) const MAX_SEALS: usize = 128;
