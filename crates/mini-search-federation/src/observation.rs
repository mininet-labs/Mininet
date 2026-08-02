//! F1: signed, content-addressed crawl-observation exchange objects
//! (`docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
//! §29, "Nodes exchange content-addressed crawl observations").
//!
//! This module provides the wire format and the signed-object publish/read
//! pair only -- no network transport, no peer discovery, no scheduling.
//! `publish_crawl_observation` wraps an already-produced
//! [`mini_web_types::CrawlObservation`] in a signed [`mini_objects::Object`]
//! exactly the way [`crate::segment::publish_index_segment`] and
//! `mini-media`'s `publish_media` already wrap their own payloads --
//! reusing the identical signed-content-address pattern, not inventing a
//! second one. Authenticity comes from the wrapping object's real
//! `did-mini` signature (verified by the caller via
//! [`mini_objects::Object::verify_signature`], exactly as any other signed
//! object in this workspace); this crate does not choose or derive a
//! privacy-preserving signing identity for the caller -- a caller wanting
//! a scoped pseudonym rather than a root identity already has SPEC-01 §10's
//! `Controller::incept_pairwise_pseudonym` for that, unchanged by this
//! crate.

use did_mini::{Controller, Did};
use mini_crypto::Multihash;
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};
use mini_web_types::{
    CanonicalUrl, CrawlObservation, CrawlObservationId, FetchStatus, HttpStatus, NormalizedHost,
    ProviderPseudonym, Scheme, WebMediaType,
};

use crate::codec::{Reader, Writer};
use crate::error::{FederationError, Result};

/// The custom object type carrying a signed [`CrawlObservation`].
pub const CRAWL_OBSERVATION_TYPE: &str = "mini/crawl-observation";

const MAX_MULTIHASH_BYTES: usize = 128;
const MAX_HOST_BYTES: usize = 253;
const MAX_PATH_BYTES: usize = 4096;
const MAX_QUERY_BYTES: usize = 4096;
const MAX_MEDIA_TYPE_OTHER_BYTES: usize = 64;
/// Bound on `redirect_chain` length -- generous relative to any sane
/// redirect limit a real crawler would enforce, but finite so a decoder
/// cannot be made to allocate unboundedly.
const MAX_REDIRECT_CHAIN: usize = 64;

/// Publish `observation` as a signed, content-addressed object. Returns the
/// wrapping object's id (the exchange/dedup key a peer would request by).
///
/// The same field bounds the reader enforces are checked before encoding, so
/// this function can never publish an object that this crate's own reader must
/// reject merely because a caller constructed an oversized typed value in
/// memory.
pub fn publish_crawl_observation<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    observation: &CrawlObservation,
) -> Result<ObjectId> {
    let payload = encode_observation(observation)?;
    let obj = ObjectBuilder::new(ObjectType::Custom(CRAWL_OBSERVATION_TYPE.to_string()))
        .timestamp_ms(observation.observed_at_ms)
        .payload(Payload::Public(payload))
        .sign(human, device)?;
    let id = obj.id().clone();
    store.insert(&obj)?;
    Ok(id)
}

/// Parse a signed crawl-observation object back into a [`CrawlObservation`].
/// Does not itself verify the object's signature -- callers that need
/// authenticity, not just well-formedness, call
/// [`mini_objects::Object::verify_signature`] (or `verify_provenance`)
/// against the peer's KEL first, the same two-step pattern every other
/// signed-object reader in this workspace already follows.
pub fn read_crawl_observation(obj: &Object) -> Result<CrawlObservation> {
    if obj.object_type != ObjectType::Custom(CRAWL_OBSERVATION_TYPE.to_string()) {
        return Err(FederationError::WrongObjectType);
    }
    let bytes = match &obj.payload {
        Payload::Public(b) => b,
        Payload::Encrypted(_) => return Err(FederationError::NotPublicPayload),
    };
    decode_observation(bytes)
}

/// Exact canonical payload length after applying every F1 field bound.
///
/// F7 uses this as a deterministic, allocation-independent storage-budget
/// proxy. It intentionally measures canonical wire bytes rather than Rust heap
/// implementation details, which are platform-dependent and unsuitable for a
/// protocol-facing limit.
pub(crate) fn observation_wire_len(o: &CrawlObservation) -> Result<usize> {
    let mut total = 0usize;
    checked_add(&mut total, bytes_field_len(o.id.0.to_bytes().len())?)?;
    checked_add(&mut total, url_wire_len(&o.requested_url)?)?;
    checked_add(&mut total, url_wire_len(&o.final_url)?)?;
    checked_add(&mut total, 8)?; // observed_at_ms
    checked_add(&mut total, status_wire_len(&o.status)?)?;

    checked_add(&mut total, 1)?; // content_digest option tag
    if let Some(digest) = &o.content_digest {
        checked_add(
            &mut total,
            bytes_field_len(validate_multihash_len(digest.to_bytes().len())?)?,
        )?;
    }

    checked_add(&mut total, 1)?; // media_type option tag
    if let Some(media_type) = &o.media_type {
        checked_add(&mut total, media_type_wire_len(media_type)?)?;
    }

    checked_add(&mut total, 1)?; // byte_length option tag
    if o.byte_length.is_some() {
        checked_add(&mut total, 8)?;
    }

    if o.redirect_chain.len() > MAX_REDIRECT_CHAIN {
        return Err(FederationError::LimitExceeded);
    }
    checked_add(&mut total, 4)?; // redirect count
    for redirect in &o.redirect_chain {
        checked_add(&mut total, url_wire_len(redirect)?)?;
    }

    checked_add(
        &mut total,
        bytes_field_len(validate_multihash_len(o.crawler.0.to_bytes().len())?)?,
    )?;
    Ok(total)
}

fn checked_add(total: &mut usize, value: usize) -> Result<()> {
    *total = total
        .checked_add(value)
        .ok_or(FederationError::LimitExceeded)?;
    Ok(())
}

fn validate_multihash_len(len: usize) -> Result<usize> {
    if len > MAX_MULTIHASH_BYTES {
        Err(FederationError::LimitExceeded)
    } else {
        Ok(len)
    }
}

fn bytes_field_len(len: usize) -> Result<usize> {
    if len > u32::MAX as usize {
        return Err(FederationError::LimitExceeded);
    }
    4usize
        .checked_add(len)
        .ok_or(FederationError::LimitExceeded)
}

fn str_field_len(value: &str, max: usize) -> Result<usize> {
    if value.len() > max {
        return Err(FederationError::LimitExceeded);
    }
    bytes_field_len(value.len())
}

fn url_wire_len(url: &CanonicalUrl) -> Result<usize> {
    match url.scheme {
        Scheme::Http | Scheme::Https => {}
        _ => return Err(FederationError::BadEncoding),
    }

    let mut total = 1usize; // scheme
    checked_add(&mut total, str_field_len(url.host.as_str(), MAX_HOST_BYTES)?)?;
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

fn status_wire_len(status: &FetchStatus) -> Result<usize> {
    match status {
        FetchStatus::Success(_) => Ok(3),
        FetchStatus::RedirectLimitExceeded
        | FetchStatus::Timeout
        | FetchStatus::NetworkError
        | FetchStatus::RobotsExcluded
        | FetchStatus::UnsupportedScheme
        | FetchStatus::AddressBlocked
        | FetchStatus::ResponseTooLarge
        | FetchStatus::UnsupportedMediaType
        | FetchStatus::InvalidRedirect => Ok(1),
        _ => Err(FederationError::BadEncoding),
    }
}

fn media_type_wire_len(media_type: &WebMediaType) -> Result<usize> {
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

fn encode_observation(o: &CrawlObservation) -> Result<Vec<u8>> {
    let expected_len = observation_wire_len(o)?;
    let mut w = Writer::new();
    w.bytes(&o.id.0.to_bytes());
    encode_url(&mut w, &o.requested_url);
    encode_url(&mut w, &o.final_url);
    w.u64(o.observed_at_ms);
    encode_status(&mut w, &o.status);
    encode_opt_multihash(&mut w, o.content_digest.as_ref());
    encode_opt_media_type(&mut w, o.media_type.as_ref());
    match o.byte_length {
        Some(n) => {
            w.u8(1);
            w.u64(n);
        }
        None => w.u8(0),
    }
    w.u32(o.redirect_chain.len() as u32);
    for u in &o.redirect_chain {
        encode_url(&mut w, u);
    }
    w.bytes(&o.crawler.0.to_bytes());
    let bytes = w.into_bytes();
    debug_assert_eq!(bytes.len(), expected_len);
    Ok(bytes)
}

fn decode_observation(bytes: &[u8]) -> Result<CrawlObservation> {
    let mut r = Reader::new(bytes);
    let id = CrawlObservationId(
        Multihash::from_bytes(&r.bytes_limited(MAX_MULTIHASH_BYTES)?)
            .map_err(|_| FederationError::BadEncoding)?,
    );
    let requested_url = decode_url(&mut r)?;
    let final_url = decode_url(&mut r)?;
    let observed_at_ms = r.u64()?;
    let status = decode_status(&mut r)?;
    let content_digest = decode_opt_multihash(&mut r)?;
    let media_type = decode_opt_media_type(&mut r)?;
    let byte_length = match r.u8()? {
        0 => None,
        1 => Some(r.u64()?),
        _ => return Err(FederationError::BadEncoding),
    };
    let n_redirects = r.u32()? as usize;
    if n_redirects > MAX_REDIRECT_CHAIN {
        return Err(FederationError::LimitExceeded);
    }
    let mut redirect_chain = Vec::with_capacity(n_redirects);
    for _ in 0..n_redirects {
        redirect_chain.push(decode_url(&mut r)?);
    }
    let crawler = ProviderPseudonym(
        Multihash::from_bytes(&r.bytes_limited(MAX_MULTIHASH_BYTES)?)
            .map_err(|_| FederationError::BadEncoding)?,
    );
    if !r.finished() {
        return Err(FederationError::BadEncoding);
    }
    Ok(CrawlObservation {
        id,
        requested_url,
        final_url,
        observed_at_ms,
        status,
        content_digest,
        media_type,
        byte_length,
        redirect_chain,
        crawler,
    })
}

fn encode_url(w: &mut Writer, u: &CanonicalUrl) {
    w.u8(match u.scheme {
        Scheme::Http => 0,
        Scheme::Https => 1,
        // `observation_wire_len` rejects any future unsupported variant before
        // this encoder is reached.
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

fn decode_url(r: &mut Reader) -> Result<CanonicalUrl> {
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

fn encode_status(w: &mut Writer, s: &FetchStatus) {
    match s {
        FetchStatus::Success(code) => {
            w.u8(0);
            w.u16(code.code());
        }
        FetchStatus::RedirectLimitExceeded => w.u8(1),
        FetchStatus::Timeout => w.u8(2),
        FetchStatus::NetworkError => w.u8(3),
        FetchStatus::RobotsExcluded => w.u8(4),
        FetchStatus::UnsupportedScheme => w.u8(5),
        FetchStatus::AddressBlocked => w.u8(6),
        FetchStatus::ResponseTooLarge => w.u8(7),
        FetchStatus::UnsupportedMediaType => w.u8(8),
        FetchStatus::InvalidRedirect => w.u8(9),
        // `observation_wire_len` rejects any future unsupported variant before
        // this encoder is reached.
        _ => unreachable!("validated fetch status"),
    }
}

fn decode_status(r: &mut Reader) -> Result<FetchStatus> {
    Ok(match r.u8()? {
        0 => {
            let code = r.u16()?;
            FetchStatus::Success(HttpStatus::new(code).map_err(|_| FederationError::BadEncoding)?)
        }
        1 => FetchStatus::RedirectLimitExceeded,
        2 => FetchStatus::Timeout,
        3 => FetchStatus::NetworkError,
        4 => FetchStatus::RobotsExcluded,
        5 => FetchStatus::UnsupportedScheme,
        6 => FetchStatus::AddressBlocked,
        7 => FetchStatus::ResponseTooLarge,
        8 => FetchStatus::UnsupportedMediaType,
        9 => FetchStatus::InvalidRedirect,
        _ => return Err(FederationError::BadEncoding),
    })
}

fn encode_opt_multihash(w: &mut Writer, m: Option<&Multihash>) {
    match m {
        Some(m) => {
            w.u8(1);
            w.bytes(&m.to_bytes());
        }
        None => w.u8(0),
    }
}

fn decode_opt_multihash(r: &mut Reader) -> Result<Option<Multihash>> {
    Ok(match r.u8()? {
        0 => None,
        1 => Some(
            Multihash::from_bytes(&r.bytes_limited(MAX_MULTIHASH_BYTES)?)
                .map_err(|_| FederationError::BadEncoding)?,
        ),
        _ => return Err(FederationError::BadEncoding),
    })
}

fn encode_media_type(w: &mut Writer, t: &WebMediaType) {
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
        // `observation_wire_len` rejects any future unsupported variant before
        // this encoder is reached.
        _ => unreachable!("validated web media type"),
    }
}

fn decode_media_type(r: &mut Reader) -> Result<WebMediaType> {
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

fn encode_opt_media_type(w: &mut Writer, t: Option<&WebMediaType>) {
    match t {
        Some(t) => {
            w.u8(1);
            encode_media_type(w, t);
        }
        None => w.u8(0),
    }
}

fn decode_opt_media_type(r: &mut Reader) -> Result<Option<WebMediaType>> {
    Ok(match r.u8()? {
        0 => None,
        1 => Some(decode_media_type(r)?),
        _ => return Err(FederationError::BadEncoding),
    })
}
