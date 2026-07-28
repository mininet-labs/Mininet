//! Bounds that keep one call to [`crate::extract`] doing bounded work over
//! bounded output, regardless of what an adversarial or merely huge page
//! contains. Exceeding a byte-length bound is a hard [`crate::ExtractError`];
//! exceeding a count bound truncates the affected array and sets
//! [`crate::PageExtract::truncated`] instead, since "the page had 50,000
//! links" is a fact worth reporting partially rather than refusing outright.

/// Hard ceiling on input size. The parser makes a single pass, so this
/// bounds total parse work.
pub const MAX_HTML_BYTES: usize = 8 * 1024 * 1024;

pub const MAX_LINKS: usize = 2_000;
pub const MAX_HEADINGS: usize = 500;
pub const MAX_META_ENTRIES: usize = 200;
pub const MAX_EXTERNAL_SCRIPT_HOSTS: usize = 200;

pub const MAX_VISIBLE_TEXT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_TITLE_BYTES: usize = 500;
pub const MAX_HEADING_TEXT_BYTES: usize = 2_000;
pub const MAX_ANCHOR_TEXT_BYTES: usize = 500;
