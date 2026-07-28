//! The document fields the index distinguishes, and the deterministic
//! tokenizer that turns field text into positioned terms.
//!
//! Fields exist so a later ranker (Track E6) can weight a title match
//! differently from a body match. This crate assigns **no** weights: it
//! records which field each occurrence came from and stops there. Deciding
//! what a field is *worth* is ranking, a separate layer (D-0312 keeps
//! discovery, availability, and ranking distinct).

use crate::codec::{Reader, Writer};
use crate::error::{LexicalIndexError, Result};

/// Which part of a document a term occurrence came from.
///
/// `#[non_exhaustive]` because new fields (e.g. anchor text, headings) are
/// a forward-compatible extension — but every field must have a stable
/// wire tag, since it participates in the segment's content address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Field {
    /// The document's title / `<title>`-equivalent.
    Title,
    /// The main extracted body text.
    Body,
    /// The document's own URL, tokenized so host and path words are
    /// findable (e.g. a query for a domain word matches its pages).
    Url,
}

impl Field {
    /// Stable wire tag. Public so a caller building queries or inspecting a
    /// segment refers to the same value the codec writes.
    pub fn tag(self) -> u8 {
        match self {
            Field::Title => 1,
            Field::Body => 2,
            Field::Url => 3,
        }
    }

    pub fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            1 => Ok(Field::Title),
            2 => Ok(Field::Body),
            3 => Ok(Field::Url),
            _ => Err(LexicalIndexError::BadField),
        }
    }

    pub(crate) fn encode(self, w: &mut Writer) {
        w.u8(self.tag());
    }

    pub(crate) fn decode(r: &mut Reader) -> Result<Self> {
        Field::from_tag(r.u8()?)
    }
}

/// Longest token this tokenizer will emit. A single "word" longer than
/// this (a base64 blob, a minified script fragment that slipped through
/// extraction) is truncated rather than allowed to bloat the term
/// dictionary without bound.
pub const MAX_TOKEN_CHARS: usize = 64;

/// Split `text` into lowercased terms paired with their 0-based position
/// **within this field**, deterministically.
///
/// The rules are intentionally simple and total, because determinism
/// matters more here than linguistic sophistication (stemming, locale
/// casing, and synonyms belong in a ranker or a query expander, not in the
/// one canonical index everyone must agree on byte-for-byte):
///
/// - a token is a maximal run of Unicode alphanumeric characters;
/// - every other character is a separator;
/// - tokens are lowercased with `char::to_lowercase` (Unicode, locale-
///   independent, so the same input yields the same output on every host);
/// - a token longer than [`MAX_TOKEN_CHARS`] chars is truncated to that
///   many chars;
/// - positions count emitted tokens in order, starting at 0, so adjacent
///   tokens have adjacent positions — which is what phrase search needs.
///
/// Positions count tokens, not characters or separators, so "quick  brown"
/// (two spaces) and "quick brown" produce the same positions. That is
/// deliberate: phrase adjacency should not depend on how much whitespace
/// separated two words.
pub fn tokenize(text: &str) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut position: u32 = 0;

    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if current.chars().count() < MAX_TOKEN_CHARS {
                for lower in ch.to_lowercase() {
                    current.push(lower);
                }
            }
            // else: past the cap, drop further chars of this one token.
        } else if !current.is_empty() {
            // Guard against a lowercase mapping pushing us over the cap.
            truncate_to_chars(&mut current, MAX_TOKEN_CHARS);
            out.push((core::mem::take(&mut current), position));
            position += 1;
        }
    }
    if !current.is_empty() {
        truncate_to_chars(&mut current, MAX_TOKEN_CHARS);
        out.push((current, position));
    }
    out
}

fn truncate_to_chars(s: &mut String, max: usize) {
    if s.chars().count() > max {
        let end = s.char_indices().nth(max).map(|(i, _)| i).unwrap_or(s.len());
        s.truncate(end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_field_tag_round_trips() {
        for field in [Field::Title, Field::Body, Field::Url] {
            assert_eq!(Field::from_tag(field.tag()).unwrap(), field);
        }
    }

    #[test]
    fn an_unrecognized_field_tag_is_rejected() {
        assert_eq!(Field::from_tag(0xFF), Err(LexicalIndexError::BadField));
    }

    #[test]
    fn tokenize_lowercases_and_positions_in_order() {
        assert_eq!(
            tokenize("The Quick Brown Fox"),
            vec![
                ("the".to_string(), 0),
                ("quick".to_string(), 1),
                ("brown".to_string(), 2),
                ("fox".to_string(), 3),
            ]
        );
    }

    #[test]
    fn runs_of_separators_collapse_and_do_not_advance_position() {
        // Extra whitespace and punctuation must not create phantom gaps in
        // the position sequence, or phrase adjacency would break on
        // whitespace differences.
        assert_eq!(
            tokenize("quick,,,   brown"),
            vec![("quick".to_string(), 0), ("brown".to_string(), 1)]
        );
    }

    #[test]
    fn unicode_alphanumerics_are_tokens_and_lowercased() {
        assert_eq!(
            tokenize("Café ÜBER 123"),
            vec![
                ("café".to_string(), 0),
                ("über".to_string(), 1),
                ("123".to_string(), 2),
            ]
        );
    }

    #[test]
    fn an_overlong_token_is_truncated_not_dropped() {
        let long = "a".repeat(MAX_TOKEN_CHARS + 50);
        let toks = tokenize(&long);
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].1, 0);
        assert_eq!(toks[0].0.chars().count(), MAX_TOKEN_CHARS);
    }

    #[test]
    fn empty_and_separator_only_text_yields_no_tokens() {
        assert!(tokenize("").is_empty());
        assert!(tokenize("   ...--- \n\t").is_empty());
    }

    #[test]
    fn tokenizing_is_deterministic() {
        let text = "Résumé of the QUICK brown-fox, version 2!";
        assert_eq!(tokenize(text), tokenize(text));
    }
}
