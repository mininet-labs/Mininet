//! Device delegation primitives (SPEC-01 §6): capability scoping and the seals a
//! human-root uses to authorize or revoke a device. The counting layer today is
//! *identity-root* based (personhood is SPEC-02, pending — D-0030); this file's
//! guarantee is narrower and already enforced: capabilities can only *narrow* a
//! device, never inflate a root's standing.
//!
//! ## Capabilities scope *authority*, never *vote count*
//!
//! A capability decides what a given device is allowed to do on the root's
//! behalf. It never multiplies the root's standing. Every device chains to ONE
//! identity root, and the personhood/governance layer counts that root exactly
//! once (constitution **P2** *target*: one verified human, one equal vote — read
//! as one verified identity root until SPEC-02 lands). So `VOTE` means
//! "this device may cast the root's single vote," not "this device adds a vote."
//! There is deliberately no capability that could create extra votes, extra
//! presence weight, or extra anything — capability scoping can only *narrow* a
//! device, never inflate the root.

use crate::codec::{Reader, Writer};
use crate::error::{IdentityError, Result};
use crate::limits::MAX_DID_BYTES;
use crate::Did;

/// A bitset of device capabilities (SPEC-01 §6 capability scoping).
///
/// Secure defaults: [`Capabilities::primary`] and [`Capabilities::secondary`].
/// Sensitive operations (managing devices, rotating the root) are not granted by
/// either default and require root participation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities(u32);

impl Capabilities {
    /// Day-to-day signing.
    pub const SIGN: Capabilities = Capabilities(1);
    /// Initiate payments.
    pub const PAY: Capabilities = Capabilities(1 << 1);
    /// Publish posts/content.
    pub const POST: Capabilities = Capabilities(1 << 2);
    /// Co-sign presence attestations.
    pub const ATTEST: Capabilities = Capabilities(1 << 3);
    /// Cast the human's (single, equal) governance vote — see the module note:
    /// this never adds a vote, it only designates which device may cast the one
    /// the human already has.
    pub const VOTE: Capabilities = Capabilities(1 << 4);
    /// Add or revoke *other* devices. A root-level power; off in both secure
    /// defaults, so a delegated device cannot expand the device set on its own.
    pub const MANAGE_DEVICES: Capabilities = Capabilities(1 << 5);
    /// Publish storage commitments and answer possession/replication proofs on
    /// the root's behalf.
    ///
    /// Off in **both** secure defaults, deliberately. Unlike signing or posting,
    /// a storage commitment exposes the root to durable, publishable conflict
    /// evidence about its own storage conduct (see `mini-storage-fraud`): a
    /// device with this capability can bind the root to a replica claim that
    /// outlives the device. That liability has to be granted on purpose, per
    /// storage device, not inherited from a "primary device" default.
    pub const STORE: Capabilities = Capabilities(1 << 6);

    /// No capabilities.
    pub const fn empty() -> Self {
        Capabilities(0)
    }

    /// Build from a raw bit pattern, rejecting unknown future bits. Wire
    /// decoders must be conservative: a capability a verifier does not
    /// understand must not be silently granted.
    pub fn from_bits(bits: u32) -> Result<Self> {
        if bits & !Self::ALL.bits() != 0 {
            return Err(IdentityError::BadEvent);
        }
        Ok(Capabilities(bits))
    }

    /// All capability bits understood by this version.
    pub const ALL: Capabilities = Capabilities(
        Self::SIGN.bits()
            | Self::PAY.bits()
            | Self::POST.bits()
            | Self::ATTEST.bits()
            | Self::VOTE.bits()
            | Self::MANAGE_DEVICES.bits()
            | Self::STORE.bits(),
    );

    /// The raw bit pattern.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// The union of two capability sets.
    pub const fn with(self, other: Capabilities) -> Self {
        Capabilities(self.0 | other.0)
    }

    /// Whether `self` contains every capability in `other`.
    pub const fn contains(self, other: Capabilities) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Secure default for a primary device: broad day-to-day authority, but NOT
    /// device management or root rotation (SPEC-01 §6).
    pub fn primary() -> Self {
        Self::SIGN
            .with(Self::PAY)
            .with(Self::POST)
            .with(Self::ATTEST)
            .with(Self::VOTE)
    }

    /// Secure default for a secondary device: sign / pay / post only — no vote,
    /// no device management (SPEC-01 §6).
    pub fn secondary() -> Self {
        Self::SIGN.with(Self::PAY).with(Self::POST)
    }

    /// The fixed, risk-bounded capability set for a [`DeviceTier`] (D-0530,
    /// Founder Directive 13, issue #14). This is the *only* sanctioned path
    /// from "what kind of device is this" to "what may it do" — see
    /// [`DeviceTier`] for why each tier gets the bound it does.
    pub fn for_tier(tier: DeviceTier) -> Self {
        match tier {
            DeviceTier::ColdRoot => Self::ALL,
            DeviceTier::HardwareToken => Self::SIGN,
            DeviceTier::DailyDevice => Self::primary(),
            DeviceTier::Emerging => Self::secondary(),
        }
    }
}

/// A named tier in the device hierarchy an identity root delegates to
/// (issue #14, D-0530). Each tier is a fixed risk profile, not a free-form
/// label: [`Capabilities::for_tier`] is the only constructor that turns a
/// tier into a capability set, so the tier bounds what a device may do at
/// compile time rather than at whatever bits a caller happens to assemble
/// (the same typed-domain discipline this crate applies everywhere else —
/// see the crate's `sign(bytes)`/`finalize(state)` prohibition).
///
/// This is a **policy layer over an existing primitive**: every tier still
/// delegates through the ordinary [`Seal::Delegate`]/[`Seal::Revoke`]
/// mechanism and the root's own KEL. No new cryptography, no new wire
/// format, no new capability bits — just a named, bounded mapping from
/// device *kind* to device *capability set*.
///
/// `#[non_exhaustive]` per Founder Directive 13 ("think in centuries, not
/// releases"): today's device shapes (phone, hardware dongle) are not
/// assumed permanent. A genuinely new device shape (e.g. an implant or
/// wearable) is *not* silently matched into an existing tier or given a
/// bespoke capability set on the spot; see [`DeviceTier::Emerging`] and the
/// design note at `docs/design/device-hierarchy.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeviceTier {
    /// The rarely-used, highest-authority key: full authority including
    /// [`Capabilities::MANAGE_DEVICES`] (add/revoke other devices) and
    /// [`Capabilities::VOTE`]/[`Capabilities::STORE`]. Meant to sit offline
    /// or air-gapped, invoked only to re-key the device set after a lower
    /// tier is lost or compromised — never for everyday signing. Losing
    /// this tier's key is the worst case a human root can face short of
    /// full root compromise, so it is granted the full bound, never more
    /// (it still cannot exceed [`Capabilities::ALL`] — no capability this
    /// crate does not already define).
    ColdRoot,
    /// Dedicated signing hardware (e.g. a FIDO2-class security key):
    /// [`Capabilities::SIGN`] only. Deliberately excludes
    /// [`Capabilities::MANAGE_DEVICES`] — a hardware token authenticates
    /// day-to-day operations, it does not get to reshape the device set —
    /// and excludes [`Capabilities::VOTE`]/[`Capabilities::PAY`]/
    /// [`Capabilities::POST`]/[`Capabilities::ATTEST`]/[`Capabilities::STORE`]
    /// so a stolen token is a signing nuisance, not a governance or funds
    /// incident.
    HardwareToken,
    /// A phone-class device in constant use: [`Capabilities::primary`]'s
    /// bound (sign/pay/post/attest/vote) but never
    /// [`Capabilities::MANAGE_DEVICES`] or [`Capabilities::STORE`] — the
    /// device most likely to be lost, left unlocked, or malware-infected
    /// gets everyday authority but no key-management or durable-storage-
    /// liability authority.
    DailyDevice,
    /// A device shape this crate does not yet have a named tier for
    /// (implant, wearable, or anything not yet invented) — Founder
    /// Directive 13's "think in centuries" clause in code form. Bound to
    /// [`Capabilities::secondary`] (sign/pay/post, no vote, no device
    /// management) until the network has enough experience with the shape
    /// to warrant its own named tier and bound, decided the same way any
    /// other capability policy is: a new `docs/DECISION_LOG.md` entry, not
    /// a silent code change.
    Emerging,
}

/// A seal carried by a human-root's `Seal` event to authorize or revoke a
/// delegated device (SPEC-01 §6). Seals ride in the root's own KEL, so the root's
/// history is a tamper-evident record of which devices it authorized and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seal {
    /// Authorize a delegated device identifier with a capability set.
    Delegate {
        /// The device's `did:mini:<scid>` string.
        device: String,
        /// The capabilities granted to that device.
        capabilities: Capabilities,
    },
    /// Revoke a previously delegated device.
    Revoke {
        /// The device's `did:mini:<scid>` string.
        device: String,
    },
}

const SEAL_DELEGATE: u8 = 0x01;
const SEAL_REVOKE: u8 = 0x02;

pub(crate) fn encode_seal(w: &mut Writer, seal: &Seal) {
    match seal {
        Seal::Delegate {
            device,
            capabilities,
        } => {
            w.u8(SEAL_DELEGATE);
            w.bytes(device.as_bytes());
            w.u32(capabilities.bits());
        }
        Seal::Revoke { device } => {
            w.u8(SEAL_REVOKE);
            w.bytes(device.as_bytes());
        }
    }
}

pub(crate) fn decode_seal(r: &mut Reader) -> Result<Seal> {
    let tag = r.u8()?;
    match tag {
        SEAL_DELEGATE => {
            let device = String::from_utf8(r.bytes_limited("seal.device", MAX_DID_BYTES)?)
                .map_err(|_| IdentityError::BadEvent)?;
            Did::parse(&device)?;
            let capabilities = Capabilities::from_bits(r.u32()?)?;
            Ok(Seal::Delegate {
                device,
                capabilities,
            })
        }
        SEAL_REVOKE => {
            let device = String::from_utf8(r.bytes_limited("seal.device", MAX_DID_BYTES)?)
                .map_err(|_| IdentityError::BadEvent)?;
            Did::parse(&device)?;
            Ok(Seal::Revoke { device })
        }
        _ => Err(IdentityError::BadEvent),
    }
}
