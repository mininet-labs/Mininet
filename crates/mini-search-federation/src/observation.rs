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
pub fn publish_crawl_observation<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    observation: &CrawlObservation,
) -> Result<ObjectId> {
    let payload = encode_observation(observation);
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
    let observation = decode_observation(bytes)?;
    Ok(observation)
}

fn encode_observation(o: &CrawlObservation) -> Vec<u8> {
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
    w.into_bytes()
}

fn decode_observation(bytes: &[u8]) -> Result<CrawlObservation> {
    let mut r = Reader::new(bytes);
    let id = CrawlObservationId(
        Multihash::from_bytes(&r.bytes_limited(128)?).map_err(|_| FederationError::BadEncoding)?,
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
        Multihash::from_bytes(&r.bytes_limited(128)?).map_err(|_| FederationError::BadEncoding)?,
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
        // `Scheme` is `#[non_exhaustive]` in `mini-web-types`; a future
        // variant there needs a matching wire-format decision here, not a
        // silent fallback.
        _ => unreachable!("Scheme has only Http/Https today"),
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
        // `FetchStatus` is `#[non_exhaustive]`; see the `Scheme` note above.
        _ => unreachable!("FetchStatus has no variants beyond the six above today"),
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
            Multihash::from_bytes(&r.bytes_limited(128)?)
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
        // `WebMediaType` is `#[non_exhaustive]`; see the `Scheme` note above.
        _ => unreachable!("WebMediaType has no variants beyond the seven above today"),
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
