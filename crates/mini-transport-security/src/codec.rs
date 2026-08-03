//! Canonical bounded binary codec helpers.

use crate::{Result, TransportSecurityError};

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

    pub(crate) fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn raw(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }

    pub(crate) fn bytes(&mut self, value: &[u8]) -> Result<()> {
        let length = u32::try_from(value.len()).map_err(|_| TransportSecurityError::LimitExceeded)?;
        self.u32(length);
        self.raw(value);
        Ok(())
    }

    pub(crate) fn string(&mut self, value: &str) -> Result<()> {
        self.bytes(value.as_bytes())
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

#[derive(Debug)]
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    pub(crate) fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(TransportSecurityError::Malformed)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(TransportSecurityError::Truncated)?;
        self.position = end;
        Ok(value)
    }

    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| TransportSecurityError::Malformed)?;
        Ok(u16::from_be_bytes(bytes))
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| TransportSecurityError::Malformed)?;
        Ok(u32::from_be_bytes(bytes))
    }

    pub(crate) fn u64(&mut self) -> Result<u64> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| TransportSecurityError::Malformed)?;
        Ok(u64::from_be_bytes(bytes))
    }

    pub(crate) fn bytes(&mut self, maximum: usize) -> Result<&'a [u8]> {
        let length = self.u32()? as usize;
        if length > maximum {
            return Err(TransportSecurityError::LimitExceeded);
        }
        self.take(length)
    }

    pub(crate) fn string(&mut self, maximum: usize) -> Result<&'a str> {
        core::str::from_utf8(self.bytes(maximum)?).map_err(|_| TransportSecurityError::Malformed)
    }

    pub(crate) fn finished(&self) -> bool {
        self.position == self.bytes.len()
    }
}
