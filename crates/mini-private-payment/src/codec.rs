//! Canonical wire encoding: one object, exactly one valid byte string.
//!
//! Every length is written before the bytes it governs, every bound is
//! checked before allocation, and decoding rejects trailing bytes. The
//! discipline matters more here than usual: a claim's transcript is what a
//! ring signature commits to, so two encodings of "the same" payment would
//! be two different payments — and an attacker who can produce a second
//! encoding of a claim can replay it past a nullifier set keyed on
//! anything other than the key image.

use crate::error::{DecodeFailure, Result};

/// Longest byte string any single length-prefixed field may carry. Well
/// above every real field (a compressed point is 32 bytes, a chain
/// reference a few dozen) and far below anything that could exhaust a
/// weak device's memory (Directive 11).
pub const MAX_FIELD_BYTES: usize = 4096;

pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Raw bytes with no length prefix. Only for fixed-width values and
    /// domain separators, where the length is a constant of the format.
    pub fn raw(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    pub fn u8(&mut self, value: u8) {
        self.buf.push(value);
    }

    pub fn u32(&mut self, value: u32) {
        self.buf.extend_from_slice(&value.to_be_bytes());
    }

    pub fn u64(&mut self, value: u64) {
        self.buf.extend_from_slice(&value.to_be_bytes());
    }

    /// A length-prefixed byte string.
    pub fn bytes(&mut self, bytes: &[u8]) {
        self.u32(bytes.len() as u32);
        self.raw(bytes);
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn u8(&mut self) -> Result<u8> {
        let byte = *self.buf.get(self.pos).ok_or(DecodeFailure::Truncated)?;
        self.pos += 1;
        Ok(byte)
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.array::<4>()?))
    }

    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.array::<8>()?))
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let end = self.pos.checked_add(N).ok_or(DecodeFailure::Truncated)?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or(DecodeFailure::Truncated)?;
        self.pos = end;
        Ok(slice.try_into().expect("slice of exactly N"))
    }

    /// A length-prefixed byte string, bounded before allocation.
    pub fn bytes(&mut self) -> Result<Vec<u8>> {
        let len = usize::try_from(self.u32()?).map_err(|_| DecodeFailure::LengthOutOfRange)?;
        if len > MAX_FIELD_BYTES {
            return Err(DecodeFailure::LimitExceeded.into());
        }
        let end = self.pos.checked_add(len).ok_or(DecodeFailure::Truncated)?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or(DecodeFailure::Truncated)?;
        self.pos = end;
        Ok(slice.to_vec())
    }

    /// A 32-byte field element carried as a length-prefixed string, so a
    /// wrong-width point is a decode error rather than a curve error.
    pub fn field_element(&mut self) -> Result<Vec<u8>> {
        let bytes = self.bytes()?;
        if bytes.len() != 32 {
            return Err(DecodeFailure::BadFieldElement.into());
        }
        Ok(bytes)
    }

    /// Every object must consume its input exactly.
    pub fn finish(self) -> Result<()> {
        if self.pos == self.buf.len() {
            Ok(())
        } else {
            Err(DecodeFailure::TrailingBytes.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_round_trip_preserves_every_width() {
        let mut w = Writer::new();
        w.u8(7);
        w.u32(70_000);
        w.u64(u64::MAX);
        w.bytes(b"hello");
        let encoded = w.finish();

        let mut r = Reader::new(&encoded);
        assert_eq!(r.u8().unwrap(), 7);
        assert_eq!(r.u32().unwrap(), 70_000);
        assert_eq!(r.u64().unwrap(), u64::MAX);
        assert_eq!(r.bytes().unwrap(), b"hello");
        assert!(r.finish().is_ok());
    }

    #[test]
    fn trailing_bytes_are_refused() {
        let mut w = Writer::new();
        w.u8(1);
        let mut encoded = w.finish();
        encoded.push(0xff);
        let mut r = Reader::new(&encoded);
        assert_eq!(r.u8().unwrap(), 1);
        assert!(matches!(
            r.finish(),
            Err(crate::PrivatePaymentError::Decode(
                DecodeFailure::TrailingBytes
            ))
        ));
    }

    #[test]
    fn an_oversized_length_prefix_is_refused_before_allocating() {
        // The attack this stops: a two-byte message claiming a 4 GiB field,
        // which a decoder that allocated first would happily try to reserve
        // (Directive 11 -- the weakest device is the one that dies).
        let mut w = Writer::new();
        w.u32(u32::MAX);
        let encoded = w.finish();
        let mut r = Reader::new(&encoded);
        assert!(matches!(
            r.bytes(),
            Err(crate::PrivatePaymentError::Decode(
                DecodeFailure::LimitExceeded
            ))
        ));
    }

    #[test]
    fn a_truncated_field_is_refused() {
        let mut w = Writer::new();
        w.bytes(b"twelve bytes");
        let encoded = w.finish();
        let mut r = Reader::new(&encoded[..encoded.len() - 3]);
        assert!(matches!(
            r.bytes(),
            Err(crate::PrivatePaymentError::Decode(DecodeFailure::Truncated))
        ));
    }

    #[test]
    fn a_field_element_of_the_wrong_width_is_refused() {
        let mut w = Writer::new();
        w.bytes(&[0u8; 31]);
        let encoded = w.finish();
        let mut r = Reader::new(&encoded);
        assert!(matches!(
            r.field_element(),
            Err(crate::PrivatePaymentError::Decode(
                DecodeFailure::BadFieldElement
            ))
        ));
    }
}
