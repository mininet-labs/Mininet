//! The private inner object (`MN-103`): everything a v1 [`crate::Object`]
//! exposes in cleartext — type, authors, timestamp, sequence, links,
//! signatures — moves inside [`crate::envelope_v2::ObjectEnvelopeV2`]'s
//! AEAD ciphertext instead. This module defines that inner form's own
//! canonical encoding and typed-domain signing; it never travels or is
//! stored on its own.

use did_mini::{Controller, Did, IndexedSig, Kel};
use mini_crypto::{Signature, SignatureSuite};

use crate::codec::{Reader, Writer};
use crate::error::{ObjectError, Result};
use crate::object::{Link, ObjectId, ObjectType};

const MAX_APPLICATION_METADATA_BYTES: usize = 64 * 1024;
const MAX_PAYLOAD_BYTES: usize = crate::MAX_PAYLOAD_BYTES;
const MAX_LINKS: usize = crate::MAX_LINKS;
const MAX_TYPE_BYTES: usize = 64;
const MAX_DID_BYTES: usize = 256;
const MAX_REL_BYTES: usize = 32;
const MAX_ID_BYTES: usize = 128;
const MAX_SIGNATURES: usize = 16;
const MAX_SIG_BYTES: usize = 256;

/// The typed-domain prefix bound into [`PrivateObject::signing_bytes`] —
/// never a generic `sign(bytes)` call on caller-assembled data. Distinct
/// from v1's implicit domain separation (v1 relies on its own envelope
/// version byte being unique workspace-wide); `PrivateObject` is a new
/// type with its own namespace, so it states its domain explicitly.
const SIGNING_DOMAIN: &[u8] = b"mininet/mini-objects/private-object/v1";

/// The decrypted content of an [`crate::envelope_v2::ObjectEnvelopeV2`] —
/// application semantics, authorship, links, and signatures that stay
/// outside the public outer envelope entirely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateObject {
    pub object_type: ObjectType,
    pub author_human: Did,
    pub author_device: Did,
    pub timestamp_ms: u64,
    pub sequence: u64,
    pub links: Vec<Link>,
    /// Opaque, application-defined extra bytes this layer never interprets.
    pub application_metadata: Vec<u8>,
    /// The content. Opaque to this layer — the application decides its own
    /// sub-encoding, the same way v1's `Payload::Public`/`Encrypted` bytes are
    /// opaque to `mini-objects` itself.
    pub payload: Vec<u8>,
    signatures: Vec<IndexedSig>,
}

impl PrivateObject {
    /// Build an unsigned private object. Call [`PrivateObject::sign_with`]
    /// before sealing it into an envelope.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        object_type: ObjectType,
        author_human: Did,
        author_device: Did,
        timestamp_ms: u64,
        sequence: u64,
        links: Vec<Link>,
        application_metadata: Vec<u8>,
        payload: Vec<u8>,
    ) -> Self {
        PrivateObject {
            object_type,
            author_human,
            author_device,
            timestamp_ms,
            sequence,
            links,
            application_metadata,
            payload,
            signatures: Vec::new(),
        }
    }

    /// The device signatures, if signed.
    pub fn signatures(&self) -> &[IndexedSig] {
        &self.signatures
    }

    /// Sign with the authoring device's current keys, replacing any
    /// previous signatures. Mirrors `did-mini`/`mini-objects`' existing
    /// pattern: a typed, domain-prefixed message is built internally and
    /// handed to `Controller::sign_message`, never raw caller bytes.
    pub fn sign_with(mut self, device: &Controller) -> Self {
        self.signatures = device.sign_message(&self.signing_bytes());
        self
    }

    /// Canonical bytes without signatures, domain-prefixed — what gets signed.
    fn signing_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(SIGNING_DOMAIN);
        self.encode_fields(&mut w);
        w.into_bytes()
    }

    /// Canonical full bytes (fields + signatures) — what the envelope
    /// encrypts and what `from_bytes` parses back.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        self.encode_fields(&mut w);
        w.u32(self.signatures.len() as u32);
        for s in &self.signatures {
            w.u32(s.index);
            w.u8(s.signature.suite().tag());
            w.bytes(&s.signature.to_bytes());
        }
        w.into_bytes()
    }

    fn encode_fields(&self, w: &mut Writer) {
        match &self.object_type {
            ObjectType::WellKnown(t) => {
                w.u16(*t);
                w.bytes(b"");
            }
            ObjectType::Custom(name) => {
                w.u16(u16::MAX);
                w.bytes(name.as_bytes());
            }
        }
        w.bytes(self.author_human.as_str().as_bytes());
        w.bytes(self.author_device.as_str().as_bytes());
        w.u64(self.timestamp_ms);
        w.u64(self.sequence);
        w.u32(self.links.len() as u32);
        for l in &self.links {
            w.bytes(l.rel.as_bytes());
            w.bytes(l.target.as_str().as_bytes());
        }
        w.bytes(&self.application_metadata);
        w.bytes(&self.payload);
    }

    /// Decode a private object from already-decrypted bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<PrivateObject> {
        let mut r = Reader::new(bytes);
        let type_tag = r.u16()?;
        let type_name = r.bytes_limited(MAX_TYPE_BYTES)?;
        let object_type = if type_tag == u16::MAX {
            let name = String::from_utf8(type_name).map_err(|_| ObjectError::BadObject)?;
            if name.is_empty() {
                return Err(ObjectError::BadObject);
            }
            ObjectType::Custom(name)
        } else {
            if !type_name.is_empty() {
                return Err(ObjectError::BadObject);
            }
            ObjectType::WellKnown(type_tag)
        };
        let author_human = parse_did(r.bytes_limited(MAX_DID_BYTES)?)?;
        let author_device = parse_did(r.bytes_limited(MAX_DID_BYTES)?)?;
        let timestamp_ms = r.u64()?;
        let sequence = r.u64()?;
        let nlinks = r.u32()? as usize;
        if nlinks > MAX_LINKS {
            return Err(ObjectError::LimitExceeded);
        }
        let mut links = Vec::with_capacity(nlinks);
        for _ in 0..nlinks {
            let rel_bytes = r.bytes_limited(MAX_REL_BYTES)?;
            let rel = String::from_utf8(rel_bytes).map_err(|_| ObjectError::BadObject)?;
            if rel.is_empty() {
                return Err(ObjectError::BadObject);
            }
            let id_bytes = r.bytes_limited(MAX_ID_BYTES)?;
            let id_str = String::from_utf8(id_bytes).map_err(|_| ObjectError::BadObject)?;
            links.push(Link {
                rel,
                target: ObjectId::parse(&id_str)?,
            });
        }
        let application_metadata = r.bytes_limited(MAX_APPLICATION_METADATA_BYTES)?;
        let payload = r.bytes_limited(MAX_PAYLOAD_BYTES)?;
        let nsigs = r.u32()? as usize;
        if nsigs > MAX_SIGNATURES {
            return Err(ObjectError::LimitExceeded);
        }
        let mut signatures = Vec::with_capacity(nsigs);
        for _ in 0..nsigs {
            let index = r.u32()?;
            let sig_suite = SignatureSuite::from_tag(r.u8()?).map_err(ObjectError::Crypto)?;
            let sig_bytes = r.bytes_limited(MAX_SIG_BYTES)?;
            let signature =
                Signature::from_suite_bytes(sig_suite, &sig_bytes).map_err(ObjectError::Crypto)?;
            signatures.push(IndexedSig { index, signature });
        }
        if !r.finished() {
            return Err(ObjectError::TrailingBytes);
        }
        Ok(PrivateObject {
            object_type,
            author_human,
            author_device,
            timestamp_ms,
            sequence,
            links,
            application_metadata,
            payload,
            signatures,
        })
    }

    /// Verify the named device signed this private object, against the
    /// device's KEL. Only meaningful after the envelope has already been
    /// opened — nothing here or in `ObjectEnvelopeV2::open` implicitly
    /// checks this; a caller who needs authenticity must call it.
    pub fn verify_signature(&self, device: &Kel) -> Result<()> {
        if device.did().as_str() != self.author_device.as_str() {
            return Err(ObjectError::DeviceMismatch);
        }
        device
            .verify_message(&self.signing_bytes(), &self.signatures)
            .map_err(ObjectError::Identity)
    }
}

fn parse_did(bytes: Vec<u8>) -> Result<Did> {
    let s = String::from_utf8(bytes).map_err(|_| ObjectError::BadObject)?;
    Did::parse(&s).map_err(ObjectError::Identity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;

    fn sample() -> (Controller, PrivateObject) {
        let device = Controller::incept_single().unwrap();
        let obj = PrivateObject::new(
            ObjectType::POST,
            device.did(),
            device.did(),
            1_000,
            1,
            vec![Link {
                rel: "re".to_string(),
                target: ObjectId::of(b"linked-object"),
            }],
            b"meta".to_vec(),
            b"hello, private world".to_vec(),
        );
        (device, obj)
    }

    #[test]
    fn a_signed_private_object_round_trips_through_bytes() {
        let (device, obj) = sample();
        let signed = obj.sign_with(&device);
        let decoded = PrivateObject::from_bytes(&signed.to_bytes()).unwrap();
        assert_eq!(decoded, signed);
    }

    #[test]
    fn a_valid_signature_verifies_against_the_devices_kel() {
        let (device, obj) = sample();
        let signed = obj.sign_with(&device);
        signed.verify_signature(&device.kel()).unwrap();
    }

    #[test]
    fn a_tampered_field_after_signing_fails_verification() {
        let (device, obj) = sample();
        let mut signed = obj.sign_with(&device);
        signed.sequence += 1;
        assert!(signed.verify_signature(&device.kel()).is_err());
    }

    #[test]
    fn verification_fails_against_the_wrong_devices_kel() {
        let (device, obj) = sample();
        let signed = obj.sign_with(&device);
        let other = Controller::incept_single().unwrap();
        assert!(signed.verify_signature(&other.kel()).is_err());
    }

    #[test]
    fn a_truncated_encoding_is_rejected_at_every_length() {
        let (device, obj) = sample();
        let full = obj.sign_with(&device).to_bytes();
        for cut in 0..full.len() {
            assert!(
                PrivateObject::from_bytes(&full[..cut]).is_err(),
                "truncating to {cut} bytes must be rejected"
            );
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let (device, obj) = sample();
        let mut bytes = obj.sign_with(&device).to_bytes();
        bytes.push(0xff);
        assert_eq!(
            PrivateObject::from_bytes(&bytes),
            Err(ObjectError::TrailingBytes)
        );
    }

    #[test]
    fn an_over_cap_link_count_is_rejected_before_allocating() {
        let device = Controller::incept_single().unwrap();
        let did_bytes = device.did().as_str().as_bytes().to_vec();
        let mut w = Writer::new();
        w.u16(1); // POST
        w.bytes(b"");
        w.bytes(&did_bytes);
        w.bytes(&did_bytes);
        w.u64(0);
        w.u64(0);
        w.u32((MAX_LINKS as u32) + 1);
        assert_eq!(
            PrivateObject::from_bytes(&w.into_bytes()),
            Err(ObjectError::LimitExceeded)
        );
    }

    #[test]
    fn an_unsigned_private_object_has_no_signatures() {
        let (_device, obj) = sample();
        assert!(obj.signatures().is_empty());
    }

    #[test]
    fn custom_object_types_round_trip() {
        let (device, mut obj) = sample();
        obj.object_type = ObjectType::Custom("chess/move".to_string());
        let signed = obj.sign_with(&device);
        let decoded = PrivateObject::from_bytes(&signed.to_bytes()).unwrap();
        assert_eq!(
            decoded.object_type,
            ObjectType::Custom("chess/move".to_string())
        );
    }
}
