//! The structured query the ranker scores against.
//!
//! This is *not* a query parser — turning a user's raw query string (with
//! quoting, `site:` filters, exclusions, date bounds) into this structure
//! is Track E7's job. E6 takes an already-structured query: a set of terms
//! and, optionally, one exact phrase.

use mini_lexical_index::tokenize;

/// A structured query: terms to match, and an optional exact phrase whose
/// tokens must appear consecutively in one field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    /// Query terms, normalized with the index-time tokenizer so they match
    /// what was indexed. Empty terms (punctuation) are dropped.
    terms: Vec<String>,
    /// An optional exact phrase, kept as raw text; the ranker tokenizes it
    /// with the same tokenizer when testing phrase matches.
    phrase: Option<String>,
}

impl Query {
    /// Build a query from raw term strings. Each is tokenized and its tokens
    /// added, so `Query::new(["quick brown", "fox"])` yields the three terms
    /// `quick`, `brown`, `fox`. Duplicate terms are collapsed so a repeated
    /// word does not double-count in coverage.
    pub fn new<I, S>(terms: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut out = Vec::new();
        for raw in terms {
            for (tok, _) in tokenize(raw.as_ref()) {
                if !out.contains(&tok) {
                    out.push(tok);
                }
            }
        }
        Query {
            terms: out,
            phrase: None,
        }
    }

    /// Attach an exact phrase. The phrase's own words also count as terms
    /// (so a phrase query still contributes to lexical coverage), but the
    /// phrase signal fires only when they are adjacent.
    pub fn with_phrase(mut self, phrase: impl Into<String>) -> Self {
        let phrase = phrase.into();
        for (tok, _) in tokenize(&phrase) {
            if !self.terms.contains(&tok) {
                self.terms.push(tok);
            }
        }
        self.phrase = if tokenize(&phrase).is_empty() {
            None
        } else {
            Some(phrase)
        };
        self
    }

    pub fn terms(&self) -> &[String] {
        &self.terms
    }

    pub fn phrase(&self) -> Option<&str> {
        self.phrase.as_deref()
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}
