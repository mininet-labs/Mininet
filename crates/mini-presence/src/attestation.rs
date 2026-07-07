//! Presence attestation types and the deterministic transcript both devices sign.

use did_mini::{Controller, Did, IndexedSig, Kel};
use mini_crypto::HashAlgorithm;

/// Wire version of a presence attestation.
pub const PRESENCE_VERSION: u8 = 1;

/// How two devices were in contact.
///
/// Only proximity transports can evidence co-presence. A relay/internet path
/// cannot, so it is rejected by [`crate::verify_presence`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransportKind {
    /// Bluetooth Low Energy.
    Ble,
    /// A local Wi-Fi / hotspot link.
    LocalWifi,
    /// The in-process test bearer (treated as proximity for CI only).
    InProcess,
    /// An internet relay — not proximity, cannot evidence co-presence.
    Relay,
}

impl TransportKind {
    /// Stable wire tag.
    pub fn tag(self) -> u8 {
        match self {
            TransportKind::Ble => 1,
            TransportKind::LocalWifi => 2,
            TransportKind::InProcess => 3,
            TransportKind::Relay => 4,
        }
    }

    /// Whether this transport can evidence physical co-presence.
    pub fn is_proximity(self) -> bool {
        matches!(
            self,
            TransportKind::Ble | TransportKind::LocalWifi | TransportKind::InProcess
        )
    }
}

/// One side of an attestation: which device, pinned to its KEL state, with a
/// fresh nonce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Party {
    /// The device's `did:mini`.
    pub device: Did,
    /// BLAKE3 digest of the device KEL at attestation time (see [`kel_digest`]).
    pub kel_digest: [u8; 32],
    /// A fresh 32-byte nonce for replay resistance.
    pub nonce: [u8; 32],
}

/// The signed content of a presence attestation — everything except the two
/// signatures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttestationFields {
    /// Presence protocol version.
    pub version: u8,
    /// The 32-byte binding of the encrypted channel the parties used.
    pub channel_binding: [u8; 32],
    /// The initiating device.
    pub initiator: Party,
    /// The responding device.
    pub responder: Party,
    /// Session start (device clock, ms).
    pub started_at_ms: u64,
    /// Session finish (device clock, ms).
    pub finished_at_ms: u64,
    /// Round-trip range samples in milliseconds (the proximity evidence).
    pub rtt_samples_ms: Vec<u32>,
    /// How the two devices were in contact.
    pub transport: TransportKind,
    /// Optional fuzzed location commitment (a hash; no raw coordinates).
    pub location_commitment: Option<[u8; 32]>,
}

impl AttestationFields {
    /// The exact bytes both devices sign. Deterministic and length-prefixed so it
    /// re-serializes identically on every device.
    pub fn transcript(&self) -> Vec<u8> {
        let mut w = Vec::new();
        w.push(self.version);
        w.extend_from_slice(&self.channel_binding);
        put_party(&mut w, &self.initiator);
        put_party(&mut w, &self.responder);
        w.extend_from_slice(&self.started_at_ms.to_be_bytes());
        w.extend_from_slice(&self.finished_at_ms.to_be_bytes());
        w.extend_from_slice(&(self.rtt_samples_ms.len() as u32).to_be_bytes());
        for s in &self.rtt_samples_ms {
            w.extend_from_slice(&s.to_be_bytes());
        }
        w.push(self.transport.tag());
        match &self.location_commitment {
            Some(commitment) => {
                w.push(1);
                w.extend_from_slice(commitment);
            }
            None => w.push(0),
        }
        w
    }

    /// Sign the transcript with a device controller. Called independently on each
    /// device; secrets never leave the device, only the resulting signatures.
    pub fn sign(&self, device: &Controller) -> Vec<IndexedSig> {
        device.sign_message(&self.transcript())
    }
}

fn put_party(w: &mut Vec<u8>, p: &Party) {
    let did = p.device.as_str().as_bytes();
    w.extend_from_slice(&(did.len() as u32).to_be_bytes());
    w.extend_from_slice(did);
    w.extend_from_slice(&p.kel_digest);
    w.extend_from_slice(&p.nonce);
}

/// A complete, mutually-signed presence attestation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresenceAttestation {
    /// The signed content.
    pub fields: AttestationFields,
    /// The initiating device's signature over the transcript.
    pub initiator_sig: Vec<IndexedSig>,
    /// The responding device's signature over the transcript.
    pub responder_sig: Vec<IndexedSig>,
}

impl PresenceAttestation {
    /// Assemble an attestation from its fields and the two devices' signatures.
    pub fn new(
        fields: AttestationFields,
        initiator_sig: Vec<IndexedSig>,
        responder_sig: Vec<IndexedSig>,
    ) -> Self {
        PresenceAttestation {
            fields,
            initiator_sig,
            responder_sig,
        }
    }
}

/// The BLAKE3 digest of a KEL's canonical bytes — pins an attestation to a
/// specific key state, so a later rotation can't retroactively change what was
/// attested.
pub fn kel_digest(kel: &Kel) -> [u8; 32] {
    HashAlgorithm::Blake3.digest(&kel.to_bytes())
}
