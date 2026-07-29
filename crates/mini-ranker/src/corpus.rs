//! Per-document metadata the ranker needs but the lexical index does not
//! hold: the canonical URL, display strings, and the signals that are not
//! lexical (freshness, link count, duplicate identity, availability).
//!
//! The index answers "which documents contain this term"; the corpus
//! answers "what is this document" — kept separate so the index stays a
//! pure inverted index and the ranker composes the two.

use std::collections::HashMap;

use mini_crypto::Multihash;
use mini_web_types::{AvailabilityState, CanonicalUrl, UrlId};

/// Everything the ranker needs about one document beyond its indexed text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentMeta {
    pub url: CanonicalUrl,
    pub title: String,
    pub snippet: String,
    /// When this document was observed, in milliseconds since the Unix
    /// epoch. The freshness signal is computed against an explicit query
    /// time, never a wall clock read inside the ranker, so ranking is
    /// reproducible.
    pub observed_at_ms: u64,
    /// Count of inbound links, the basis for the (deliberately basic) link
    /// signal. This is a plain count with no notion of who paid for a link.
    pub inbound_links: u32,
    /// Content digest used for exact-duplicate detection. Two documents
    /// with the same digest are the same content at different URLs; the
    /// ranker keeps one and drops the rest.
    pub content_digest: Multihash,
    /// Availability. Only `Available` documents become displayable results;
    /// restricted/unavailable documents are excluded outright rather than
    /// scored down, so an availability decision is never silently converted
    /// into a relevance penalty (a D-0312 search invariant).
    pub availability: AvailabilityState,
}

/// A lookup from document identity to metadata. Keyed by the `UrlId`'s bytes
/// so it composes the same content-addressed identity the index uses.
#[derive(Debug, Default, Clone)]
pub struct Corpus {
    docs: HashMap<Vec<u8>, DocumentMeta>,
}

impl Corpus {
    pub fn new() -> Self {
        Corpus {
            docs: HashMap::new(),
        }
    }

    /// Insert or replace one document's metadata.
    pub fn insert(&mut self, id: &UrlId, meta: DocumentMeta) {
        self.docs.insert(id.0.to_bytes(), meta);
    }

    pub fn get(&self, id: &UrlId) -> Option<&DocumentMeta> {
        self.docs.get(&id.0.to_bytes())
    }

    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }
}
