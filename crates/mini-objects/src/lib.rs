//! The unified object model (SPEC-09 §2): one signed, typed, content-addressed
//! envelope that every surface reads and writes.
//!
//! A microblog post, a forum comment, a media manifest, a commit, a release —
//! all are the same [`Object`]:
//!
//! - **type** — an extensible tag ([`ObjectType`]) with a well-known core set;
//! - **author** — a `did:mini` human-root plus the delegated device that signed
//!   (either may be a pairwise pseudonym; ZK linkage lands with personhood);
//! - **signature** — by the device's current keys, verifiable by any peer;
//! - **timestamp / sequence**;
//! - **payload** — plaintext or ciphertext ([`Payload`]); the signature always
//!   covers the object, encryption only hides the content (SPEC-09 §4);
//! - **links** — typed references ([`Link`]) to other objects by content id.
//!
//! **Content-addressed:** an object's name is its [`ObjectId`] — a strong
//! multihash over its canonical bytes — so it is tamper-evident, deduplicated,
//! and servable by any holder. (Our id is multibase/multihash like a `did:mini`
//! SCID; byte-level IPLD-CID interop is a later, additive mapping.)
//!
//! **Composability [FREEZE]:** because everything is one envelope, any object can
//! link any other regardless of surface — a forum post links a commit, a feed
//! post links a thread — with no per-surface format, ever.
//!
//! ## Verification is layered
//!
//! 1. [`Object::verify_integrity`] — the id matches the bytes (any holder).
//! 2. [`Object::verify_signature`] — the named device signed it (needs the
//!    device KEL).
//! 3. [`verify_provenance`] — the device is a delegated, unrevoked device of the
//!    named human-root holding the required capability (needs both KELs). For
//!    content types the required capability is `POST` (SPEC-01 §6 scoping).
//!
//! Objects are **immutable**; mutable state (signed head pointers, CRDT op-logs,
//! locally-computed feeds — SPEC-09 §3) builds on top in the next batches.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod capability;
mod codec;
mod envelope_v2;
mod error;
mod object;
mod private_object;
mod pseudonym;

pub use capability::{
    CapabilityGrant, CapabilityRight, CapabilityScope, CapabilityToken, CapabilityTokenCommitment,
    CAPABILITY_VERSION,
};
pub use envelope_v2::{
    ObjectEnvelopeV2, OpaqueRoute, RetentionClass, StorageDescriptor, ENVELOPE_VERSION,
};
pub use error::{ObjectError, Result};
pub use object::{
    verify_provenance, Link, Object, ObjectBuilder, ObjectId, ObjectType, Payload, MAX_LINKS,
    MAX_PAYLOAD_BYTES,
};
pub use private_object::PrivateObject;
pub use pseudonym::{derive_scoped_pseudonym, PseudonymPurpose};
