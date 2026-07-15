//! Deterministic, length-prefixed binary codec — the same discipline as
//! `mini-relay`/`mini-bridge`/`did-mini`: big-endian integers, u32-length-
//! prefixed byte strings, and hard caps applied *before* allocation when
//! decoding untrusted input.

use crate::error::{IndexError, Result};

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
    /// Write exactly `v.len()` bytes with no length prefix — for fields
    /// whose length is fixed by the format itself.
    pub(crate) fn raw(&mut self, v: &[u8]) {
        self.buf.extend_from_slice(v);
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
            return Err(IndexError::Truncated);
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
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
    /// Read a length-prefixed byte string, refusing lengths above `max`.
    pub(crate) fn bytes_limited(&mut self, max: usize) -> Result<Vec<u8>> {
        let len = self.u32()? as usize;
        if len > max {
            return Err(IndexError::LimitExceeded);
        }
        Ok(self.take(len)?.to_vec())
    }
    pub(crate) fn finished(&self) -> bool {
        self.pos == self.data.len()
    }
    /// Read exactly `n` raw bytes with no length prefix.
    pub(crate) fn raw(&mut self, n: usize) -> Result<&'a [u8]> {
        self.take(n)
    }
}
