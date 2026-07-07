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
            | Self::MANAGE_DEVICES.bits(),
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
