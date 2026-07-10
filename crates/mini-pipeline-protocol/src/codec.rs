//! Shared length-prefixed primitive encode/decode helpers, the same
//! pattern `mini-forge`/`mini-provenance`/`mini-porep` all use.

use mini_pipeline::Capability;

use crate::error::{ProtocolError, Result};

/// Maximum bytes for any single length-prefixed string field in this
/// protocol (a capability string, a version string).
pub const MAX_FIELD_BYTES: usize = 4096;

pub(crate) fn put_str(w: &mut Vec<u8>, s: &str) {
    w.extend_from_slice(&(s.len() as u32).to_be_bytes());
    w.extend_from_slice(s.as_bytes());
}

pub(crate) fn take_str(b: &[u8], off: &mut usize) -> Option<String> {
    if *off + 4 > b.len() {
        return None;
    }
    let len = u32::from_be_bytes([b[*off], b[*off + 1], b[*off + 2], b[*off + 3]]) as usize;
    *off += 4;
    if *off + len > b.len() || len > MAX_FIELD_BYTES {
        return None;
    }
    let s = String::from_utf8(b[*off..*off + len].to_vec()).ok()?;
    *off += len;
    Some(s)
}

pub(crate) fn put_digest(w: &mut Vec<u8>, d: &[u8; 32]) {
    w.extend_from_slice(d);
}

pub(crate) fn take_digest(b: &[u8], off: &mut usize) -> Option<[u8; 32]> {
    if *off + 32 > b.len() {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&b[*off..*off + 32]);
    *off += 32;
    Some(out)
}

pub(crate) fn put_u64(w: &mut Vec<u8>, v: u64) {
    w.extend_from_slice(&v.to_be_bytes());
}

pub(crate) fn take_u64(b: &[u8], off: &mut usize) -> Option<u64> {
    if *off + 8 > b.len() {
        return None;
    }
    let v = u64::from_be_bytes(b[*off..*off + 8].try_into().ok()?);
    *off += 8;
    Some(v)
}

pub(crate) fn put_capabilities(w: &mut Vec<u8>, caps: &[Capability]) {
    w.extend_from_slice(&(caps.len() as u32).to_be_bytes());
    for c in caps {
        put_str(w, &c.to_canonical_string());
    }
}

pub(crate) fn take_capabilities(b: &[u8], off: &mut usize, max: usize) -> Result<Vec<Capability>> {
    if *off + 4 > b.len() {
        return Err(ProtocolError::BadMessage);
    }
    let n = u32::from_be_bytes([b[*off], b[*off + 1], b[*off + 2], b[*off + 3]]) as usize;
    *off += 4;
    if n > max {
        return Err(ProtocolError::BadMessage);
    }
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let s = take_str(b, off).ok_or(ProtocolError::BadMessage)?;
        out.push(Capability::parse(&s).map_err(|_| ProtocolError::BadMessage)?);
    }
    Ok(out)
}

pub(crate) fn put_digests(w: &mut Vec<u8>, ds: &[[u8; 32]]) {
    w.extend_from_slice(&(ds.len() as u32).to_be_bytes());
    for d in ds {
        put_digest(w, d);
    }
}

pub(crate) fn take_digests(b: &[u8], off: &mut usize, max: usize) -> Result<Vec<[u8; 32]>> {
    if *off + 4 > b.len() {
        return Err(ProtocolError::BadMessage);
    }
    let n = u32::from_be_bytes([b[*off], b[*off + 1], b[*off + 2], b[*off + 3]]) as usize;
    *off += 4;
    if n > max {
        return Err(ProtocolError::BadMessage);
    }
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(take_digest(b, off).ok_or(ProtocolError::BadMessage)?);
    }
    Ok(out)
}
