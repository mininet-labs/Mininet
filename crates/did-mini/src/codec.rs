//! A tiny, deterministic, length-prefixed binary codec.
//!
//! We hand-roll the minimal codec the KEL needs rather than pull a serialization
//! framework, for the same reason `mini-crypto` hand-rolls multihash: the
//! security-critical path stays small and auditable, and the byte layout is
//! fully determined (no field-ordering or canonicalisation ambiguity). The exact
//! same bytes are produced on every platform, which is what lets a digest or a
//! signature computed on one device verify on another.
//!
//! Layout primitives:
//!   - `u8`            : one byte
//!   - `u32` / `u64`   : big-endian, fixed width
//!   - `bytes`         : a `u32` big-endian length followed by that many bytes

use crate::error::{IdentityError, Result};

/// Append-only writer over a byte buffer.
#[derive(Debug, Default)]
pub(crate) struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub(crate) fn new() -> Self {
        Writer::default()
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub(crate) fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub(crate) fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub(crate) fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    /// A length-prefixed byte string: `u32` big-endian length, then the bytes.
    pub(crate) fn bytes(&mut self, b: &[u8]) {
        self.u32(b.len() as u32);
        self.buf.extend_from_slice(b);
    }
}

/// Cursor-based reader over a byte buffer.
#[derive(Debug)]
pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    /// True when every byte has been consumed.
    pub(crate) fn finished(&self) -> bool {
        self.pos == self.buf.len()
    }

    pub(crate) fn u8(&mut self) -> Result<u8> {
        let b = *self.buf.get(self.pos).ok_or(IdentityError::Truncated)?;
        self.pos += 1;
        Ok(b)
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        let end = self.pos.checked_add(4).ok_or(IdentityError::Truncated)?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or(IdentityError::Truncated)?;
        let arr: [u8; 4] = slice.try_into().map_err(|_| IdentityError::Truncated)?;
        self.pos = end;
        Ok(u32::from_be_bytes(arr))
    }

    pub(crate) fn u64(&mut self) -> Result<u64> {
        let end = self.pos.checked_add(8).ok_or(IdentityError::Truncated)?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or(IdentityError::Truncated)?;
        let arr: [u8; 8] = slice.try_into().map_err(|_| IdentityError::Truncated)?;
        self.pos = end;
        Ok(u64::from_be_bytes(arr))
    }

    pub(crate) fn bytes_limited(&mut self, field: &'static str, max: usize) -> Result<Vec<u8>> {
        let len = self.u32()? as usize;
        if len > max {
            return Err(IdentityError::FieldTooLarge {
                field,
                max,
                got: len,
            });
        }
        let end = self.pos.checked_add(len).ok_or(IdentityError::Truncated)?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or(IdentityError::Truncated)?;
        let out = slice.to_vec();
        self.pos = end;
        Ok(out)
    }
}
