//! Static HTML extraction for MiniSearch (D-0312, Track E4 —
//! `docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
//! §14.3).
//!
//! [`extract`] takes already-fetched HTML text (a `mini-crawler` runtime's
//! future job, not this crate's) and returns a [`PageExtract`]: title,
//! headings, visible text, links, language, metadata, a canonical-link hint,
//! a content digest, and a few structural signals (script/iframe counts,
//! hidden-text byte count) a later `mini-ranker` can use. It is a pure
//! function — same bytes in, same [`PageExtract`] out, no I/O, no network
//! access, no JavaScript execution, no external parsing crate.
//!
//! **"Sandboxed-in-principle," not sandboxed today.** The doctrine document
//! calls for running page parsing in isolation, matching D-0069's `wasm-
//! component`/isolated-runner discipline elsewhere in this tree. This crate
//! does not spawn a process or a Wasm sandbox — it hand-rolls a small,
//! `#![forbid(unsafe_code)]` tokenizer with no `unsafe`, no third-party HTML
//! parsing dependency (so no external dependency's CVE surface, matching the
//! same reasoning `mini-crypto::multihash` gives for hand-rolling instead of
//! importing), and, critically, it never executes anything found in the
//! document: `<script>`/`<style>`/`<noscript>`/`<template>` content is
//! always treated as opaque bytes to skip past, never parsed as markup or
//! evaluated. Wiring this behind `mini-extract-host`'s real process
//! isolation (or an equivalent) remains real, separately-scoped follow-up —
//! this crate's own memory safety is what stands between a hostile page and
//! the caller today, not a process boundary.
//!
//! What this crate deliberately does not attempt: full HTML5 tree
//! construction (malformed nesting is repaired by simple stack-matching
//! heuristics, not the specification's algorithm); CSS parsing beyond a
//! literal `display:none`/`visibility:hidden`/`hidden`-attribute check for
//! hidden-text detection (an externally-stylesheet-driven hide is invisible
//! to this crate); relative-URL resolution (`ExtractedLink::href` is exactly
//! what the document wrote); language-tag validation; and any judgment about
//! spam, quality, or ranking — those are `mini-ranker`'s job, over the
//! [`PageExtract::hidden_text_byte_count`] and similar raw signals this
//! crate only measures; and true RCDATA parsing of `<title>` (real HTML5
//! treats its content as text-only — this crate instead parses markup
//! nested inside an unclosed `<title>` normally and additionally folds its
//! descendant text into the title, a simplification that only differs from
//! spec behavior on the rare malformed-and-never-closed `<title>` case).

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod limits;
mod parse;
mod types;

pub use error::{ExtractError, Result};
pub use limits::{
    MAX_ANCHOR_TEXT_BYTES, MAX_EXTERNAL_SCRIPT_HOSTS, MAX_HEADINGS, MAX_HEADING_TEXT_BYTES,
    MAX_HTML_BYTES, MAX_LINKS, MAX_META_ENTRIES, MAX_TITLE_BYTES, MAX_VISIBLE_TEXT_BYTES,
};
pub use types::{ExtractedLink, Heading, HeadingLevel, MetaEntry, PageExtract};

/// Extract [`PageExtract`] from already-fetched HTML text.
///
/// Returns [`ExtractError::InputTooLarge`] if `html.len()` exceeds
/// [`MAX_HTML_BYTES`]; otherwise never fails. Malformed markup degrades
/// gracefully (see the crate doc comment) rather than erroring, since a
/// crawler will see arbitrarily broken pages in practice and "extraction
/// found nothing" is a more honest failure mode than refusing the page
/// outright.
pub fn extract(html: &str) -> Result<PageExtract> {
    parse::extract(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_title_headings_text_and_links() {
        let html = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <title>Example Domain</title>
  <meta name="description" content="An example page">
  <link rel="canonical" href="https://example.org/">
</head>
<body>
  <h1>Welcome</h1>
  <p>This domain is for <a href="/use-cases" rel="nofollow">use cases</a>.</p>
  <h2>More info</h2>
  <p>See <a href="https://iana.org/domains/example">IANA</a> for details.</p>
</body>
</html>"#;
        let page = extract(html).unwrap();
        assert_eq!(page.title.as_deref(), Some("Example Domain"));
        assert_eq!(page.language.as_deref(), Some("en"));
        assert_eq!(page.canonical_href.as_deref(), Some("https://example.org/"));
        assert_eq!(
            page.meta,
            vec![MetaEntry {
                name: "description".to_string(),
                content: "An example page".to_string(),
            }]
        );
        assert_eq!(page.headings.len(), 2);
        assert_eq!(page.headings[0].level, HeadingLevel::H1);
        assert_eq!(page.headings[0].text, "Welcome");
        assert_eq!(page.headings[1].text, "More info");
        assert_eq!(page.links.len(), 2);
        assert_eq!(page.links[0].href, "/use-cases");
        assert_eq!(page.links[0].rel.as_deref(), Some("nofollow"));
        assert_eq!(page.links[0].anchor_text, "use cases");
        assert_eq!(page.links[1].href, "https://iana.org/domains/example");
        assert!(page.visible_text.contains("Welcome"));
        assert!(page.visible_text.contains("use cases"));
        assert!(page.visible_text.contains("IANA"));
        assert!(!page.truncated);
    }

    #[test]
    fn script_and_style_content_is_never_treated_as_text_or_markup() {
        let html = r#"<html><head><style>body { color: red; }</style>
<script src="https://cdn.example.net/app.js">if (1 < 2) { alert('safe'); }</script>
</head><body><p>Real text</p><script>var x = "<h1>fake heading</h1>";</script></body></html>"#;
        let page = extract(html).unwrap();
        assert_eq!(page.script_tag_count, 2);
        assert_eq!(page.external_script_hosts, vec!["cdn.example.net"]);
        assert!(page.headings.is_empty());
        assert_eq!(page.visible_text.trim(), "Real text");
        assert!(!page.visible_text.contains("color"));
        assert!(!page.visible_text.contains("alert"));
    }

    /// A literal `</script` sequence inside a script body (e.g. embedded in a
    /// JS string) ends raw-text scanning at that point, exactly like a real
    /// browser's HTML parser — this is why real-world inline scripts must
    /// escape it (`"<\/script>"`) if the literal text is ever needed. The
    /// tail after that point is parsed as ordinary markup/text, which is
    /// this crate's honest, spec-matching (if surprising) behavior.
    #[test]
    fn a_literal_close_script_sequence_inside_the_script_ends_raw_text_scanning() {
        let html = r#"<script>var s = "</script> tail text";</script>"#;
        let page = extract(html).unwrap();
        assert!(page.visible_text.contains("tail text"));
    }

    #[test]
    fn hidden_text_is_counted_but_excluded_from_visible_text() {
        let html = r#"<body>
<p>Visible content here</p>
<div style="display: none">Cloaked keyword stuffing text</div>
<span hidden>also hidden</span>
</body>"#;
        let page = extract(html).unwrap();
        assert!(page.visible_text.contains("Visible content here"));
        assert!(!page.visible_text.contains("Cloaked"));
        assert!(!page.visible_text.contains("also hidden"));
        assert!(page.hidden_text_byte_count > 0);
    }

    #[test]
    fn relative_script_sources_are_not_misclassified_as_a_host() {
        let page = extract(r#"<script src="/static/app.js"></script>"#).unwrap();
        assert_eq!(page.script_tag_count, 1);
        assert!(page.external_script_hosts.is_empty());
    }

    #[test]
    fn html_entities_are_decoded() {
        let page = extract("<p>Fish &amp; Chips &lt;tasty&gt; &#65;&#x42;</p>").unwrap();
        assert!(page.visible_text.contains("Fish & Chips <tasty> AB"));
    }

    #[test]
    fn malformed_unclosed_markup_still_yields_partial_output() {
        let html = "<html><head><title>Broken</title><body><h1>Heading without close<p>Text";
        let page = extract(html).unwrap();
        assert_eq!(page.title.as_deref(), Some("Broken"));
        assert_eq!(page.headings.len(), 1);
        assert!(page.headings[0].text.starts_with("Heading without close"));
        assert!(page.visible_text.contains("Text"));
    }

    /// `<title>` has no matching `</title>` anywhere in the document. Unlike
    /// real HTML5 (where `<title>` is a text-only/RCDATA element — nested
    /// `<` never starts a real tag), this crate parses markup nested inside
    /// an open `<title>` normally and only *additionally* folds its
    /// descendant text into the title buffer; a documented simplification,
    /// not spec-accurate RCDATA. So `<h1>` here is captured as a real
    /// heading *and* its text becomes part of the never-closed title.
    #[test]
    fn a_title_with_no_closing_tag_anywhere_consumes_the_rest_of_the_document() {
        let html = "<title>Broken<body><h1>Not a real heading</h1>";
        let page = extract(html).unwrap();
        assert_eq!(page.title.as_deref(), Some("Broken Not a real heading"));
        assert_eq!(page.headings.len(), 1);
        assert_eq!(page.headings[0].text, "Not a real heading");
    }

    #[test]
    fn stray_end_tags_with_no_matching_open_tag_are_ignored() {
        let page = extract("<p>Hello</div></p> World</span>").unwrap();
        assert!(page.visible_text.contains("Hello"));
        assert!(page.visible_text.contains("World"));
    }

    #[test]
    fn oversized_input_is_rejected_before_parsing() {
        let huge = "x".repeat(MAX_HTML_BYTES + 1);
        assert_eq!(
            extract(&huge),
            Err(ExtractError::InputTooLarge {
                byte_length: huge.len()
            })
        );
    }

    #[test]
    fn content_digest_is_deterministic_and_reflects_only_visible_text() {
        let a = extract("<p>Same text</p>").unwrap();
        let b = extract("<div><span>Same text</span></div>").unwrap();
        assert_eq!(a.content_digest, b.content_digest);

        let c = extract("<p>Different text</p>").unwrap();
        assert_ne!(a.content_digest, c.content_digest);
    }

    #[test]
    fn only_the_first_title_is_kept() {
        let page = extract("<title>First</title><title>Second</title>").unwrap();
        assert_eq!(page.title.as_deref(), Some("First"));
    }

    #[test]
    fn adjacent_element_text_never_glues_together() {
        let page = extract("<p>Hello</p><p>World</p>").unwrap();
        assert!(page.visible_text.contains("Hello World"));
        assert!(!page.visible_text.contains("HelloWorld"));
    }

    #[test]
    fn canonical_link_requires_rel_canonical() {
        let page = extract(
            r#"<link rel="stylesheet" href="/style.css"><link rel="canonical" href="https://example.org/x">"#,
        )
        .unwrap();
        assert_eq!(
            page.canonical_href.as_deref(),
            Some("https://example.org/x")
        );
    }

    #[test]
    fn iframe_tags_are_counted() {
        let page = extract(r#"<iframe src="https://example.org/embed"></iframe>"#).unwrap();
        assert_eq!(page.iframe_tag_count, 1);
    }
}
