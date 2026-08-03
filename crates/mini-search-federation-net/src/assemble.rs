//! Turning a [`crate::SourcePullReport::trusted`] id set into a real,
//! owned `federate_query`-ready source.
//!
//! [`crate::pull_source`] proves a set of ids are F1/F2/F2b objects
//! authored by an expected provider; it does not know anything about
//! `mini_query`/`mini_ranker`'s own types. This module is the one place
//! that bridges the two: find the F2 [`IndexSegment`] and the F2b
//! [`mini_search_federation::CorpusBundle`] that declares the same
//! [`IndexSegmentId`], rebuild a fresh [`Corpus`]/[`DocumentContextTable`]
//! from the bundle's declared entries (mirroring exactly how a caller would
//! build them from scratch via `.insert()`), and hand back an
//! [`OwnedFederationSource`] whose [`OwnedFederationSource::as_source`]
//! borrows a real [`mini_search_federation::FederationSource`] a caller can
//! pass straight to `federate_query` alongside other sources -- local or
//! pulled from other peers.

use mini_lexical_index::IndexSegment;
use mini_objects::ObjectId;
use mini_query::DocumentContextTable;
use mini_ranker::Corpus;
use mini_search_federation::{read_corpus_bundle, read_index_segment, FederationSource};
use mini_store::{Backend, Store};
use mini_web_types::{IndexSegmentId, ProviderPseudonym};

use crate::error::{NetError, Result};

/// One provider's real, owned queryable state, rebuilt from pulled F2/F2b
/// objects. Holds the same three pieces of data
/// [`mini_search_federation::FederationSource`] borrows, so this type
/// outlives one `federate_query` call and can be reused across several.
#[derive(Debug)]
pub struct OwnedFederationSource {
    pub provider: ProviderPseudonym,
    pub index_segment: IndexSegmentId,
    pub segment: IndexSegment,
    pub corpus: Corpus,
    pub contexts: DocumentContextTable,
}

impl OwnedFederationSource {
    /// Borrow a [`FederationSource`] `federate_query` can take directly.
    pub fn as_source(&self) -> FederationSource<'_> {
        FederationSource {
            provider: self.provider.clone(),
            index: &self.segment,
            corpus: &self.corpus,
            contexts: &self.contexts,
            index_segment: self.index_segment.clone(),
        }
    }
}

/// Assemble one [`OwnedFederationSource`] from a trusted id set (typically
/// [`crate::SourcePullReport::trusted`] from one [`crate::pull_source`]
/// call). `store` must already hold the objects those ids name -- exactly
/// the state `pull_source` leaves behind.
///
/// Requires exactly one F2 index segment among `trusted_ids`
/// ([`NetError::NoIndexSegment`]/[`NetError::AmbiguousIndexSegment`]
/// otherwise) and at least one F2b corpus bundle declaring that segment's
/// own [`IndexSegmentId`] ([`NetError::NoMatchingCorpusBundle`] otherwise).
/// A peer that pulled multiple segments from one provider calls this once
/// per segment with a narrower id subset.
pub fn assemble_federation_source<B: Backend>(
    store: &Store<B>,
    trusted_ids: &[ObjectId],
    provider: ProviderPseudonym,
) -> Result<OwnedFederationSource> {
    let mut found_segment: Option<(IndexSegment, IndexSegmentId)> = None;
    let mut bundle_docs = Vec::new();
    let mut bundle_contexts = Vec::new();
    let mut matched_bundle = false;

    // First pass: find the one segment and remember its id.
    for id in trusted_ids {
        let obj = store.get(id)?;
        if let Ok(segment) = read_index_segment(&obj) {
            if found_segment.is_some() {
                return Err(NetError::AmbiguousIndexSegment);
            }
            let segment_id = segment.segment_id();
            found_segment = Some((segment, segment_id));
        }
    }
    let (segment, segment_id) = found_segment.ok_or(NetError::NoIndexSegment)?;

    // Second pass: collect every bundle that declares this exact segment.
    for id in trusted_ids {
        let obj = store.get(id)?;
        if let Ok(bundle) = read_corpus_bundle(&obj) {
            if bundle.index_segment == segment_id {
                matched_bundle = true;
                bundle_docs.extend(bundle.docs);
                bundle_contexts.extend(bundle.contexts);
            }
        }
    }
    if !matched_bundle {
        return Err(NetError::NoMatchingCorpusBundle);
    }

    let mut corpus = Corpus::new();
    for (id, meta) in bundle_docs {
        corpus.insert(&id, meta);
    }
    let mut contexts = DocumentContextTable::new();
    for (id, ctx) in bundle_contexts {
        contexts.insert(&id, ctx);
    }

    Ok(OwnedFederationSource {
        provider,
        index_segment: segment_id,
        segment,
        corpus,
        contexts,
    })
}
