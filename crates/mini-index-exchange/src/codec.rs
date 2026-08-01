//! Deterministic, length-prefixed binary codec — the same discipline as
//! `mini-lexical-index`/`mini-extract-protocol`: big-endian integers,
//! u32-length-prefixed byte strings, hard caps before allocation.
//!
//! Both the signed message (which must be byte-identical on the signing
//! and verifying sides) and the wire publication share this encoding, so a
//! publication produced anywhere verifies anywhere.

use crate::error::{ExchangeError, Result};

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
    pub(crate) fn bytes(&mut self, v: &[u8]) {
        self.u32(v.len() as u32);
        self.buf.extend_from_slice(v);
    }
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

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
        let end = self.pos.checked_add(n).ok_or(ExchangeError::Truncated)?;
        if end > self.data.len() {
            return Err(ExchangeError::Truncated);
        }
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn bytes_limited(&mut self, max: usize) -> Result<Vec<u8>> {
        let len = self.u32()? as usize;
        if len > max {
            return Err(ExchangeError::LimitExceeded);
        }
        Ok(self.take(len)?.to_vec())
    }

    pub(crate) fn finished(&self) -> bool {
        self.pos == self.data.len()
    }
}
