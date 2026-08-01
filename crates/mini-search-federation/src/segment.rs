//! F2: signed, content-addressed index-segment exchange objects
//! (`docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
//! §29, "Publish and verify immutable index segments").
//!
//! [`mini_lexical_index::IndexSegment`] already has a canonical
//! `to_bytes`/`from_bytes` codec and its own content-addressed
//! `segment_id()` (BLAKE3 of its canonical bytes); this module does not
//! reimplement that. It wraps those already-canonical bytes in a signed
//! [`mini_objects::Object`] so a segment can be published, exchanged, and
//! authenticated the same way any other signed object in this workspace
//! is -- the "publish and verify" pairing this module provides is: publish
//! (sign + content-address the wrapper), and verify (decode rejects any
//! non-canonical segment bytes -- `IndexSegment::from_bytes`'s own
//! documented behavior -- plus the caller's own
//! [`mini_objects::Object::verify_signature`] against the publishing
//! peer's KEL, exactly as [`crate::observation::read_crawl_observation`]
//! leaves signature verification to the caller).

use did_mini::{Controller, Did};
use mini_lexical_index::IndexSegment;
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload, MAX_PAYLOAD_BYTES};
use mini_store::{Backend, Store};
use mini_web_types::IndexSegmentId;

use crate::error::{FederationError, Result};

/// The custom object type carrying a signed [`IndexSegment`].
pub const INDEX_SEGMENT_TYPE: &str = "mini/index-segment";

/// Publish `segment` as a signed, content-addressed object. Returns both
/// the wrapping object's id and the segment's own content-addressed
/// [`IndexSegmentId`] (`segment.segment_id()`) -- a caller wanting to
/// request "this exact segment" from a peer asks by the latter; a caller
/// wanting to fetch/dedup the wrapping exchange object asks by the former.
pub fn publish_index_segment<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    segment: &IndexSegment,
) -> Result<(ObjectId, IndexSegmentId)> {
    let bytes = segment.to_bytes();
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(FederationError::LimitExceeded);
    }
    let obj = ObjectBuilder::new(ObjectType::Custom(INDEX_SEGMENT_TYPE.to_string()))
        .payload(Payload::Public(bytes))
        .sign(human, device)?;
    let id = obj.id().clone();
    let segment_id = segment.segment_id();
    store.insert(&obj)?;
    Ok((id, segment_id))
}

/// Parse and canonical-form-verify a signed index-segment object back into
/// an [`IndexSegment`]. Rejects a non-canonical encoding the same way
/// `IndexSegment::from_bytes` always has; does not itself verify the
/// wrapping object's signature (see module docs).
pub fn read_index_segment(obj: &Object) -> Result<IndexSegment> {
    if obj.object_type != ObjectType::Custom(INDEX_SEGMENT_TYPE.to_string()) {
        return Err(FederationError::WrongObjectType);
    }
    let bytes = match &obj.payload {
        Payload::Public(b) => b,
        Payload::Encrypted(_) => return Err(FederationError::NotPublicPayload),
    };
    Ok(IndexSegment::from_bytes(bytes)?)
}
