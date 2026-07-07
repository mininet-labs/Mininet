//! Minimal multibase encoding for self-describing identifiers.
//!
//! Multibase prefixes a base-encoded string with a single character naming the
//! base, so the string is self-describing. We support the prefixes Mininet
//! identifiers use today:
//!   - `z` : base58btc — default for `did:mini` SCIDs and content ids
//!   - `f` : base16 (hex, lower-case)
//!
//! Additional bases (base32 `b`, base64url `u`) are easy to add later without
//! changing any call site.

use crate::error::{CryptoError, Result};

/// Multibase prefix for base58btc.
pub const BASE58BTC: char = 'z';
/// Multibase prefix for lower-case hex.
pub const BASE16: char = 'f';

/// Encode `data` as a multibase string with the given base `prefix`.
pub fn encode(prefix: char, data: &[u8]) -> Result<String> {
    let body = match prefix {
        BASE58BTC => bs58::encode(data).into_string(),
        BASE16 => hex_lower(data),
        other => return Err(CryptoError::UnsupportedMultibase(other)),
    };
    let mut s = String::with_capacity(body.len() + 1);
    s.push(prefix);
    s.push_str(&body);
    Ok(s)
}

/// Decode a multibase string back to raw bytes.
pub fn decode(s: &str) -> Result<Vec<u8>> {
    let prefix = s.chars().next().ok_or(CryptoError::EmptyMultibase)?;
    let body = &s[prefix.len_utf8()..];
    match prefix {
        BASE58BTC => bs58::decode(body)
            .into_vec()
            .map_err(|_| CryptoError::BadEncoding),
        BASE16 => hex_decode(body),
        other => Err(CryptoError::UnsupportedMultibase(other)),
    }
}

fn hex_lower(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(data.len() * 2);
    for &b in data {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

fn hex_decode(s: &str) -> Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        return Err(CryptoError::BadEncoding);
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_val(bytes[i])?;
        let lo = hex_val(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn hex_val(c: u8) -> Result<u8> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(CryptoError::BadEncoding),
    }
}
