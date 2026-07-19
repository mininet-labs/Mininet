//! Shared length-prefixed primitive encode/decode helpers, the same
//! pattern `mini-pipeline-protocol`/`mini-forge`/`mini-provenance`/
//! `mini-porep` all use.

use crate::error::{ProtocolError, Result};

/// Maximum bytes for any single length-prefixed string field in this
/// protocol (an error message).
pub const MAX_FIELD_BYTES: usize = 4096;

pub(crate) fn put_str(w: &mut Vec<u8>, s: &str) {
    w.extend_from_slice(&(s.len() as u32).to_be_bytes());
    w.extend_from_slice(s.as_bytes());
}

pub(crate) fn take_str(b: &[u8], off: &mut usize) -> Result<String> {
    if *off + 4 > b.len() {
        return Err(ProtocolError::BadMessage);
    }
    let len = u32::from_be_bytes([b[*off], b[*off + 1], b[*off + 2], b[*off + 3]]) as usize;
    *off += 4;
    if *off + len > b.len() || len > MAX_FIELD_BYTES {
        return Err(ProtocolError::BadMessage);
    }
    let s =
        String::from_utf8(b[*off..*off + len].to_vec()).map_err(|_| ProtocolError::BadMessage)?;
    *off += len;
    Ok(s)
}

pub(crate) fn put_bytes(w: &mut Vec<u8>, bytes: &[u8]) {
    w.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    w.extend_from_slice(bytes);
}

pub(crate) fn take_bytes(b: &[u8], off: &mut usize, max: usize) -> Result<Vec<u8>> {
    if *off + 4 > b.len() {
        return Err(ProtocolError::BadMessage);
    }
    let len = u32::from_be_bytes([b[*off], b[*off + 1], b[*off + 2], b[*off + 3]]) as usize;
    *off += 4;
    if *off + len > b.len() || len > max {
        return Err(ProtocolError::BadMessage);
    }
    let out = b[*off..*off + len].to_vec();
    *off += len;
    Ok(out)
}

pub(crate) fn put_u8(w: &mut Vec<u8>, v: u8) {
    w.push(v);
}

pub(crate) fn take_u8(b: &[u8], off: &mut usize) -> Result<u8> {
    if *off + 1 > b.len() {
        return Err(ProtocolError::BadMessage);
    }
    let v = b[*off];
    *off += 1;
    Ok(v)
}

pub(crate) fn put_u32(w: &mut Vec<u8>, v: u32) {
    w.extend_from_slice(&v.to_be_bytes());
}

pub(crate) fn take_u32(b: &[u8], off: &mut usize) -> Result<u32> {
    if *off + 4 > b.len() {
        return Err(ProtocolError::BadMessage);
    }
    let v = u32::from_be_bytes(b[*off..*off + 4].try_into().unwrap());
    *off += 4;
    Ok(v)
}
