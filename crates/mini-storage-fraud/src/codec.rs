//! The canonical codec for this crate's wire objects, and the shared
//! signature encoding every one of them uses.
//!
//! Conventions, mirroring `mini-attest::codec`: big-endian integers, `u32`
//! length-prefixed variable data, every bound checked *before* allocation,
//! trailing bytes rejected.
//!
//! Two rules beyond that, because these objects are evidence and evidence gets
//! compared byte-for-byte:
//!
//! 1. **Every decodable value must be encodable.** Limits here are the same
//!    limits `did-mini` enforces, so an identity that can legitimately sign an
//!    object can always deserialize its own encoding. A codec whose decoder is
//!    stricter than its encoder silently makes valid identities unable to
//!    exchange their own evidence.
//! 2. **One object, one encoding.** Signature lists must arrive sorted by key
//!    index with no repeats, so two byte-different encodings can never carry
//!    the same meaning — otherwise "is this the same claim I already saw" stops
//!    being answerable by comparing bytes.

use did_mini::IndexedSig;
use mini_crypto::{Signature, SignatureSuite};

use crate::error::{DecodeFailure, Result};

/// Matches `did-mini`'s own `MAX_SIGNATURES`: a threshold identity may hold up
/// to 32 keys and produce up to 64 signatures, and every one of those must
/// survive a round trip through this codec.
pub(crate) const MAX_SIGNATURES: usize = 64;
/// Matches `did-mini`'s own `MAX_SIGNATURE_BYTES`.
pub(crate) const MAX_SIGNATURE_BYTES: usize = 4096;
/// Matches `did-mini`'s own `MAX_DID_BYTES`.
pub(crate) const MAX_DID_BYTES: usize = 256;
/// Multihash bytes of a KEL event digest; matches `did-mini`'s
/// `MAX_PRIOR_BYTES`, which bounds the same value inside a key event.
pub(crate) const MAX_EVENT_DIGEST_BYTES: usize = 128;

#[derive(Debug, Default)]
pub(crate) struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    pub(crate) fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub(crate) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub(crate) fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    /// A `usize` that is semantically a count of protocol objects. Encoded as
    /// `u64` so the wire form does not depend on the encoder's pointer width.
    pub(crate) fn count(&mut self, value: usize) {
        self.u64(value as u64);
    }

    pub(crate) fn raw(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }

    pub(crate) fn bytes(&mut self, value: &[u8]) {
        self.u32(value.len() as u32);
        self.raw(value);
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

#[derive(Debug)]
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(DecodeFailure::LimitExceeded)?;
        if end > self.bytes.len() {
            return Err(DecodeFailure::Truncated.into());
        }
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .expect("take(4) returns four bytes");
        Ok(u32::from_be_bytes(bytes))
    }

    pub(crate) fn u64(&mut self) -> Result<u64> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .expect("take(8) returns eight bytes");
        Ok(u64::from_be_bytes(bytes))
    }

    /// A count written by [`Writer::count`]. Checked, never truncated: on a
    /// 32-bit target a `u64` that does not fit is a decode failure, not a
    /// silently different number.
    pub(crate) fn count(&mut self, maximum: usize) -> Result<usize> {
        let raw = self.u64()?;
        let value = usize::try_from(raw).map_err(|_| DecodeFailure::LengthOutOfRange)?;
        if value > maximum {
            return Err(DecodeFailure::LimitExceeded.into());
        }
        Ok(value)
    }

    pub(crate) fn raw_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self
            .take(N)?
            .try_into()
            .expect("take(N) returns exactly N bytes"))
    }

    pub(crate) fn bytes_limited(&mut self, maximum: usize) -> Result<Vec<u8>> {
        let length = usize::try_from(self.u32()?).map_err(|_| DecodeFailure::LengthOutOfRange)?;
        if length > maximum {
            return Err(DecodeFailure::LimitExceeded.into());
        }
        Ok(self.take(length)?.to_vec())
    }

    pub(crate) fn finish(self) -> Result<()> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(DecodeFailure::TrailingBytes.into())
        }
    }
}

/// Encode a signature list in canonical order.
///
/// The caller is responsible for having sorted it; [`canonicalize_signatures`]
/// is what every constructor in this crate uses so no object is ever built
/// holding an encoding its own decoder would reject.
pub(crate) fn encode_signatures(writer: &mut Writer, signatures: &[IndexedSig]) {
    writer.count(signatures.len());
    for signature in signatures {
        writer.u32(signature.index);
        writer.u8(signature.signature.suite().tag());
        writer.bytes(&signature.signature.to_bytes());
    }
}

pub(crate) fn decode_signatures(reader: &mut Reader<'_>) -> Result<Vec<IndexedSig>> {
    let count = reader.count(MAX_SIGNATURES)?;
    let mut signatures: Vec<IndexedSig> = Vec::with_capacity(count);
    for _ in 0..count {
        let index = reader.u32()?;
        let suite = SignatureSuite::from_tag(reader.u8()?)
            .map_err(|_| DecodeFailure::InvalidSignatureEncoding)?;
        let bytes = reader.bytes_limited(MAX_SIGNATURE_BYTES)?;
        let signature = Signature::from_suite_bytes(suite, &bytes)
            .map_err(|_| DecodeFailure::InvalidSignatureEncoding)?;
        if let Some(previous) = signatures.last() {
            if previous.index >= index {
                return Err(DecodeFailure::NoncanonicalSignatureOrder.into());
            }
        }
        signatures.push(IndexedSig { index, signature });
    }
    Ok(signatures)
}

/// Sort a freshly produced signature list into the one canonical order, and
/// drop any repeated key index.
///
/// `Controller::sign_message` already emits one signature per current key in
/// index order, so this is normally a no-op — it exists so that stays true by
/// construction rather than by luck, and so a caller assembling signatures from
/// several devices cannot accidentally build an object that fails its own
/// decoder.
pub(crate) fn canonicalize_signatures(mut signatures: Vec<IndexedSig>) -> Vec<IndexedSig> {
    signatures.sort_by_key(|signature| signature.index);
    signatures.dedup_by_key(|signature| signature.index);
    signatures
}

#[cfg(test)]
mod tests {
    use did_mini::Controller;

    use super::*;

    fn signatures() -> Vec<IndexedSig> {
        Controller::incept_single_from_seeds(&[1u8; 32], &[2u8; 32])
            .unwrap()
            .sign_message(b"canonical codec test")
    }

    #[test]
    fn a_signature_list_round_trips() {
        let signatures = signatures();
        let mut writer = Writer::new();
        encode_signatures(&mut writer, &signatures);
        let bytes = writer.finish();
        let mut reader = Reader::new(&bytes);
        assert_eq!(decode_signatures(&mut reader).unwrap(), signatures);
        assert!(reader.finish().is_ok());
    }

    #[test]
    fn repeated_signature_indices_are_rejected_as_noncanonical() {
        let mut signatures = signatures();
        signatures.push(signatures[0].clone());
        let mut writer = Writer::new();
        encode_signatures(&mut writer, &signatures);
        let bytes = writer.finish();
        assert_eq!(
            decode_signatures(&mut Reader::new(&bytes)),
            Err(DecodeFailure::NoncanonicalSignatureOrder.into())
        );
        assert_eq!(canonicalize_signatures(signatures).len(), 1);
    }

    #[test]
    fn an_invalid_suite_tag_is_not_reported_as_truncation() {
        let mut writer = Writer::new();
        writer.count(1);
        writer.u32(0);
        writer.u8(0xEE);
        writer.bytes(&[0u8; 64]);
        let bytes = writer.finish();
        assert_eq!(
            decode_signatures(&mut Reader::new(&bytes)),
            Err(DecodeFailure::InvalidSignatureEncoding.into())
        );
    }

    #[test]
    fn a_malformed_signature_body_is_not_reported_as_truncation() {
        let real = signatures();
        let mut writer = Writer::new();
        writer.count(1);
        writer.u32(0);
        writer.u8(real[0].signature.suite().tag());
        writer.bytes(&[0u8; 7]);
        let bytes = writer.finish();
        assert_eq!(
            decode_signatures(&mut Reader::new(&bytes)),
            Err(DecodeFailure::InvalidSignatureEncoding.into())
        );
    }

    #[test]
    fn a_count_past_the_limit_is_rejected_before_allocating() {
        let mut writer = Writer::new();
        writer.count(MAX_SIGNATURES + 1);
        let bytes = writer.finish();
        assert_eq!(
            decode_signatures(&mut Reader::new(&bytes)),
            Err(DecodeFailure::LimitExceeded.into())
        );
    }

    #[test]
    fn a_count_that_cannot_fit_this_platform_is_rejected_not_truncated() {
        let mut writer = Writer::new();
        writer.u64(u64::MAX);
        let bytes = writer.finish();
        let mut reader = Reader::new(&bytes);
        assert!(matches!(
            reader.count(16),
            Err(crate::FraudError::Decode(
                DecodeFailure::LengthOutOfRange | DecodeFailure::LimitExceeded
            ))
        ));
    }
}
