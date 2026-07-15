//! [`PrivateIndexRecord`]: a signed, opaque record a writer publishes at
//! one [`crate::LookupLabel`]. The record's `encrypted_descriptor` bytes
//! are never interpreted by this crate — the actual content encryption
//! is the caller's job (see [`crate::LookupPurpose::RecordEncryption`]'s
//! doc comment); this crate only bounds the size and authenticates the
//! writer.

use did_mini::{Controller, Did, IndexedSig, Kel};

use crate::codec::{Reader, Writer};
use crate::error::{IndexError, Result};
use crate::label::{IndexEpoch, LookupLabel};

/// This module's record format version.
pub const RECORD_VERSION: u8 = 1;

const SIGNING_DOMAIN: &[u8] = b"mininet/mini-private-index/record/v1";

const MAX_DID_BYTES: usize = 256;
const MAX_SIGNATURES: usize = 16;
const MAX_SIG_BYTES: usize = 256;

/// A coarse, fixed set of payload sizes — publishing at a fixed size
/// class (rather than the descriptor's exact length) keeps a record's
/// wire size from itself leaking information about the descriptor it
/// carries (research report §"fixed-size query and response classes").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RecordSizeClass {
    Small,
    Medium,
    Large,
}

impl RecordSizeClass {
    pub const fn max_bytes(self) -> usize {
        match self {
            RecordSizeClass::Small => 256,
            RecordSizeClass::Medium => 1024,
            RecordSizeClass::Large => 4096,
        }
    }

    pub const fn tag(self) -> u8 {
        match self {
            RecordSizeClass::Small => 1,
            RecordSizeClass::Medium => 2,
            RecordSizeClass::Large => 3,
        }
    }

    pub fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            1 => Ok(RecordSizeClass::Small),
            2 => Ok(RecordSizeClass::Medium),
            3 => Ok(RecordSizeClass::Large),
            _ => Err(IndexError::BadSizeClass),
        }
    }
}

/// A signed record published at one [`LookupLabel`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateIndexRecord {
    pub writer: Did,
    pub epoch: IndexEpoch,
    pub lookup_label: LookupLabel,
    pub size_class: RecordSizeClass,
    encrypted_descriptor: Vec<u8>,
    pub expires_at_ms: u64,
    pub sequence: u64,
    signature: Vec<IndexedSig>,
}

impl PrivateIndexRecord {
    /// Issue and sign a record. Rejects an `encrypted_descriptor` that
    /// exceeds `size_class`'s byte budget before signing anything.
    pub fn issue(
        writer: &Controller,
        epoch: IndexEpoch,
        lookup_label: LookupLabel,
        size_class: RecordSizeClass,
        encrypted_descriptor: Vec<u8>,
        expires_at_ms: u64,
        sequence: u64,
    ) -> Result<Self> {
        if encrypted_descriptor.len() > size_class.max_bytes() {
            return Err(IndexError::RecordExceedsSizeClass);
        }
        let mut record = PrivateIndexRecord {
            writer: writer.did(),
            epoch,
            lookup_label,
            size_class,
            encrypted_descriptor,
            expires_at_ms,
            sequence,
            signature: Vec::new(),
        };
        record.signature = writer.sign_message(&record.signing_bytes());
        Ok(record)
    }

    fn signing_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(SIGNING_DOMAIN);
        w.u8(RECORD_VERSION);
        w.bytes(self.writer.as_str().as_bytes());
        w.u64(self.epoch.0);
        w.raw(&self.lookup_label.to_bytes());
        w.u8(self.size_class.tag());
        w.bytes(&self.encrypted_descriptor);
        w.u64(self.expires_at_ms);
        w.u64(self.sequence);
        w.into_bytes()
    }

    /// The record's opaque payload — never decrypted by this crate.
    pub fn encrypted_descriptor(&self) -> &[u8] {
        &self.encrypted_descriptor
    }

    /// Verify the writer's signature and expiry against `now_ms`. Does
    /// **not** check `sequence` against any prior record — that's
    /// [`crate::LocalIndex::write`]'s job, since only the local index
    /// knows what was previously stored at this label.
    pub fn verify(&self, writer_kel: &Kel, now_ms: u64) -> Result<()> {
        if writer_kel.did().as_str() != self.writer.as_str() {
            return Err(IndexError::BadSignature);
        }
        writer_kel
            .verify_message(&self.signing_bytes(), &self.signature)
            .map_err(|_| IndexError::BadSignature)?;
        if now_ms >= self.expires_at_ms {
            return Err(IndexError::Expired);
        }
        Ok(())
    }

    /// Canonical wire bytes (fields + signature).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(RECORD_VERSION);
        w.bytes(self.writer.as_str().as_bytes());
        w.u64(self.epoch.0);
        w.raw(&self.lookup_label.to_bytes());
        w.u8(self.size_class.tag());
        w.bytes(&self.encrypted_descriptor);
        w.u64(self.expires_at_ms);
        w.u64(self.sequence);
        w.u32(self.signature.len() as u32);
        for s in &self.signature {
            w.u32(s.index);
            w.u8(s.signature.suite().tag());
            w.bytes(&s.signature.to_bytes());
        }
        w.into_bytes()
    }

    /// Decode a record from untrusted bytes. Rejects an unrecognized
    /// [`RECORD_VERSION`], an oversized descriptor for its declared size
    /// class, and trailing bytes. Does **not** verify the signature —
    /// call [`PrivateIndexRecord::verify`] with the writer's KEL for
    /// that.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        if r.u8()? != RECORD_VERSION {
            return Err(IndexError::UnsupportedRecordVersion);
        }
        let writer = parse_did(r.bytes_limited(MAX_DID_BYTES)?)?;
        let epoch = IndexEpoch(r.u64()?);
        let lookup_label_bytes: [u8; 32] = r
            .raw(32)?
            .try_into()
            .expect("Reader::raw(32) always returns exactly 32 bytes");
        let lookup_label = LookupLabel::from_bytes(lookup_label_bytes);
        let size_class = RecordSizeClass::from_tag(r.u8()?)?;
        let encrypted_descriptor = r.bytes_limited(size_class.max_bytes())?;
        let expires_at_ms = r.u64()?;
        let sequence = r.u64()?;
        let nsigs = r.u32()? as usize;
        if nsigs > MAX_SIGNATURES {
            return Err(IndexError::LimitExceeded);
        }
        let mut signature = Vec::with_capacity(nsigs);
        for _ in 0..nsigs {
            let index = r.u32()?;
            let sig_suite =
                mini_crypto::SignatureSuite::from_tag(r.u8()?).map_err(IndexError::Crypto)?;
            let sig_bytes = r.bytes_limited(MAX_SIG_BYTES)?;
            let sig = mini_crypto::Signature::from_suite_bytes(sig_suite, &sig_bytes)
                .map_err(IndexError::Crypto)?;
            signature.push(IndexedSig {
                index,
                signature: sig,
            });
        }
        if !r.finished() {
            return Err(IndexError::TrailingBytes);
        }
        Ok(PrivateIndexRecord {
            writer,
            epoch,
            lookup_label,
            size_class,
            encrypted_descriptor,
            expires_at_ms,
            sequence,
            signature,
        })
    }
}

fn parse_did(bytes: Vec<u8>) -> Result<Did> {
    let s = String::from_utf8(bytes).map_err(|_| IndexError::TrailingBytes)?;
    Ok(Did::parse(&s)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_label() -> LookupLabel {
        LookupLabel::from_bytes([9u8; 32])
    }

    fn sample_record(writer: &Controller) -> PrivateIndexRecord {
        PrivateIndexRecord::issue(
            writer,
            IndexEpoch(1),
            sample_label(),
            RecordSizeClass::Small,
            b"opaque-ciphertext".to_vec(),
            2_000,
            1,
        )
        .unwrap()
    }

    #[test]
    fn a_record_verifies_against_its_own_writer_kel_before_expiry() {
        let writer = Controller::incept_single().unwrap();
        let record = sample_record(&writer);
        record.verify(&writer.kel(), 1_000).unwrap();
    }

    #[test]
    fn a_record_is_rejected_at_or_after_its_expiry() {
        let writer = Controller::incept_single().unwrap();
        let record = sample_record(&writer);
        assert_eq!(
            record.verify(&writer.kel(), 2_000),
            Err(IndexError::Expired)
        );
    }

    #[test]
    fn a_record_signed_by_one_writer_does_not_verify_under_another_writers_kel() {
        let writer = Controller::incept_single().unwrap();
        let other = Controller::incept_single().unwrap();
        let record = sample_record(&writer);
        assert_eq!(
            record.verify(&other.kel(), 1_000),
            Err(IndexError::BadSignature)
        );
    }

    #[test]
    fn tampering_with_the_sequence_after_signing_breaks_verification() {
        let writer = Controller::incept_single().unwrap();
        let mut record = sample_record(&writer);
        record.sequence = 99;
        assert_eq!(
            record.verify(&writer.kel(), 1_000),
            Err(IndexError::BadSignature)
        );
    }

    #[test]
    fn issuing_a_descriptor_larger_than_its_size_class_is_rejected_before_signing() {
        let writer = Controller::incept_single().unwrap();
        let oversized = vec![0u8; RecordSizeClass::Small.max_bytes() + 1];
        assert_eq!(
            PrivateIndexRecord::issue(
                &writer,
                IndexEpoch(1),
                sample_label(),
                RecordSizeClass::Small,
                oversized,
                2_000,
                1,
            ),
            Err(IndexError::RecordExceedsSizeClass)
        );
    }

    #[test]
    fn a_record_round_trips_through_bytes() {
        let writer = Controller::incept_single().unwrap();
        let record = sample_record(&writer);
        let decoded = PrivateIndexRecord::from_bytes(&record.to_bytes()).unwrap();
        assert_eq!(decoded, record);
        decoded.verify(&writer.kel(), 1_000).unwrap();
    }

    #[test]
    fn an_unsupported_version_is_rejected() {
        let writer = Controller::incept_single().unwrap();
        let record = sample_record(&writer);
        let mut bytes = record.to_bytes();
        bytes[0] = 99;
        assert_eq!(
            PrivateIndexRecord::from_bytes(&bytes),
            Err(IndexError::UnsupportedRecordVersion)
        );
    }

    #[test]
    fn trailing_bytes_after_a_complete_decode_are_rejected() {
        let writer = Controller::incept_single().unwrap();
        let record = sample_record(&writer);
        let mut bytes = record.to_bytes();
        bytes.push(0xFF);
        assert_eq!(
            PrivateIndexRecord::from_bytes(&bytes),
            Err(IndexError::TrailingBytes)
        );
    }

    #[test]
    fn size_class_byte_budgets_are_distinct_and_increasing() {
        assert!(RecordSizeClass::Small.max_bytes() < RecordSizeClass::Medium.max_bytes());
        assert!(RecordSizeClass::Medium.max_bytes() < RecordSizeClass::Large.max_bytes());
    }

    #[test]
    fn every_size_class_round_trips_through_its_tag() {
        for class in [
            RecordSizeClass::Small,
            RecordSizeClass::Medium,
            RecordSizeClass::Large,
        ] {
            assert_eq!(RecordSizeClass::from_tag(class.tag()).unwrap(), class);
        }
    }
}
