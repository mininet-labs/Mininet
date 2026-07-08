//! Vouch attestation types and the deterministic transcript both devices sign.
//!
//! A vouch is the signal-(a) primitive: two delegated devices, each bound to
//! a `did:mini` identity root, mutually assert "we know each other as
//! distinct, genuine humans." Unlike [`mini_presence::PresenceAttestation`],
//! a vouch is **not proximity-bound** — the whitepaper's social-vouching
//! graph (SS5) is a general web of acquaintance, not an in-person-only signal
//! (that's signal (c), physical presence). Vouching may ride any
//! [`mini_presence::TransportKind`], including a relay.

use did_mini::{Controller, Did, IndexedSig};
use mini_presence::TransportKind;

/// Wire version of a vouch attestation.
pub const VOUCH_VERSION: u8 = 1;

/// One side of a vouch: which device, pinned to its KEL state, with a fresh
/// nonce. Same shape and same nonce-generation rule as
/// [`mini_presence::Party`]: real use must call `mini_crypto::random_32`,
/// never a fixed value; fixed values are a test-only convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoucherParty {
    /// The device's `did:mini`.
    pub device: Did,
    /// BLAKE3 digest of the device KEL at vouch time.
    pub kel_digest: [u8; 32],
    /// A fresh 32-byte nonce for replay resistance.
    pub nonce: [u8; 32],
}

/// The signed content of a vouch attestation — everything except the two
/// signatures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VouchFields {
    /// Vouch protocol version.
    pub version: u8,
    /// The 32-byte binding of the channel the parties used, if any
    /// established one — vouching does not require a proximity bearer, so
    /// unlike presence this may be an internet relay session.
    pub channel_binding: [u8; 32],
    /// The channel's transport kind, carried for context only — vouch
    /// verification does not require proximity (contrast
    /// `mini_presence::verify_presence`'s [FREEZE]).
    pub transport: TransportKind,
    /// The first party.
    pub a: VoucherParty,
    /// The second party.
    pub b: VoucherParty,
    /// When the vouch was asserted (device clock, ms).
    pub asserted_at_ms: u64,
}

impl VouchFields {
    /// The exact bytes both devices sign. Deterministic and length-prefixed
    /// so it re-serializes identically on every device.
    pub fn transcript(&self) -> Vec<u8> {
        let mut w = Vec::new();
        w.push(self.version);
        w.extend_from_slice(&self.channel_binding);
        w.push(self.transport.tag());
        put_party(&mut w, &self.a);
        put_party(&mut w, &self.b);
        w.extend_from_slice(&self.asserted_at_ms.to_be_bytes());
        w
    }

    /// Sign the transcript with a device controller. Called independently on
    /// each device; secrets never leave the device, only the resulting
    /// signatures.
    pub fn sign(&self, device: &Controller) -> Vec<IndexedSig> {
        device.sign_message(&self.transcript())
    }
}

fn put_party(w: &mut Vec<u8>, p: &VoucherParty) {
    let did = p.device.as_str().as_bytes();
    w.extend_from_slice(&(did.len() as u32).to_be_bytes());
    w.extend_from_slice(did);
    w.extend_from_slice(&p.kel_digest);
    w.extend_from_slice(&p.nonce);
}

/// A complete, mutually-signed vouch attestation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VouchAttestation {
    /// The signed content.
    pub fields: VouchFields,
    /// Party `a`'s device signature over the transcript.
    pub a_sig: Vec<IndexedSig>,
    /// Party `b`'s device signature over the transcript.
    pub b_sig: Vec<IndexedSig>,
}

impl VouchAttestation {
    /// Assemble a vouch from its fields and the two devices' signatures.
    pub fn new(fields: VouchFields, a_sig: Vec<IndexedSig>, b_sig: Vec<IndexedSig>) -> Self {
        VouchAttestation {
            fields,
            a_sig,
            b_sig,
        }
    }
}
