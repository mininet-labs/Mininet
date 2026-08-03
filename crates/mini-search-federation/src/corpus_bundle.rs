//! F2b: signed, content-addressed corpus/context bundle exchange objects.
//!
//! `mini_query::search`/`crate::federate_query` need each provider's
//! [`mini_ranker::Corpus`]/[`mini_query::DocumentContextTable`] alongside its
//! F2 [`mini_lexical_index::IndexSegment`] -- the index answers "which
//! documents contain this term," the corpus/context tables answer "what is
//! this document" (title, snippet, freshness, availability, language,
//! source observation). Until this module, neither table had a wire format:
//! a network-pulled F2 segment alone could never feed a real `federate_query`
//! call, only an in-process one built from data the same process already
//! held. This closes that gap the same way F1/F2 already close theirs --
//! wrap already-produced values in a signed, content-addressed
//! [`mini_objects::Object`], reusing the shared URL/media-type codec
//! (`crate::codec`) F1 already established rather than inventing a second
//! one.
//!
//! `Corpus`/`DocumentContextTable` expose no enumeration API (by design --
//! they only support point lookup by [`UrlId`]), so this module does not
//! introspect an existing table. A caller that built one already has the
//! `(UrlId, DocumentMeta)`/`(UrlId, DocumentContext)` pairs it inserted;
//! [`publish_corpus_bundle`] takes those pairs directly, and
//! [`read_corpus_bundle`] hands them back for the caller to re-insert into
//! its own fresh tables. This keeps this crate from depending on
//! `mini-ranker`/`mini-query` growing a new iteration API just for this.

use did_mini::{Controller, Did};
use mini_crypto::Multihash;
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload, MAX_PAYLOAD_BYTES};
use mini_query::DocumentContext;
use mini_ranker::DocumentMeta;
use mini_store::{Backend, Store};
use mini_web_types::{
    AvailabilityState, CrawlObservationId, IndexSegmentId, RestrictionReason, UnavailabilityReason,
    UrlId, WebMediaType,
};

use crate::codec::{
    decode_media_type, decode_url, encode_media_type, encode_url, media_type_wire_len,
    url_wire_len, Reader, Writer,
};
use crate::error::{FederationError, Result};

/// The custom object type carrying a signed corpus/context bundle.
pub const CORPUS_BUNDLE_TYPE: &str = "mini/corpus-bundle";

const MAX_MULTIHASH_BYTES: usize = 128;
const MAX_TITLE_BYTES: usize = 512;
const MAX_SNIPPET_BYTES: usize = 2048;
const MAX_JURISDICTION_BYTES: usize = 128;
/// Generous bound on a BCP-47-ish language tag.
const MAX_LANGUAGE_BYTES: usize = 64;
/// Pre-allocation ceiling on how many `(UrlId, _)` pairs one bundle may
/// declare, checked before `Vec::with_capacity` -- independent of, and
/// tighter than, the overall `MAX_PAYLOAD_BYTES` check below, the same way
/// `observation.rs`'s `MAX_REDIRECT_CHAIN` and `mini_lexical_index`'s
/// `MAX_DOCUMENTS` are pre-allocation ceilings rather than expected values.
/// Metadata entries are heavier than lexical postings, so this is set well
/// below `mini_lexical_index::MAX_DOCUMENTS` (1 << 24): even at a minimum
/// realistic per-entry size, `MAX_PAYLOAD_BYTES` (8 MiB) is reached long
/// before this count is.
const MAX_BUNDLE_ENTRIES: usize = 1 << 20;

/// One provider's declared corpus/context entries for one [`IndexSegmentId`].
/// [`publish_corpus_bundle`]/[`read_corpus_bundle`] work on these pairs
/// directly rather than an assembled `Corpus`/`DocumentContextTable`, since
/// neither type exposes enumeration (see module docs).
#[derive(Debug)]
pub struct CorpusBundle {
    pub index_segment: IndexSegmentId,
    pub docs: Vec<(UrlId, DocumentMeta)>,
    pub contexts: Vec<(UrlId, DocumentContext)>,
}

/// Publish a corpus/context bundle for `index_segment` as a signed,
/// content-addressed object. The same field bounds the reader enforces are
/// checked before encoding, so this function can never publish an object
/// its own reader must reject solely for an oversized typed value.
pub fn publish_corpus_bundle<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    index_segment: &IndexSegmentId,
    docs: &[(UrlId, DocumentMeta)],
    contexts: &[(UrlId, DocumentContext)],
) -> Result<ObjectId> {
    let payload = encode_bundle(index_segment, docs, contexts)?;
    let obj = ObjectBuilder::new(ObjectType::Custom(CORPUS_BUNDLE_TYPE.to_string()))
        .payload(Payload::Public(payload))
        .sign(human, device)?;
    let id = obj.id().clone();
    store.insert(&obj)?;
    Ok(id)
}

/// Parse a signed corpus bundle object back into its declared entries. Does
/// not itself verify the object's signature -- callers verify against the
/// publisher's KEL first, the same two-step pattern every signed-object
/// reader in this workspace already follows.
pub fn read_corpus_bundle(obj: &Object) -> Result<CorpusBundle> {
    if obj.object_type != ObjectType::Custom(CORPUS_BUNDLE_TYPE.to_string()) {
        return Err(FederationError::WrongObjectType);
    }
    let bytes = match &obj.payload {
        Payload::Public(b) => b,
        Payload::Encrypted(_) => return Err(FederationError::NotPublicPayload),
    };
    decode_bundle(bytes)
}

fn checked_add(total: &mut usize, value: usize) -> Result<()> {
    *total = (*total)
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

fn multihash_field_len(m: &Multihash) -> Result<usize> {
    bytes_field_len(validate_multihash_len(m.to_bytes().len())?)
}

fn availability_wire_len(a: &AvailabilityState) -> Result<usize> {
    Ok(match a {
        AvailabilityState::Available => 1,
        AvailabilityState::Unavailable(reason) => {
            1 + match reason {
                UnavailabilityReason::NotFetched
                | UnavailabilityReason::FetchFailed
                | UnavailabilityReason::Gone
                | UnavailabilityReason::UnsupportedContent => 1,
                _ => return Err(FederationError::BadEncoding),
            }
        }
        AvailabilityState::Restricted(reason) => {
            1 + match reason {
                RestrictionReason::RobotsExcluded
                | RestrictionReason::Malware
                | RestrictionReason::Spam
                | RestrictionReason::UserFilter
                | RestrictionReason::SafetyWarning => 1,
                RestrictionReason::LegalRestriction { jurisdiction } => {
                    1 + str_field_len(jurisdiction, MAX_JURISDICTION_BYTES)?
                }
                _ => return Err(FederationError::BadEncoding),
            }
        }
        _ => return Err(FederationError::BadEncoding),
    })
}

fn document_meta_wire_len(meta: &DocumentMeta) -> Result<usize> {
    let mut total = 0usize;
    checked_add(&mut total, url_wire_len(&meta.url)?)?;
    checked_add(&mut total, str_field_len(&meta.title, MAX_TITLE_BYTES)?)?;
    checked_add(&mut total, str_field_len(&meta.snippet, MAX_SNIPPET_BYTES)?)?;
    checked_add(&mut total, 8)?; // observed_at_ms
    checked_add(&mut total, 4)?; // inbound_links
    checked_add(&mut total, multihash_field_len(&meta.content_digest)?)?;
    checked_add(&mut total, availability_wire_len(&meta.availability)?)?;
    Ok(total)
}

fn document_context_wire_len(ctx: &DocumentContext) -> Result<usize> {
    let mut total = 1usize; // language option tag
    if let Some(lang) = &ctx.language {
        checked_add(&mut total, str_field_len(lang, MAX_LANGUAGE_BYTES)?)?;
    }
    checked_add(&mut total, 1)?; // media_type option tag
    if let Some(mt) = &ctx.media_type {
        checked_add(&mut total, media_type_wire_len(mt)?)?;
    }
    checked_add(&mut total, multihash_field_len(&ctx.source_observation.0)?)?;
    Ok(total)
}

fn bundle_wire_len(
    index_segment: &IndexSegmentId,
    docs: &[(UrlId, DocumentMeta)],
    contexts: &[(UrlId, DocumentContext)],
) -> Result<usize> {
    if docs.len() > MAX_BUNDLE_ENTRIES || contexts.len() > MAX_BUNDLE_ENTRIES {
        return Err(FederationError::LimitExceeded);
    }
    let mut total = 0usize;
    checked_add(&mut total, multihash_field_len(&index_segment.0)?)?;
    checked_add(&mut total, 4)?; // docs count
    for (id, meta) in docs {
        checked_add(&mut total, multihash_field_len(&id.0)?)?;
        checked_add(&mut total, document_meta_wire_len(meta)?)?;
    }
    checked_add(&mut total, 4)?; // contexts count
    for (id, ctx) in contexts {
        checked_add(&mut total, multihash_field_len(&id.0)?)?;
        checked_add(&mut total, document_context_wire_len(ctx)?)?;
    }
    if total > MAX_PAYLOAD_BYTES {
        return Err(FederationError::LimitExceeded);
    }
    Ok(total)
}

fn encode_availability(w: &mut Writer, a: &AvailabilityState) {
    match a {
        AvailabilityState::Available => w.u8(0),
        AvailabilityState::Unavailable(reason) => {
            w.u8(1);
            w.u8(match reason {
                UnavailabilityReason::NotFetched => 0,
                UnavailabilityReason::FetchFailed => 1,
                UnavailabilityReason::Gone => 2,
                UnavailabilityReason::UnsupportedContent => 3,
                // `availability_wire_len` rejects any future unsupported
                // variant before this encoder is reached.
                _ => unreachable!("validated unavailability reason"),
            });
        }
        AvailabilityState::Restricted(reason) => {
            w.u8(2);
            match reason {
                RestrictionReason::RobotsExcluded => w.u8(0),
                RestrictionReason::LegalRestriction { jurisdiction } => {
                    w.u8(1);
                    w.str(jurisdiction);
                }
                RestrictionReason::Malware => w.u8(2),
                RestrictionReason::Spam => w.u8(3),
                RestrictionReason::UserFilter => w.u8(4),
                RestrictionReason::SafetyWarning => w.u8(5),
                // `availability_wire_len` rejects any future unsupported
                // variant before this encoder is reached.
                _ => unreachable!("validated restriction reason"),
            }
        }
        // `availability_wire_len` rejects any future unsupported variant
        // before this encoder is reached.
        _ => unreachable!("validated availability state"),
    }
}

fn decode_availability(r: &mut Reader) -> Result<AvailabilityState> {
    Ok(match r.u8()? {
        0 => AvailabilityState::Available,
        1 => AvailabilityState::Unavailable(match r.u8()? {
            0 => UnavailabilityReason::NotFetched,
            1 => UnavailabilityReason::FetchFailed,
            2 => UnavailabilityReason::Gone,
            3 => UnavailabilityReason::UnsupportedContent,
            _ => return Err(FederationError::BadEncoding),
        }),
        2 => AvailabilityState::Restricted(match r.u8()? {
            0 => RestrictionReason::RobotsExcluded,
            1 => RestrictionReason::LegalRestriction {
                jurisdiction: r.str_limited(MAX_JURISDICTION_BYTES)?,
            },
            2 => RestrictionReason::Malware,
            3 => RestrictionReason::Spam,
            4 => RestrictionReason::UserFilter,
            5 => RestrictionReason::SafetyWarning,
            _ => return Err(FederationError::BadEncoding),
        }),
        _ => return Err(FederationError::BadEncoding),
    })
}

fn encode_document_meta(w: &mut Writer, meta: &DocumentMeta) {
    encode_url(w, &meta.url);
    w.str(&meta.title);
    w.str(&meta.snippet);
    w.u64(meta.observed_at_ms);
    w.u32(meta.inbound_links);
    w.bytes(&meta.content_digest.to_bytes());
    encode_availability(w, &meta.availability);
}

fn decode_document_meta(r: &mut Reader) -> Result<DocumentMeta> {
    let url = decode_url(r)?;
    let title = r.str_limited(MAX_TITLE_BYTES)?;
    let snippet = r.str_limited(MAX_SNIPPET_BYTES)?;
    let observed_at_ms = r.u64()?;
    let inbound_links = r.u32()?;
    let content_digest = Multihash::from_bytes(&r.bytes_limited(MAX_MULTIHASH_BYTES)?)
        .map_err(|_| FederationError::BadEncoding)?;
    let availability = decode_availability(r)?;
    Ok(DocumentMeta {
        url,
        title,
        snippet,
        observed_at_ms,
        inbound_links,
        content_digest,
        availability,
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

fn encode_document_context(w: &mut Writer, ctx: &DocumentContext) {
    match &ctx.language {
        Some(lang) => {
            w.u8(1);
            w.str(lang);
        }
        None => w.u8(0),
    }
    encode_opt_media_type(w, ctx.media_type.as_ref());
    w.bytes(&ctx.source_observation.0.to_bytes());
}

fn decode_document_context(r: &mut Reader) -> Result<DocumentContext> {
    let language = match r.u8()? {
        0 => None,
        1 => Some(r.str_limited(MAX_LANGUAGE_BYTES)?),
        _ => return Err(FederationError::BadEncoding),
    };
    let media_type = decode_opt_media_type(r)?;
    let source_observation = CrawlObservationId(
        Multihash::from_bytes(&r.bytes_limited(MAX_MULTIHASH_BYTES)?)
            .map_err(|_| FederationError::BadEncoding)?,
    );
    Ok(DocumentContext {
        language,
        media_type,
        source_observation,
    })
}

fn encode_bundle(
    index_segment: &IndexSegmentId,
    docs: &[(UrlId, DocumentMeta)],
    contexts: &[(UrlId, DocumentContext)],
) -> Result<Vec<u8>> {
    let expected_len = bundle_wire_len(index_segment, docs, contexts)?;
    let mut w = Writer::new();
    w.bytes(&index_segment.0.to_bytes());
    w.u32(docs.len() as u32);
    for (id, meta) in docs {
        w.bytes(&id.0.to_bytes());
        encode_document_meta(&mut w, meta);
    }
    w.u32(contexts.len() as u32);
    for (id, ctx) in contexts {
        w.bytes(&id.0.to_bytes());
        encode_document_context(&mut w, ctx);
    }
    let bytes = w.into_bytes();
    debug_assert_eq!(bytes.len(), expected_len);
    Ok(bytes)
}

fn decode_bundle(bytes: &[u8]) -> Result<CorpusBundle> {
    let mut r = Reader::new(bytes);
    let index_segment = IndexSegmentId(
        Multihash::from_bytes(&r.bytes_limited(MAX_MULTIHASH_BYTES)?)
            .map_err(|_| FederationError::BadEncoding)?,
    );
    let n_docs = r.u32()? as usize;
    if n_docs > MAX_BUNDLE_ENTRIES {
        return Err(FederationError::LimitExceeded);
    }
    let mut docs = Vec::with_capacity(n_docs);
    for _ in 0..n_docs {
        let id = UrlId(
            Multihash::from_bytes(&r.bytes_limited(MAX_MULTIHASH_BYTES)?)
                .map_err(|_| FederationError::BadEncoding)?,
        );
        docs.push((id, decode_document_meta(&mut r)?));
    }
    let n_contexts = r.u32()? as usize;
    if n_contexts > MAX_BUNDLE_ENTRIES {
        return Err(FederationError::LimitExceeded);
    }
    let mut contexts = Vec::with_capacity(n_contexts);
    for _ in 0..n_contexts {
        let id = UrlId(
            Multihash::from_bytes(&r.bytes_limited(MAX_MULTIHASH_BYTES)?)
                .map_err(|_| FederationError::BadEncoding)?,
        );
        contexts.push((id, decode_document_context(&mut r)?));
    }
    if !r.finished() {
        return Err(FederationError::BadEncoding);
    }
    Ok(CorpusBundle {
        index_segment,
        docs,
        contexts,
    })
}
