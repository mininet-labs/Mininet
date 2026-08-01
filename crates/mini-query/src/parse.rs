//! Turn a raw user query string into a [`ParsedQuery`] (Track E7 of
//! `docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`,
//! §E7): exact phrases, a `site:`/host filter, `-term` exclusions,
//! `before:`/`after:` date bounds, `lang:`, and `type:`.
//!
//! This is deliberately a small, hand-rolled, deterministic parser over a
//! fixed token grammar -- not a general query language. Unpersonalized mode
//! is not a token this parser emits: `mini_ranker::rank` takes no per-user
//! state at all, so "no personalization" already holds by construction
//! (the same structural argument that crate's own docs make), not by a flag
//! this crate could fail to set.

use mini_web_types::{NormalizedHost, WebMediaType};

/// A raw query string parsed into its structured pieces. `terms` and
/// `phrase` map directly onto [`mini_ranker::Query`]; the rest are this
/// crate's own filters, applied in [`crate::search`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedQuery {
    /// Plain terms (from unquoted words, and quoted-phrase words too --
    /// mirrors `mini_ranker::Query::with_phrase`'s own "phrase words also
    /// count as terms" rule).
    pub terms: Vec<String>,
    /// At most one exact phrase (`"exact phrase"`), consistent with
    /// `mini_ranker::Query`'s own single-phrase limit.
    pub phrase: Option<String>,
    /// Single-token exclusions (`-word`). Excluding a whole phrase is out
    /// of scope for this parser.
    pub excluded_terms: Vec<String>,
    /// `site:example.com` / `host:example.com`.
    pub host_filter: Option<NormalizedHost>,
    /// `before:YYYY-MM-DD`, inclusive upper bound in ms since the Unix
    /// epoch (midnight UTC of the given day) -- documents observed on or
    /// after this instant are excluded.
    pub before_ms: Option<u64>,
    /// `after:YYYY-MM-DD`, inclusive lower bound (midnight UTC of the
    /// *next* day) -- documents observed at or before the given day are
    /// excluded.
    pub after_ms: Option<u64>,
    /// `lang:xx`.
    pub language: Option<String>,
    /// `type:html` / `type:pdf` / ... (see [`parse_media_type`]).
    pub media_type: Option<WebMediaType>,
}

/// Parse one raw query string. Malformed filter tokens (an unparsable date,
/// an invalid host) are dropped silently rather than failing the whole
/// query -- the surrounding words still search normally, the same
/// "best-effort filter, never a hard error on user input" posture a search
/// box needs. A caller that wants strict validation should validate the
/// filter tokens itself before calling this.
pub fn parse_query(raw: &str) -> ParsedQuery {
    let mut out = ParsedQuery::default();
    let mut terms: Vec<String> = Vec::new();
    let mut rest = raw;

    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        if let Some(after_quote) = rest.strip_prefix('"') {
            let (phrase, remainder) = match after_quote.find('"') {
                Some(end) => (&after_quote[..end], &after_quote[end + 1..]),
                None => (after_quote, ""),
            };
            if !phrase.trim().is_empty() && out.phrase.is_none() {
                out.phrase = Some(phrase.to_string());
                terms.push(phrase.to_string());
            }
            rest = remainder;
            continue;
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (token, remainder) = rest.split_at(end);
        apply_token(token, &mut out, &mut terms);
        rest = remainder;
    }

    out.terms = terms;
    out
}

fn apply_token(token: &str, out: &mut ParsedQuery, terms: &mut Vec<String>) {
    if let Some(word) = token.strip_prefix('-') {
        if !word.is_empty() {
            out.excluded_terms.push(word.to_ascii_lowercase());
        }
        return;
    }
    if let Some(host) = token
        .strip_prefix("site:")
        .or_else(|| token.strip_prefix("host:"))
    {
        if let Ok(h) = NormalizedHost::new(host) {
            out.host_filter = Some(h);
        }
        return;
    }
    if let Some(date) = token.strip_prefix("before:") {
        if let Some(ms) = parse_date_ms(date) {
            out.before_ms = Some(ms);
        }
        return;
    }
    if let Some(date) = token.strip_prefix("after:") {
        // Inclusive lower bound: exclude anything observed at or before
        // this day, i.e. the bound is midnight of the *next* day.
        if let Some(ms) = parse_date_ms(date) {
            out.after_ms = Some(ms.saturating_add(86_400_000));
        }
        return;
    }
    if let Some(lang) = token.strip_prefix("lang:") {
        if !lang.is_empty() {
            out.language = Some(lang.to_ascii_lowercase());
        }
        return;
    }
    if let Some(ty) = token.strip_prefix("type:") {
        out.media_type = Some(parse_media_type(ty));
        return;
    }
    if !token.is_empty() {
        terms.push(token.to_string());
    }
}

/// A small, fixed vocabulary of `type:` tokens onto [`WebMediaType`];
/// anything else becomes `WebMediaType::Other`, so a `type:` filter never
/// silently fails to parse -- it just may not match anything.
fn parse_media_type(token: &str) -> WebMediaType {
    match token.to_ascii_lowercase().as_str() {
        "html" | "htm" => WebMediaType::Html,
        "text" | "txt" | "plain" => WebMediaType::TextPlain,
        "md" | "markdown" => WebMediaType::Markdown,
        "json" => WebMediaType::Json,
        "pdf" => WebMediaType::Pdf,
        "image" | "img" => WebMediaType::Image,
        other => WebMediaType::Other(other.to_string()),
    }
}

/// Days from the Unix epoch (1970-01-01) for a proleptic-Gregorian
/// `(year, month, day)`, via Howard Hinnant's well-known `days_from_civil`
/// algorithm (public domain integer arithmetic) -- a full calendar library
/// dependency is not warranted for this crate's one `YYYY-MM-DD` filter.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m + 9) % 12; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// Parse a strict `YYYY-MM-DD` date into milliseconds since the Unix epoch
/// (midnight UTC). Returns `None` for anything else -- no partial dates, no
/// timezone offsets, no two-digit years.
fn parse_date_ms(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: i64 = parts[1].parse().ok()?;
    let d: i64 = parts[2].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let days = days_from_civil(y, m, d);
    if days < 0 {
        return None;
    }
    Some((days as u64).saturating_mul(86_400_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_terms_are_collected_in_order() {
        let q = parse_query("rust programming language");
        assert_eq!(q.terms, vec!["rust", "programming", "language"]);
        assert_eq!(q.phrase, None);
    }

    #[test]
    fn a_quoted_phrase_is_captured_and_also_counts_as_a_term() {
        let q = parse_query(r#"intro "systems programming" guide"#);
        assert_eq!(q.phrase.as_deref(), Some("systems programming"));
        assert_eq!(q.terms, vec!["intro", "systems programming", "guide"]);
    }

    #[test]
    fn only_the_first_phrase_is_kept() {
        let q = parse_query(r#""first phrase" "second phrase""#);
        assert_eq!(q.phrase.as_deref(), Some("first phrase"));
    }

    #[test]
    fn exclusion_tokens_are_collected_lowercase() {
        let q = parse_query("rust -Java -c++");
        assert_eq!(q.terms, vec!["rust"]);
        assert_eq!(q.excluded_terms, vec!["java", "c++"]);
    }

    #[test]
    fn site_and_host_filters_parse_a_normalized_host() {
        let q = parse_query("rust site:Example.Org.");
        assert_eq!(
            q.host_filter.as_ref().map(|h| h.as_str()),
            Some("example.org")
        );
        let q2 = parse_query("rust host:example.org");
        assert_eq!(
            q2.host_filter.as_ref().map(|h| h.as_str()),
            Some("example.org")
        );
    }

    #[test]
    fn an_invalid_host_filter_is_silently_dropped() {
        let q = parse_query("rust site:..bad..");
        assert_eq!(q.host_filter, None);
        assert_eq!(q.terms, vec!["rust"]);
    }

    #[test]
    fn before_and_after_parse_to_midnight_utc_ms_with_after_exclusive() {
        let q = parse_query("rust before:2026-01-01 after:2025-01-01");
        // 2026-01-01 00:00:00 UTC.
        assert_eq!(q.before_ms, Some(1_767_225_600_000));
        // after: is the day AFTER 2025-01-01, i.e. 2025-01-02 00:00:00 UTC.
        assert_eq!(q.after_ms, Some(1_735_776_000_000));
    }

    #[test]
    fn a_malformed_date_is_silently_dropped() {
        let q = parse_query("rust before:not-a-date after:2025-13-99");
        assert_eq!(q.before_ms, None);
        assert_eq!(q.after_ms, None);
    }

    #[test]
    fn lang_and_type_tokens_parse() {
        let q = parse_query("rust lang:en type:pdf");
        assert_eq!(q.language.as_deref(), Some("en"));
        assert_eq!(q.media_type, Some(WebMediaType::Pdf));
    }

    #[test]
    fn an_unrecognized_type_token_becomes_other() {
        let q = parse_query("rust type:epub");
        assert_eq!(q.media_type, Some(WebMediaType::Other("epub".to_string())));
    }

    #[test]
    fn an_empty_query_produces_no_terms() {
        let q = parse_query("   ");
        assert!(q.terms.is_empty());
        assert_eq!(q.phrase, None);
    }
}
