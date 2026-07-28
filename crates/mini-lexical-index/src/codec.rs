//! Deterministic, length-prefixed binary codec — the same discipline as
//! `mini-intake-types`/`mini-extract-protocol`: big-endian integers,
//! u32-length-prefixed byte strings, hard caps applied before allocation.
//!
//! An index segment's bytes are the input to its content address, so the
//! encoding must be canonical: the same logical index must always produce
//! byte-identical output. That property is the builder's responsibility
//! (it emits terms and documents in sorted order); this codec only
//! provides the primitives and the bounded decode path.

use crate::error::{LexicalIndexError, Result};

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
    pub(crate) fn str(&mut self, v: &str) {
        self.bytes(v.as_bytes());
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
        let end = self
            .pos
            .checked_add(n)
            .ok_or(LexicalIndexError::Truncated)?;
        if end > self.data.len() {
            return Err(LexicalIndexError::Truncated);
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
            return Err(LexicalIndexError::LimitExceeded);
        }
        Ok(self.take(len)?.to_vec())
    }

    pub(crate) fn str_limited(&mut self, max: usize) -> Result<String> {
        let bytes = self.bytes_limited(max)?;
        String::from_utf8(bytes).map_err(|_| LexicalIndexError::NotUtf8)
    }

    pub(crate) fn count_limited(&mut self, max: usize) -> Result<usize> {
        let n = self.u32()? as usize;
        if n > max {
            return Err(LexicalIndexError::LimitExceeded);
        }
        Ok(n)
    }

    pub(crate) fn finished(&self) -> bool {
        self.pos == self.data.len()
    }
}
