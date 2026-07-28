use mini_crypto::Multihash;

/// Heading level, `<h1>`..`<h6>` only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HeadingLevel {
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
}

impl HeadingLevel {
    pub(crate) fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "h1" => Some(HeadingLevel::H1),
            "h2" => Some(HeadingLevel::H2),
            "h3" => Some(HeadingLevel::H3),
            "h4" => Some(HeadingLevel::H4),
            "h5" => Some(HeadingLevel::H5),
            "h6" => Some(HeadingLevel::H6),
            _ => None,
        }
    }
}

/// One extracted heading and its collapsed inner text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: HeadingLevel,
    pub text: String,
}

/// One extracted `<a href="...">` — the href exactly as written in the
/// document, not resolved against a base URL. Turning this into a
/// [`mini_web_types::CanonicalUrl`] the crawler can queue is deliberately
/// later work: relative-URL resolution is its own correctness-sensitive
/// surface and does not belong silently inside a text extractor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedLink {
    pub href: String,
    pub rel: Option<String>,
    pub anchor_text: String,
}

/// One `<meta name="..." content="...">` pair. `property` (Open Graph-style)
/// meta tags are not collected — only the plain `name`/`content` form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaEntry {
    pub name: String,
    pub content: String,
}

/// Everything this crate can honestly say about one already-fetched HTML
/// document. See the crate doc comment for what is deliberately not here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageExtract {
    pub title: Option<String>,
    pub headings: Vec<Heading>,
    pub visible_text: String,
    pub links: Vec<ExtractedLink>,
    /// From `<html lang="...">`, verbatim and unvalidated against any
    /// language-tag registry. `None` if the document declared none.
    pub language: Option<String>,
    pub meta: Vec<MetaEntry>,
    pub canonical_href: Option<String>,
    /// BLAKE3 digest of [`Self::visible_text`] — the content signal, not the
    /// raw HTML digest a crawler already records in `CrawlObservation`.
    /// Two documents with identical visible text but different markup
    /// share this digest; that is the point (near-duplicate detection at
    /// the markup level is explicitly not attempted here).
    pub content_digest: Multihash,
    pub script_tag_count: u32,
    pub iframe_tag_count: u32,
    /// Hosts parsed from `<script src="...">` values that were themselves
    /// absolute `http`/`https` URLs. Relative script sources (the common
    /// case) are not resolved and so are not classified as first- or
    /// third-party here — an honest gap, not a silent "local" assumption.
    pub external_script_hosts: Vec<String>,
    /// Bytes of text found inside elements this parser judged hidden
    /// (`hidden` attribute, or an inline `style` containing
    /// `display:none`/`visibility:hidden`) and therefore excluded from
    /// [`Self::visible_text`]. A large count relative to `visible_text`'s
    /// length is a cloaking/keyword-stuffing signal for a later ranker —
    /// this crate only measures it, it does not score or penalize it.
    pub hidden_text_byte_count: u64,
    /// True if any of this crate's output arrays were truncated against a
    /// bound in [`crate::limits`] because the document declared more items
    /// than the bound allows. Never true for `visible_text`, which is
    /// bounded by input size alone.
    pub truncated: bool,
}
