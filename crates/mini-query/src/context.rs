//! Per-document context this crate needs but neither the lexical index nor
//! `mini_ranker::Corpus` carries: language, media type (for the `lang:`/
//! `type:` filters), and which crawl observation the document came from
//! (for Track E8 provenance). Kept as its own table, the same "index answers
//! what text exists, corpus answers what a document is" separation
//! `mini-ranker` already uses -- this is a third, narrower table for facts
//! only a query/provenance layer needs.

use std::collections::HashMap;

use mini_web_types::{CrawlObservationId, UrlId, WebMediaType};

/// Facts about one document beyond its lexical content and ranking metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentContext {
    /// BCP-47-ish language tag as observed at extraction time, if known.
    pub language: Option<String>,
    /// Media type as observed at extraction time, if known.
    pub media_type: Option<WebMediaType>,
    /// Which crawl observation produced this document, for Track E8's
    /// "source observation" provenance field.
    pub source_observation: CrawlObservationId,
}

/// A lookup from document identity to [`DocumentContext`]. Keyed by the
/// `UrlId`'s bytes, mirroring `mini_ranker::Corpus`'s own keying exactly, so
/// the two tables always agree on what identifies "a document."
#[derive(Debug, Default, Clone)]
pub struct DocumentContextTable {
    contexts: HashMap<Vec<u8>, DocumentContext>,
}

impl DocumentContextTable {
    pub fn new() -> Self {
        DocumentContextTable {
            contexts: HashMap::new(),
        }
    }

    /// Insert or replace one document's context.
    pub fn insert(&mut self, id: &UrlId, ctx: DocumentContext) {
        self.contexts.insert(id.0.to_bytes(), ctx);
    }

    pub fn get(&self, id: &UrlId) -> Option<&DocumentContext> {
        self.contexts.get(&id.0.to_bytes())
    }

    pub fn len(&self) -> usize {
        self.contexts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.contexts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_crypto::{HashAlgorithm, Multihash};

    fn url_id(seed: &[u8]) -> UrlId {
        UrlId(Multihash::of(HashAlgorithm::Blake3, seed))
    }

    fn obs_id(seed: &[u8]) -> CrawlObservationId {
        CrawlObservationId(Multihash::of(HashAlgorithm::Blake3, seed))
    }

    #[test]
    fn insert_and_get_round_trip() {
        let mut table = DocumentContextTable::new();
        let id = url_id(b"doc-1");
        let ctx = DocumentContext {
            language: Some("en".to_string()),
            media_type: None,
            source_observation: obs_id(b"obs-1"),
        };
        table.insert(&id, ctx.clone());
        assert_eq!(table.get(&id), Some(&ctx));
        assert_eq!(table.len(), 1);
        assert!(!table.is_empty());
    }

    #[test]
    fn unknown_id_returns_none() {
        let table = DocumentContextTable::new();
        assert_eq!(table.get(&url_id(b"missing")), None);
        assert!(table.is_empty());
    }
}
