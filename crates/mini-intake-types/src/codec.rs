//! Deterministic, length-prefixed binary codec — the same discipline as
//! `mini-relay`/`mini-bridge`/`mini-private-index`: big-endian integers,
//! u32-length-prefixed byte strings, and hard caps applied *before*
//! allocation when decoding untrusted input.

use crate::error::{IntakeError, Result};

/// Serialises fields in canonical order.
#[derive(Debug, Default)]
pub(crate) struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub(crate) fn new() -> Self {
        Writer { buf: Vec::new() }
    }
    pub(crate) fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    pub(crate) fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
    pub(crate) fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
    pub(crate) fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
    pub(crate) fn bytes(&mut self, v: &[u8]) {
        self.u32(v.len() as u32);
        self.buf.extend_from_slice(v);
    }
    pub(crate) fn str(&mut self, v: &str) {
        self.bytes(v.as_bytes());
    }
    pub(crate) fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

/// Reads fields back in the same canonical order, refusing oversized
/// lengths before allocating.
#[derive(Debug)]
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.pos + n > self.data.len() {
            return Err(IntakeError::Truncated);
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub(crate) fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    pub(crate) fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub(crate) fn u64(&mut self) -> Result<u64> {
        let b = self.take(8)?;
        Ok(u64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
    pub(crate) fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }
    /// Read a length-prefixed byte string, refusing lengths above `max`.
    pub(crate) fn bytes_limited(&mut self, max: usize) -> Result<Vec<u8>> {
        let len = self.u32()? as usize;
        if len > max {
            return Err(IntakeError::LimitExceeded);
        }
        Ok(self.take(len)?.to_vec())
    }
    /// Read a length-prefixed UTF-8 string, refusing lengths above `max`
    /// bytes and non-UTF-8 content.
    pub(crate) fn str_limited(&mut self, max: usize) -> Result<String> {
        let bytes = self.bytes_limited(max)?;
        String::from_utf8(bytes).map_err(|_| IntakeError::TrailingBytes)
    }
    pub(crate) fn finished(&self) -> bool {
        self.pos == self.data.len()
    }
}
