//! Storage-served receipt types and the deterministic transcript both
//! devices sign.

use did_mini::{Controller, Did, IndexedSig};
use mini_objects::ObjectId;

/// Wire version of a storage-served receipt.
pub const RECEIPT_VERSION: u8 = 1;

/// The signed content of a receipt — everything except the two signatures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptFields {
    /// Receipt protocol version.
    pub version: u8,
    /// What was served (a content-addressed manifest or object id).
    pub content_id: ObjectId,
    /// Bytes served.
    pub bytes: u64,
    /// BLAKE3 digest of the bytes the witness actually received. Part of
    /// the signed transcript, so it cannot be altered after signing, but
    /// `verify_serve` has no access to the raw bytes and so does **not**
    /// itself cross-check this digest — that check is the caller's to make
    /// (e.g. against locally re-hashed content) before trusting a verdict.
    pub content_digest: [u8; 32],
    /// The serving device.
    pub host_device: Did,
    /// The receiving/witnessing device.
    pub witness_device: Did,
    /// The host's fresh nonce, for replay resistance.
    pub host_nonce: [u8; 32],
    /// The witness's fresh nonce, for replay resistance.
    pub witness_nonce: [u8; 32],
    /// When the serve completed (device clock, ms).
    pub at_ms: u64,
}

impl ReceiptFields {
    /// The exact bytes both devices sign. Deterministic and length-prefixed
    /// so it re-serializes identically on every device.
    pub fn transcript(&self) -> Vec<u8> {
        let mut w = Vec::new();
        w.push(self.version);
        put_str(&mut w, self.content_id.as_str());
        w.extend_from_slice(&self.bytes.to_be_bytes());
        w.extend_from_slice(&self.content_digest);
        put_str(&mut w, self.host_device.as_str());
        put_str(&mut w, self.witness_device.as_str());
        w.extend_from_slice(&self.host_nonce);
        w.extend_from_slice(&self.witness_nonce);
        w.extend_from_slice(&self.at_ms.to_be_bytes());
        w
    }

    /// Sign the transcript with a device controller. Called independently on
    /// each device; secrets never leave the device, only the resulting
    /// signatures.
    pub fn sign(&self, device: &Controller) -> Vec<IndexedSig> {
        device.sign_message(&self.transcript())
    }
}

fn put_str(w: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    w.extend_from_slice(&(b.len() as u32).to_be_bytes());
    w.extend_from_slice(b);
}

/// A complete, mutually-signed storage-served receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServeReceipt {
    /// The signed content.
    pub fields: ReceiptFields,
    /// The host device's signature over the transcript.
    pub host_sig: Vec<IndexedSig>,
    /// The witness device's signature over the transcript.
    pub witness_sig: Vec<IndexedSig>,
}

impl ServeReceipt {
    /// Assemble a receipt from its fields and the two devices' signatures.
    pub fn new(
        fields: ReceiptFields,
        host_sig: Vec<IndexedSig>,
        witness_sig: Vec<IndexedSig>,
    ) -> Self {
        ServeReceipt {
            fields,
            host_sig,
            witness_sig,
        }
    }
}
