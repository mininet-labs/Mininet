//! # did-mini
//!
//! Self-sovereign identity for Mininet (SPEC-01): a stable identifier you own,
//! with keys you can rotate, verifiable peer-to-peer with **no central registry
//! and no required blockchain** (SPEC-01 G8).
//!
//! Built on the KERI model of autonomic identifiers (Founder Decision A2):
//!
//!   - a **self-certifying identifier** (`<scid>`) derived from the inception
//!     event, so anyone can verify a `did:mini` is authentic by recomputing it
//!     (SPEC-01 §3);
//!   - a hash-chained, append-only **Key Event Log** (KEL) of signed events
//!     (SPEC-01 §4);
//!   - **pre-rotation** — each event commits to the *hash* of the next keys, so a
//!     leaked current key cannot seize control (SPEC-01 §5).
//!
//! ## Scope (and the boundary that must not be blurred)
//!
//! This crate makes **no claim about humanness**. A `did:mini` could be a bot, and
//! one person can make many. did-mini solves *undercounting* — proving several
//! devices are one human (the delegation batch) — while *overcounting* (the Sybil
//! problem) is personhood's job (SPEC-02). See SPEC-01 §0.
//!
//! ## This batch (SPEC-01 M1 + M2)
//!
//! M1: inception, the KEL, pre-rotation, SCID derivation, offline verification,
//! and a peer-to-peer wire format. M2: device delegation — each device is its own
//! delegated identifier (own KEL + pre-rotation) committing to its human-root,
//! authorized with a capability set and revocable, so several devices are
//! provably one human (SPEC-01 §6). Witnesses (M3), revocation hardening (M4),
//! social recovery (M5), and ZK linkage (M6) build on this in later batches.
//!
//! ```
//! use did_mini::{Controller, Kel};
//!
//! // One device creates an identity entirely offline...
//! let alice = Controller::incept_single().unwrap();
//! let blob = alice.kel().to_bytes();
//!
//! // ...another device, with only the bytes, verifies it with no shared state.
//! let kel = Kel::from_bytes(&blob).unwrap();
//! let state = kel.verify().unwrap();
//! assert_eq!(kel.scid(), alice.scid());
//! assert_eq!(state.sn, 0);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod base_device;
mod codec;
mod controller;
mod delegation;
mod error;
mod event;
mod freshness;
mod identity_mode;
mod kel;
mod limits;
mod witness;

use mini_crypto::{encoding, Multihash};

pub use base_device::{AvailabilityWindow, BaseDeviceRole, BatteryPolicy, PrivacyMode};
pub use controller::Controller;
pub use delegation::{Capabilities, Seal};
pub use error::{IdentityError, Result};
pub use event::{Establishment, Event, EventKind, IndexedSig};
pub use freshness::FreshnessPins;
pub use identity_mode::IdentityMode;
pub use kel::{verify_delegation, Kel, KeyState};
pub use witness::{
    sign_witness_receipt, KeyEventKind, WitnessCertificateVersion, WitnessId, WitnessPolicy,
    WitnessReceipt, WitnessReceiptStatement, WitnessReceiptVersion, WitnessedEventCertificate,
};

/// The `did:mini` method prefix.
pub const METHOD: &str = "did:mini:";

/// A `did:mini` identifier: `did:mini:<scid>`.
///
/// The `<scid>` is self-certifying — it is derived from the inception event, so a
/// `Did` can be checked against a [`Kel`] with no registry lookup.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Did(String);

impl Did {
    /// Build a `Did` from a bare `<scid>`, validating that it is a canonical
    /// strong multihash SCID.
    pub fn from_scid(scid: &str) -> Result<Self> {
        validate_scid(scid)?;
        Ok(Self::from_scid_unchecked(scid))
    }

    pub(crate) fn from_scid_unchecked(scid: &str) -> Self {
        Did(format!("{METHOD}{scid}"))
    }

    /// Parse a `did:mini:<scid>` string, rejecting anything else.
    pub fn parse(s: &str) -> Result<Self> {
        match s.strip_prefix(METHOD) {
            Some(scid) if !scid.is_empty() => {
                validate_scid(scid)?;
                Ok(Did(s.to_string()))
            }
            _ => Err(IdentityError::DidFormat),
        }
    }

    /// The bare `<scid>` (without the `did:mini:` prefix).
    pub fn scid(&self) -> &str {
        &self.0[METHOD.len()..]
    }

    /// The full `did:mini:<scid>` string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for Did {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

fn validate_scid(scid: &str) -> Result<()> {
    // The SCID is a multibase string wrapping a canonical strong multihash. This
    // rejects empty strings, unsupported bases, unknown/forbidden hash codes
    // (notably SHA-1), and non-canonical digest lengths before a verifier ever
    // trusts the identifier as a `did:mini`.
    let bytes = encoding::decode(scid).map_err(IdentityError::Crypto)?;
    Multihash::from_bytes(&bytes).map_err(IdentityError::Crypto)?;
    Ok(())
}
