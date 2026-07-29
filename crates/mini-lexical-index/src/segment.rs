//! The immutable index segment: an inverted index over document fields,
//! with phrase positions, a canonical byte encoding, and a content-
//! addressed identity.
//!
//! A segment is built once from a set of documents ([`IndexBuilder`]) and
//! is thereafter immutable. Its [`IndexSegmentId`] is the BLAKE3 digest of
//! its canonical bytes, so two builders anywhere that were given the same
//! documents produce byte-identical segments with the same id — the
//! determinism D-0312 requires, and the property that lets index segments
//! be content-addressed, cached, replicated, and compared without trust.
//!
//! This layer answers only *structural* questions: which documents contain
//! a term, and which contain a phrase (consecutive terms in one field). It
//! computes no score and holds no ranking, payment, provider, or authority
//! field. Turning matches into an ordering is the ranker's job (Track E6).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use mini_crypto::{HashAlgorithm, Multihash};
use mini_web_types::{IndexSegmentId, UrlId};

use crate::codec::{Reader, Writer};
use crate::error::{LexicalIndexError, Result};
use crate::token::{tokenize, Field};

/// Format version written into every segment. Bump only on a breaking
/// change to the byte layout; a decoder refuses versions it does not know
/// rather than silently misreading them.
pub const SEGMENT_FORMAT_VERSION: u8 = 1;

// Decode caps. A segment is normally produced by this crate's own builder,
// but `from_bytes` must also survive hostile input (a segment fetched from
// another participant), so every count is bounded before allocation.
const MAX_URLID_BYTES: usize = 128;
const MAX_TERM_BYTES: usize = 256;
const MAX_DOCUMENTS: usize = 1 << 24;
const MAX_TERMS: usize = 1 << 24;
const MAX_OCCURRENCES: usize = 1 << 24;
const MAX_POSITIONS: usize = 1 << 24;

/// One term's appearance in one field of one document: the sorted
/// positions at which it occurs there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    /// Index into the segment's document table ([`IndexSegment::documents`]).
    pub doc: u32,
    pub field: Field,
    /// Strictly ascending positions within that field.
    pub positions: Vec<u32>,
}

/// An immutable inverted index over a fixed set of documents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSegment {
    /// Documents in canonical order; a `u32` doc index is a position here.
    documents: Vec<UrlId>,
    /// term -> occurrences, each list sorted by `(doc, field tag)`.
    terms: BTreeMap<String, Vec<Occurrence>>,
}

impl IndexSegment {
    /// The documents this segment indexes, in the canonical order that
    /// defines every `doc` index used in an [`Occurrence`].
    pub fn documents(&self) -> &[UrlId] {
        &self.documents
    }

    pub fn document_count(&self) -> usize {
        self.documents.len()
    }

    pub fn term_count(&self) -> usize {
        self.terms.len()
    }

    /// The raw postings for an already-normalized term, or `None`.
    /// Callers usually want [`IndexSegment::term_documents`] instead; this
    /// is for a ranker that needs positions and frequencies.
    pub fn postings(&self, normalized_term: &str) -> Option<&[Occurrence]> {
        self.terms.get(normalized_term).map(|v| v.as_slice())
    }

    /// Documents containing `query_term` in any field, in canonical
    /// (document-table) order. The term is normalized with the same
    /// tokenizer used at index time, so a caller passes raw query text and
    /// need not know the normalization rules; a term that normalizes to
    /// nothing (punctuation only) matches nothing.
    pub fn term_documents(&self, query_term: &str) -> Vec<UrlId> {
        let Some(term) = single_token(query_term) else {
            return Vec::new();
        };
        let Some(occs) = self.terms.get(&term) else {
            return Vec::new();
        };
        let mut docs: BTreeSet<u32> = BTreeSet::new();
        for occ in occs {
            docs.insert(occ.doc);
        }
        docs.into_iter()
            .map(|d| self.documents[d as usize].clone())
            .collect()
    }

    /// Documents where the tokens of `phrase` appear consecutively within
    /// a single field, in canonical order.
    ///
    /// The phrase is tokenized with the index-time tokenizer, so
    /// `"Quick  Brown"` matches text that indexed `quick` then `brown`
    /// adjacently. An empty phrase (no tokens) matches nothing; a
    /// single-token phrase is equivalent to [`IndexSegment::term_documents`].
    /// Matching is per-field: a title ending in `quick` and a body
    /// starting with `brown` is **not** a phrase match, because the ranker
    /// must be able to trust that a phrase hit is a real adjacency, not an
    /// artifact of concatenating fields.
    pub fn phrase_documents(&self, phrase: &str) -> Vec<UrlId> {
        let terms: Vec<String> = tokenize(phrase).into_iter().map(|(t, _)| t).collect();
        if terms.is_empty() {
            return Vec::new();
        }
        if terms.len() == 1 {
            return self.term_documents(&terms[0]);
        }

        // Candidate docs: those containing the first term. For each, test
        // every field for a run where term i sits at position p+i.
        let Some(first_occs) = self.terms.get(&terms[0]) else {
            return Vec::new();
        };
        let candidate_docs: BTreeSet<u32> = first_occs.iter().map(|o| o.doc).collect();

        let mut matched: Vec<u32> = Vec::new();
        for doc in candidate_docs {
            if self.phrase_in_any_field(doc, &terms) {
                matched.push(doc);
            }
        }
        matched.sort_unstable();
        matched
            .into_iter()
            .map(|d| self.documents[d as usize].clone())
            .collect()
    }

    fn phrase_in_any_field(&self, doc: u32, terms: &[String]) -> bool {
        // A phrase can only span one field, so try each field the first
        // term appears in for this document.
        let Some(first_occs) = self.terms.get(&terms[0]) else {
            return false;
        };
        let fields: Vec<Field> = first_occs
            .iter()
            .filter(|o| o.doc == doc)
            .map(|o| o.field)
            .collect();

        for field in fields {
            if self.phrase_in_field(doc, field, terms) {
                return true;
            }
        }
        false
    }

    fn phrase_in_field(&self, doc: u32, field: Field, terms: &[String]) -> bool {
        // Position sets for each term in this (doc, field). If any term is
        // absent here, the phrase cannot occur in this field.
        let mut sets: Vec<HashSet<u32>> = Vec::with_capacity(terms.len());
        for term in terms {
            match self.positions(term, doc, field) {
                Some(p) => sets.push(p.iter().copied().collect()),
                None => return false,
            }
        }
        // Anchor on the first term's positions; check the run forward.
        let Some(first) = self.positions(&terms[0], doc, field) else {
            return false;
        };
        'anchor: for &start in first {
            for (offset, set) in sets.iter().enumerate().skip(1) {
                let Some(want) = start.checked_add(offset as u32) else {
                    continue 'anchor;
                };
                if !set.contains(&want) {
                    continue 'anchor;
                }
            }
            return true;
        }
        false
    }

    fn positions(&self, term: &str, doc: u32, field: Field) -> Option<&[u32]> {
        let occs = self.terms.get(term)?;
        occs.iter()
            .find(|o| o.doc == doc && o.field == field)
            .map(|o| o.positions.as_slice())
    }

    /// This segment's content address: the BLAKE3 digest of its canonical
    /// bytes. Deterministic and stable across hosts.
    pub fn segment_id(&self) -> IndexSegmentId {
        IndexSegmentId(Multihash::of(HashAlgorithm::Blake3, &self.to_bytes()))
    }

    /// The small, verifiable description of this segment.
    pub fn manifest(&self) -> IndexManifest {
        IndexManifest {
            format_version: SEGMENT_FORMAT_VERSION,
            document_count: self.documents.len() as u32,
            term_count: self.terms.len() as u32,
            segment_id: self.segment_id(),
        }
    }

    /// Canonical serialization. Terms come from a `BTreeMap` (sorted), the
    /// document table is built sorted, and occurrences and positions are
    /// emitted in the order the builder fixed — so this is byte-canonical.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(SEGMENT_FORMAT_VERSION);
        w.u32(self.documents.len() as u32);
        for url in &self.documents {
            w.bytes(&url.0.to_bytes());
        }
        w.u32(self.terms.len() as u32);
        for (term, occs) in &self.terms {
            w.str(term);
            w.u32(occs.len() as u32);
            for occ in occs {
                w.u32(occ.doc);
                occ.field.encode(&mut w);
                w.u32(occ.positions.len() as u32);
                for &p in &occ.positions {
                    w.u32(p);
                }
            }
        }
        w.into_bytes()
    }

    /// Decode a segment, enforcing canonical form. Non-canonical input —
    /// unsorted terms, unsorted documents, out-of-order occurrences or
    /// positions — is rejected rather than accepted into a value whose
    /// re-serialization would differ from its own bytes, which would break
    /// the content-address identity.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let version = r.u8()?;
        if version != SEGMENT_FORMAT_VERSION {
            return Err(LexicalIndexError::UnsupportedVersion);
        }

        let doc_count = r.count_limited(MAX_DOCUMENTS)?;
        let mut documents = Vec::with_capacity(doc_count);
        let mut prev_doc_bytes: Option<Vec<u8>> = None;
        for _ in 0..doc_count {
            let raw = r.bytes_limited(MAX_URLID_BYTES)?;
            if let Some(prev) = &prev_doc_bytes {
                if raw <= *prev {
                    // Documents must be strictly ascending by their bytes.
                    return Err(LexicalIndexError::NotCanonical);
                }
            }
            prev_doc_bytes = Some(raw.clone());
            documents.push(UrlId(Multihash::from_bytes(&raw)?));
        }

        let term_count = r.count_limited(MAX_TERMS)?;
        let mut terms: BTreeMap<String, Vec<Occurrence>> = BTreeMap::new();
        let mut prev_term: Option<String> = None;
        for _ in 0..term_count {
            let term = r.str_limited(MAX_TERM_BYTES)?;
            if let Some(prev) = &prev_term {
                if &term <= prev {
                    return Err(LexicalIndexError::NotCanonical);
                }
            }
            prev_term = Some(term.clone());

            let occ_count = r.count_limited(MAX_OCCURRENCES)?;
            let mut occs = Vec::with_capacity(occ_count);
            let mut prev_key: Option<(u32, u8)> = None;
            for _ in 0..occ_count {
                let doc = r.u32()?;
                if doc as usize >= documents.len() {
                    // An occurrence must reference a real document.
                    return Err(LexicalIndexError::NotCanonical);
                }
                let field = Field::decode(&mut r)?;
                let key = (doc, field.tag());
                if let Some(prev) = prev_key {
                    if key <= prev {
                        return Err(LexicalIndexError::NotCanonical);
                    }
                }
                prev_key = Some(key);

                let pos_count = r.count_limited(MAX_POSITIONS)?;
                let mut positions = Vec::with_capacity(pos_count);
                let mut prev_pos: Option<u32> = None;
                for _ in 0..pos_count {
                    let p = r.u32()?;
                    if let Some(pp) = prev_pos {
                        if p <= pp {
                            return Err(LexicalIndexError::NotCanonical);
                        }
                    }
                    prev_pos = Some(p);
                    positions.push(p);
                }
                if positions.is_empty() {
                    // An occurrence with no positions carries no
                    // information and could never be emitted by the
                    // builder; refuse it so the mapping stays 1:1.
                    return Err(LexicalIndexError::NotCanonical);
                }
                occs.push(Occurrence {
                    doc,
                    field,
                    positions,
                });
            }
            if occs.is_empty() {
                return Err(LexicalIndexError::NotCanonical);
            }
            terms.insert(term, occs);
        }

        if !r.finished() {
            return Err(LexicalIndexError::TrailingBytes);
        }
        Ok(IndexSegment { documents, terms })
    }
}

/// A compact, verifiable description of a segment: enough to recognize it
/// and check its integrity without holding the whole postings list. The
/// `segment_id` binds the manifest to exactly one byte sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexManifest {
    pub format_version: u8,
    pub document_count: u32,
    pub term_count: u32,
    pub segment_id: IndexSegmentId,
}

/// Normalize a single query term the same way the indexer did, returning
/// `None` if it normalizes to zero or more than one token (a caller that
/// passes `"quick brown"` to a single-term method gets no match rather
/// than a surprising partial one — phrases go through `phrase_documents`).
fn single_token(raw: &str) -> Option<String> {
    let mut toks = tokenize(raw).into_iter();
    let first = toks.next()?.0;
    if toks.next().is_some() {
        return None;
    }
    Some(first)
}

/// Accumulates documents, then freezes them into an [`IndexSegment`].
#[derive(Debug, Default)]
pub struct IndexBuilder {
    // Keyed by UrlId bytes so iteration is deterministically sorted; the
    // value keeps the original UrlId plus each field's tokens.
    docs: BTreeMap<Vec<u8>, DocDraft>,
}

#[derive(Debug)]
struct DocDraft {
    url: UrlId,
    /// field -> (term -> ascending positions)
    fields: BTreeMap<u8, BTreeMap<String, Vec<u32>>>,
}

impl IndexBuilder {
    pub fn new() -> Self {
        IndexBuilder {
            docs: BTreeMap::new(),
        }
    }

    /// Add one document's fields. Adding the same [`UrlId`] twice replaces
    /// the earlier draft: a rebuild from a fresh crawl of the same URL
    /// should supersede, not accumulate, and "last write wins" keeps the
    /// result independent of insertion order among distinct URLs while
    /// staying predictable for a repeated one.
    pub fn add_document(&mut self, url: UrlId, fields: &[(Field, &str)]) {
        let mut draft = DocDraft {
            url: url.clone(),
            fields: BTreeMap::new(),
        };
        for (field, text) in fields {
            let entry = draft.fields.entry(field.tag()).or_default();
            for (term, pos) in tokenize(text) {
                entry.entry(term).or_default().push(pos);
            }
        }
        self.docs.insert(url.0.to_bytes(), draft);
    }

    /// Freeze the accumulated documents into an immutable segment.
    pub fn build(self) -> IndexSegment {
        let documents: Vec<UrlId> = self.docs.values().map(|d| d.url.clone()).collect();

        // Invert: term -> (doc index, field) -> positions. BTreeMaps keep
        // every level sorted so the resulting segment is canonical.
        let mut inverted: BTreeMap<String, BTreeMap<(u32, u8), Vec<u32>>> = BTreeMap::new();
        for (doc_index, draft) in self.docs.values().enumerate() {
            let doc_index = doc_index as u32;
            for (field_tag, terms) in &draft.fields {
                for (term, positions) in terms {
                    let mut positions = positions.clone();
                    positions.sort_unstable();
                    positions.dedup();
                    inverted
                        .entry(term.clone())
                        .or_default()
                        .insert((doc_index, *field_tag), positions);
                }
            }
        }

        let mut terms: BTreeMap<String, Vec<Occurrence>> = BTreeMap::new();
        for (term, by_key) in inverted {
            let occs = by_key
                .into_iter()
                .map(|((doc, field_tag), positions)| Occurrence {
                    doc,
                    // field_tag came from Field::tag(), so this cannot fail.
                    field: Field::from_tag(field_tag).expect("tag from Field::tag"),
                    positions,
                })
                .collect();
            terms.insert(term, occs);
        }

        IndexSegment { documents, terms }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(seed: &[u8]) -> UrlId {
        UrlId(Multihash::of(HashAlgorithm::Blake3, seed))
    }

    fn sample() -> IndexSegment {
        let mut b = IndexBuilder::new();
        b.add_document(
            url(b"doc-a"),
            &[
                (Field::Title, "The Quick Brown Fox"),
                (Field::Body, "A quick brown fox jumps over the lazy dog"),
            ],
        );
        b.add_document(
            url(b"doc-b"),
            &[
                (Field::Title, "Lazy Dogs Sleeping"),
                (Field::Body, "the dog was not quick at all"),
            ],
        );
        b.build()
    }

    #[test]
    fn term_lookup_finds_the_right_documents() {
        let seg = sample();
        // "fox" only in doc-a; "dog" in both.
        assert_eq!(seg.term_documents("fox"), vec![url(b"doc-a")]);
        let dogs = seg.term_documents("Dog"); // case-insensitive
        assert_eq!(dogs.len(), 2);
        assert!(dogs.contains(&url(b"doc-a")));
        assert!(dogs.contains(&url(b"doc-b")));
    }

    #[test]
    fn a_missing_or_empty_term_matches_nothing() {
        let seg = sample();
        assert!(seg.term_documents("elephant").is_empty());
        assert!(seg.term_documents("!!!").is_empty());
        // A multi-word string is not a single term.
        assert!(seg.term_documents("quick brown").is_empty());
    }

    #[test]
    fn phrase_match_respects_adjacency() {
        let seg = sample();
        // "quick brown" is adjacent in doc-a (both title and body).
        assert_eq!(seg.phrase_documents("quick brown"), vec![url(b"doc-a")]);
        // "brown fox" adjacent only in doc-a.
        assert_eq!(seg.phrase_documents("Brown Fox"), vec![url(b"doc-a")]);
    }

    #[test]
    fn phrase_match_does_not_span_fields() {
        // doc-b body ends "...quick at all"; nothing makes "quick dog"
        // adjacent, and title/body must not be concatenated.
        let seg = sample();
        assert!(seg.phrase_documents("quick dog").is_empty());
    }

    #[test]
    fn a_non_adjacent_word_pair_is_not_a_phrase() {
        let seg = sample();
        // "quick" and "fox" both appear in doc-a body but "quick brown
        // fox" has brown between them, so "quick fox" is not a phrase.
        assert!(seg.phrase_documents("quick fox").is_empty());
        // ...but the three-word phrase is present.
        assert_eq!(seg.phrase_documents("quick brown fox"), vec![url(b"doc-a")]);
    }

    #[test]
    fn a_single_word_phrase_equals_a_term_query() {
        let seg = sample();
        assert_eq!(seg.phrase_documents("fox"), seg.term_documents("fox"));
    }

    #[test]
    fn building_is_deterministic_and_content_addressed() {
        // Same documents, opposite insertion order -> identical bytes/id.
        let mut b1 = IndexBuilder::new();
        b1.add_document(url(b"one"), &[(Field::Body, "alpha beta")]);
        b1.add_document(url(b"two"), &[(Field::Body, "beta gamma")]);

        let mut b2 = IndexBuilder::new();
        b2.add_document(url(b"two"), &[(Field::Body, "beta gamma")]);
        b2.add_document(url(b"one"), &[(Field::Body, "alpha beta")]);

        let s1 = b1.build();
        let s2 = b2.build();
        assert_eq!(s1.to_bytes(), s2.to_bytes());
        assert_eq!(s1.segment_id(), s2.segment_id());
    }

    #[test]
    fn adding_the_same_url_twice_replaces_it() {
        let mut b = IndexBuilder::new();
        b.add_document(url(b"x"), &[(Field::Body, "first version")]);
        b.add_document(url(b"x"), &[(Field::Body, "second version")]);
        let seg = b.build();
        assert_eq!(seg.document_count(), 1);
        assert!(seg.term_documents("second").len() == 1);
        assert!(seg.term_documents("first").is_empty());
    }

    #[test]
    fn a_segment_round_trips_through_bytes() {
        let seg = sample();
        let bytes = seg.to_bytes();
        let decoded = IndexSegment::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, seg);
        assert_eq!(decoded.segment_id(), seg.segment_id());
    }

    #[test]
    fn the_manifest_matches_the_segment() {
        let seg = sample();
        let m = seg.manifest();
        assert_eq!(m.document_count as usize, seg.document_count());
        assert_eq!(m.term_count as usize, seg.term_count());
        assert_eq!(m.segment_id, seg.segment_id());
        assert_eq!(m.format_version, SEGMENT_FORMAT_VERSION);
    }

    #[test]
    fn an_empty_index_is_valid_and_round_trips() {
        let seg = IndexBuilder::new().build();
        assert_eq!(seg.document_count(), 0);
        assert_eq!(seg.term_count(), 0);
        let decoded = IndexSegment::from_bytes(&seg.to_bytes()).unwrap();
        assert_eq!(decoded, seg);
    }

    #[test]
    fn a_wrong_version_byte_is_rejected() {
        let seg = sample();
        let mut bytes = seg.to_bytes();
        bytes[0] = 0xFF;
        assert_eq!(
            IndexSegment::from_bytes(&bytes),
            Err(LexicalIndexError::UnsupportedVersion)
        );
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let seg = sample();
        let mut bytes = seg.to_bytes();
        bytes.push(0);
        assert_eq!(
            IndexSegment::from_bytes(&bytes),
            Err(LexicalIndexError::TrailingBytes)
        );
    }

    #[test]
    fn truncation_at_every_offset_is_rejected_without_panic() {
        let bytes = sample().to_bytes();
        for cut in 0..bytes.len() {
            // Must be an error, and must never panic.
            assert!(IndexSegment::from_bytes(&bytes[..cut]).is_err());
        }
    }

    #[test]
    fn non_canonical_unsorted_terms_are_rejected() {
        // Hand-build a two-term segment with terms in the wrong order.
        let mut w = Writer::new();
        w.u8(SEGMENT_FORMAT_VERSION);
        w.u32(1);
        w.bytes(&url(b"d").0.to_bytes());
        w.u32(2);
        // term "b" before "a" -> not canonical
        for term in ["b", "a"] {
            w.str(term);
            w.u32(1);
            w.u32(0);
            Field::Body.encode(&mut w);
            w.u32(1);
            w.u32(0);
        }
        assert_eq!(
            IndexSegment::from_bytes(&w.into_bytes()),
            Err(LexicalIndexError::NotCanonical)
        );
    }

    #[test]
    fn an_occurrence_referencing_a_missing_document_is_rejected() {
        let mut w = Writer::new();
        w.u8(SEGMENT_FORMAT_VERSION);
        w.u32(1);
        w.bytes(&url(b"d").0.to_bytes());
        w.u32(1);
        w.str("alpha");
        w.u32(1);
        w.u32(5); // doc index 5, but only 1 document exists
        Field::Body.encode(&mut w);
        w.u32(1);
        w.u32(0);
        assert_eq!(
            IndexSegment::from_bytes(&w.into_bytes()),
            Err(LexicalIndexError::NotCanonical)
        );
    }
}
