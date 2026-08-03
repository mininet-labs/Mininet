//! Deterministic, length-prefixed binary codec, the same discipline
//! `mini-lexical-index`/`mini-intake-types`/`mini-extract-protocol` already
//! use: big-endian integers, u32-length-prefixed byte strings, hard caps
//! applied before allocation.
//!
//! [`encode_url`]/[`decode_url`] and [`encode_media_type`]/
//! [`decode_media_type`] are shared between F1 (`observation.rs`) and F2b
//! (`corpus_bundle.rs`) so both modules apply the exact same field bounds to
//! a [`mini_web_types::CanonicalUrl`]/[`mini_web_types::WebMediaType`]
//! rather than maintaining two copies that could silently drift apart.

use mini_web_types::{CanonicalUrl, NormalizedHost, Scheme, WebMediaType};

use crate::error::{FederationError, Result};

/// Bound on a `CanonicalUrl`'s host field.
pub(crate) const MAX_HOST_BYTES: usize = 253;
/// Bound on a `CanonicalUrl`'s path field.
pub(crate) const MAX_PATH_BYTES: usize = 4096;
/// Bound on a `CanonicalUrl`'s query field.
pub(crate) const MAX_QUERY_BYTES: usize = 4096;
/// Bound on `WebMediaType::Other`'s string payload.
pub(crate) const MAX_MEDIA_TYPE_OTHER_BYTES: usize = 64;

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
            .ok_or(FederationError::BadEncoding)?;
        if end > self.data.len() {
            return Err(FederationError::BadEncoding);
        }
        let s = &self.data[self.pos..end];
        self.pos = end;
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

    pub(crate) fn bytes_limited(&mut self, max: usize) -> Result<Vec<u8>> {
        let len = self.u32()? as usize;
        if len > max {
            return Err(FederationError::LimitExceeded);
        }
        Ok(self.take(len)?.to_vec())
    }

    pub(crate) fn str_limited(&mut self, max: usize) -> Result<String> {
        let bytes = self.bytes_limited(max)?;
        String::from_utf8(bytes).map_err(|_| FederationError::BadEncoding)
    }

    pub(crate) fn finished(&self) -> bool {
        self.pos == self.data.len()
    }
}

fn checked_add(total: &mut usize, value: usize) -> Result<()> {
    *total = (*total)
        .checked_add(value)
        .ok_or(FederationError::LimitExceeded)?;
    Ok(())
}

fn str_field_len(value: &str, max: usize) -> Result<usize> {
    if value.len() > max {
        return Err(FederationError::LimitExceeded);
    }
    4usize
        .checked_add(value.len())
        .ok_or(FederationError::LimitExceeded)
}

/// Exact canonical wire length of a [`CanonicalUrl`] under the shared field
/// bounds, without allocating the encoded bytes first.
pub(crate) fn url_wire_len(url: &CanonicalUrl) -> Result<usize> {
    match url.scheme {
        Scheme::Http | Scheme::Https => {}
        _ => return Err(FederationError::BadEncoding),
    }

    let mut total = 1usize; // scheme
    checked_add(
        &mut total,
        str_field_len(url.host.as_str(), MAX_HOST_BYTES)?,
    )?;
    checked_add(&mut total, 1)?; // port option tag
    if url.port.is_some() {
        checked_add(&mut total, 2)?;
    }
    checked_add(&mut total, str_field_len(&url.path, MAX_PATH_BYTES)?)?;
    checked_add(&mut total, 1)?; // query option tag
    if let Some(query) = &url.query {
        checked_add(&mut total, str_field_len(query, MAX_QUERY_BYTES)?)?;
    }
    Ok(total)
}

pub(crate) fn encode_url(w: &mut Writer, u: &CanonicalUrl) {
    w.u8(match u.scheme {
        Scheme::Http => 0,
        Scheme::Https => 1,
        // `url_wire_len` rejects any future unsupported variant before this
        // encoder is reached.
        _ => unreachable!("validated URL scheme"),
    });
    w.str(u.host.as_str());
    match u.port {
        Some(p) => {
            w.u8(1);
            w.u16(p);
        }
        None => w.u8(0),
    }
    w.str(&u.path);
    match &u.query {
        Some(q) => {
            w.u8(1);
            w.str(q);
        }
        None => w.u8(0),
    }
}

pub(crate) fn decode_url(r: &mut Reader) -> Result<CanonicalUrl> {
    let scheme = match r.u8()? {
        0 => Scheme::Http,
        1 => Scheme::Https,
        _ => return Err(FederationError::BadEncoding),
    };
    let host = NormalizedHost::new(r.str_limited(MAX_HOST_BYTES)?)
        .map_err(|_| FederationError::BadEncoding)?;
    let port = match r.u8()? {
        0 => None,
        1 => Some(r.u16()?),
        _ => return Err(FederationError::BadEncoding),
    };
    let path = r.str_limited(MAX_PATH_BYTES)?;
    let query = match r.u8()? {
        0 => None,
        1 => Some(r.str_limited(MAX_QUERY_BYTES)?),
        _ => return Err(FederationError::BadEncoding),
    };
    CanonicalUrl::new(scheme, host, port, path, query).map_err(|_| FederationError::BadEncoding)
}

/// Exact canonical wire length of a [`WebMediaType`] under the shared field
/// bound.
pub(crate) fn media_type_wire_len(media_type: &WebMediaType) -> Result<usize> {
    match media_type {
        WebMediaType::Html
        | WebMediaType::TextPlain
        | WebMediaType::Markdown
        | WebMediaType::Json
        | WebMediaType::Pdf
        | WebMediaType::Image => Ok(1),
        WebMediaType::Other(value) => 1usize
            .checked_add(str_field_len(value, MAX_MEDIA_TYPE_OTHER_BYTES)?)
            .ok_or(FederationError::LimitExceeded),
        _ => Err(FederationError::BadEncoding),
    }
}

pub(crate) fn encode_media_type(w: &mut Writer, t: &WebMediaType) {
    match t {
        WebMediaType::Html => w.u8(0),
        WebMediaType::TextPlain => w.u8(1),
        WebMediaType::Markdown => w.u8(2),
        WebMediaType::Json => w.u8(3),
        WebMediaType::Pdf => w.u8(4),
        WebMediaType::Image => w.u8(5),
        WebMediaType::Other(s) => {
            w.u8(6);
            w.str(s);
        }
        // `media_type_wire_len` rejects any future unsupported variant
        // before this encoder is reached.
        _ => unreachable!("validated web media type"),
    }
}

pub(crate) fn decode_media_type(r: &mut Reader) -> Result<WebMediaType> {
    Ok(match r.u8()? {
        0 => WebMediaType::Html,
        1 => WebMediaType::TextPlain,
        2 => WebMediaType::Markdown,
        3 => WebMediaType::Json,
        4 => WebMediaType::Pdf,
        5 => WebMediaType::Image,
        6 => WebMediaType::Other(r.str_limited(MAX_MEDIA_TYPE_OTHER_BYTES)?),
        _ => return Err(FederationError::BadEncoding),
    })
}
