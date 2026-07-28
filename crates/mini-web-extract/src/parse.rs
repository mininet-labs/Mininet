use mini_crypto::{HashAlgorithm, Multihash};

use crate::error::{ExtractError, Result};
use crate::limits::*;
use crate::types::{ExtractedLink, Heading, HeadingLevel, MetaEntry, PageExtract};

/// Elements whose content is never markup and is never counted as visible
/// text: `<script>`/`<style>` for the obvious reason (this crate never
/// executes or evaluates either), `<noscript>`/`<template>` because their
/// content is inert fallback/inert-template markup under a static,
/// non-scripting reading of the page rather than genuinely rendered text.
const RAW_TEXT_ELEMENTS: &[&str] = &["script", "style", "noscript", "template"];

/// Elements with no end tag and no content, handled entirely from their
/// start tag.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

enum FrameKind {
    Plain,
    Title(String),
    Heading(HeadingLevel, String),
    Anchor {
        href: Option<String>,
        rel: Option<String>,
        text: String,
    },
}

struct StackFrame {
    name: String,
    hidden_here: bool,
    kind: FrameKind,
}

pub(crate) struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    stack: Vec<StackFrame>,
    hidden_depth: u32,

    title: Option<String>,
    headings: Vec<Heading>,
    visible_text: String,
    links: Vec<ExtractedLink>,
    language: Option<String>,
    meta: Vec<MetaEntry>,
    canonical_href: Option<String>,
    script_tag_count: u32,
    iframe_tag_count: u32,
    external_script_hosts: Vec<String>,
    hidden_text_byte_count: u64,
    truncated: bool,
}

pub(crate) fn extract(html: &str) -> Result<PageExtract> {
    if html.len() > MAX_HTML_BYTES {
        return Err(ExtractError::InputTooLarge {
            byte_length: html.len(),
        });
    }
    let mut parser = Parser {
        bytes: html.as_bytes(),
        pos: 0,
        stack: Vec::new(),
        hidden_depth: 0,
        title: None,
        headings: Vec::new(),
        visible_text: String::new(),
        links: Vec::new(),
        language: None,
        meta: Vec::new(),
        canonical_href: None,
        script_tag_count: 0,
        iframe_tag_count: 0,
        external_script_hosts: Vec::new(),
        hidden_text_byte_count: 0,
        truncated: false,
    };
    parser.run();
    Ok(parser.finish())
}

impl<'a> Parser<'a> {
    fn run(&mut self) {
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b'<' => self.handle_markup(),
                _ => self.handle_text_run(),
            }
        }
        // Force-close whatever is still open at EOF so a truncated document
        // still yields the title/headings/links it did contain.
        while let Some(frame) = self.stack.pop() {
            self.finish_frame(frame);
        }
    }

    fn finish(mut self) -> PageExtract {
        let content_digest = Multihash::of(HashAlgorithm::Blake3, self.visible_text.as_bytes());
        self.visible_text.shrink_to_fit();
        PageExtract {
            title: self.title,
            headings: self.headings,
            visible_text: self.visible_text,
            links: self.links,
            language: self.language,
            meta: self.meta,
            canonical_href: self.canonical_href,
            content_digest,
            script_tag_count: self.script_tag_count,
            iframe_tag_count: self.iframe_tag_count,
            external_script_hosts: self.external_script_hosts,
            hidden_text_byte_count: self.hidden_text_byte_count,
            truncated: self.truncated,
        }
    }

    // -- text -----------------------------------------------------------

    fn handle_text_run(&mut self) {
        let start = self.pos;
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'<' {
            self.pos += 1;
        }
        let raw = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or("");
        self.handle_text(raw);
    }

    fn handle_text(&mut self, raw: &str) {
        let decoded = decode_entities(raw);
        let collapsed = collapse_whitespace(&decoded);
        if collapsed.is_empty() {
            return;
        }
        if self.hidden_depth > 0 {
            self.hidden_text_byte_count += collapsed.len() as u64;
            return;
        }
        let truncated = append_joined(&mut self.visible_text, &collapsed, MAX_VISIBLE_TEXT_BYTES);
        self.truncated |= truncated;
        for frame in self.stack.iter_mut() {
            match &mut frame.kind {
                FrameKind::Title(buf) => {
                    self.truncated |= append_joined(buf, &collapsed, MAX_TITLE_BYTES);
                }
                FrameKind::Heading(_, buf) => {
                    self.truncated |= append_joined(buf, &collapsed, MAX_HEADING_TEXT_BYTES);
                }
                FrameKind::Anchor { text, .. } => {
                    self.truncated |= append_joined(text, &collapsed, MAX_ANCHOR_TEXT_BYTES);
                }
                FrameKind::Plain => {}
            }
        }
    }

    // -- markup dispatch --------------------------------------------------

    fn handle_markup(&mut self) {
        debug_assert_eq!(self.bytes[self.pos], b'<');
        let rest = &self.bytes[self.pos..];
        if rest.starts_with(b"<!--") {
            self.skip_comment();
        } else if rest.starts_with(b"</") {
            self.handle_end_tag();
        } else if rest.starts_with(b"<!") || rest.starts_with(b"<?") {
            self.skip_to_gt();
        } else if rest.len() > 1 && is_name_start(rest[1]) {
            self.handle_start_tag();
        } else {
            // Stray '<' (e.g. "a < b" without a real tag). Treat as text.
            self.pos += 1;
            self.handle_text("<");
        }
    }

    fn skip_comment(&mut self) {
        self.pos += 4; // "<!--"
        if let Some(end) = find_subslice(&self.bytes[self.pos..], b"-->") {
            self.pos += end + 3;
        } else {
            self.pos = self.bytes.len();
        }
    }

    fn skip_to_gt(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'>' {
            self.pos += 1;
        }
        if self.pos < self.bytes.len() {
            self.pos += 1; // consume '>'
        }
    }

    // -- tags ---------------------------------------------------------------

    fn handle_start_tag(&mut self) {
        self.pos += 1; // consume '<'
        let name = self.read_tag_name();
        let attrs = self.read_attributes();
        let mut self_closing = false;
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'/' {
            self_closing = true;
            self.pos += 1;
        }
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'>' {
            self.pos += 1;
        }

        let name_lower = name.to_ascii_lowercase();
        self.apply_semantics(&name_lower, &attrs);

        let is_void = VOID_ELEMENTS.contains(&name_lower.as_str());
        if is_void || self_closing {
            return;
        }

        if RAW_TEXT_ELEMENTS.contains(&name_lower.as_str()) {
            self.skip_raw_text_element(&name_lower);
            return;
        }

        let hidden_here = is_hidden_element(&attrs);
        if hidden_here {
            self.hidden_depth += 1;
        }

        let kind = match name_lower.as_str() {
            "title" if self.title.is_none() => FrameKind::Title(String::new()),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = HeadingLevel::from_tag(&name_lower).expect("matched above");
                FrameKind::Heading(level, String::new())
            }
            "a" => FrameKind::Anchor {
                href: get_attr(&attrs, "href"),
                rel: get_attr(&attrs, "rel"),
                text: String::new(),
            },
            _ => FrameKind::Plain,
        };
        self.stack.push(StackFrame {
            name: name_lower,
            hidden_here,
            kind,
        });
    }

    fn apply_semantics(&mut self, name_lower: &str, attrs: &[(String, String)]) {
        match name_lower {
            "html" => {
                if self.language.is_none() {
                    self.language = get_attr(attrs, "lang").filter(|s| !s.is_empty());
                }
            }
            "meta" => {
                if let (Some(name), Some(content)) =
                    (get_attr(attrs, "name"), get_attr(attrs, "content"))
                {
                    if self.meta.len() < MAX_META_ENTRIES {
                        self.meta.push(MetaEntry { name, content });
                    } else {
                        self.truncated = true;
                    }
                }
            }
            "link" => {
                let is_canonical = get_attr(attrs, "rel")
                    .map(|r| r.eq_ignore_ascii_case("canonical"))
                    .unwrap_or(false);
                if is_canonical && self.canonical_href.is_none() {
                    self.canonical_href = get_attr(attrs, "href");
                }
            }
            "script" => {
                self.script_tag_count += 1;
                if let Some(src) = get_attr(attrs, "src") {
                    if let Some(host) = absolute_http_host(&src) {
                        if !self.external_script_hosts.contains(&host) {
                            if self.external_script_hosts.len() < MAX_EXTERNAL_SCRIPT_HOSTS {
                                self.external_script_hosts.push(host);
                            } else {
                                self.truncated = true;
                            }
                        }
                    }
                }
            }
            "iframe" => {
                self.iframe_tag_count += 1;
            }
            _ => {}
        }
    }

    fn skip_raw_text_element(&mut self, name_lower: &str) {
        let needle_open = format!("</{name_lower}");
        match find_subslice_ci(&self.bytes[self.pos..], needle_open.as_bytes()) {
            Some(offset) => {
                self.pos += offset;
                // Consume through the '>' that closes this end tag.
                self.skip_to_gt();
            }
            None => {
                self.pos = self.bytes.len();
            }
        }
    }

    fn handle_end_tag(&mut self) {
        self.pos += 2; // "</"
        let name = self.read_tag_name();
        self.skip_to_gt();
        let name_lower = name.to_ascii_lowercase();
        if name_lower.is_empty() {
            return;
        }
        if let Some(idx) = self.stack.iter().rposition(|f| f.name == name_lower) {
            while self.stack.len() > idx {
                let frame = self.stack.pop().expect("len checked above");
                self.finish_frame(frame);
            }
        }
        // No matching open frame: stray end tag, ignored.
    }

    fn finish_frame(&mut self, frame: StackFrame) {
        if frame.hidden_here {
            self.hidden_depth = self.hidden_depth.saturating_sub(1);
        }
        match frame.kind {
            FrameKind::Title(buf) => {
                if self.title.is_none() {
                    let trimmed = buf.trim().to_string();
                    if !trimmed.is_empty() {
                        self.title = Some(trimmed);
                    }
                }
            }
            FrameKind::Heading(level, buf) => {
                let trimmed = buf.trim().to_string();
                if self.headings.len() < MAX_HEADINGS {
                    self.headings.push(Heading {
                        level,
                        text: trimmed,
                    });
                } else {
                    self.truncated = true;
                }
            }
            FrameKind::Anchor { href, rel, text } => {
                if let Some(href) = href {
                    if self.links.len() < MAX_LINKS {
                        self.links.push(ExtractedLink {
                            href,
                            rel,
                            anchor_text: text.trim().to_string(),
                        });
                    } else {
                        self.truncated = true;
                    }
                }
            }
            FrameKind::Plain => {}
        }
    }

    // -- lexing helpers ---------------------------------------------------

    fn read_tag_name(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.bytes.len() && is_name_byte(self.bytes[self.pos]) {
            self.pos += 1;
        }
        std::str::from_utf8(&self.bytes[start..self.pos])
            .unwrap_or("")
            .to_string()
    }

    fn read_attributes(&mut self) -> Vec<(String, String)> {
        let mut attrs = Vec::new();
        loop {
            self.skip_ws();
            if self.pos >= self.bytes.len() {
                break;
            }
            let b = self.bytes[self.pos];
            if b == b'>' || b == b'/' {
                break;
            }
            if !is_name_start(b) {
                // Unexpected byte where an attribute name should be; skip it
                // so a single stray character can't stall the parser.
                self.pos += 1;
                continue;
            }
            let name_start = self.pos;
            while self.pos < self.bytes.len() && is_attr_name_byte(self.bytes[self.pos]) {
                self.pos += 1;
            }
            let name = std::str::from_utf8(&self.bytes[name_start..self.pos])
                .unwrap_or("")
                .to_ascii_lowercase();
            self.skip_ws();
            let value = if self.pos < self.bytes.len() && self.bytes[self.pos] == b'=' {
                self.pos += 1;
                self.skip_ws();
                self.read_attr_value()
            } else {
                String::new()
            };
            if !name.is_empty() {
                attrs.push((name, value));
            }
        }
        attrs
    }

    fn read_attr_value(&mut self) -> String {
        if self.pos >= self.bytes.len() {
            return String::new();
        }
        let quote = self.bytes[self.pos];
        if quote == b'"' || quote == b'\'' {
            self.pos += 1;
            let start = self.pos;
            while self.pos < self.bytes.len() && self.bytes[self.pos] != quote {
                self.pos += 1;
            }
            let raw = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or("");
            let value = decode_entities(raw);
            if self.pos < self.bytes.len() {
                self.pos += 1; // consume closing quote
            }
            value
        } else {
            let start = self.pos;
            while self.pos < self.bytes.len()
                && !self.bytes[self.pos].is_ascii_whitespace()
                && self.bytes[self.pos] != b'>'
            {
                self.pos += 1;
            }
            let raw = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or("");
            decode_entities(raw)
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }
}

// -- free functions -----------------------------------------------------

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic()
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b':'
}

fn is_attr_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b':'
}

fn get_attr(attrs: &[(String, String)], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
        .filter(|v| !v.is_empty())
}

/// A `hidden` boolean attribute, or an inline `style` declaring
/// `display:none`/`visibility:hidden`. Whitespace- and case-insensitive;
/// not a CSS parser, so a `display:none` reached only through an external
/// or `<style>` stylesheet is not detected — an honest, documented gap.
fn is_hidden_element(attrs: &[(String, String)]) -> bool {
    if attrs.iter().any(|(k, _)| k == "hidden") {
        return true;
    }
    if let Some(style) = attrs.iter().find(|(k, _)| k == "style").map(|(_, v)| v) {
        let normalized: String = style
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .to_ascii_lowercase();
        if normalized.contains("display:none") || normalized.contains("visibility:hidden") {
            return true;
        }
    }
    false
}

/// Parses `scheme://host[...]` and returns the lower-cased host, or `None`
/// for relative sources or anything not shaped like an absolute HTTP(S) URL.
fn absolute_http_host(src: &str) -> Option<String> {
    let rest = src
        .strip_prefix("https://")
        .or_else(|| src.strip_prefix("http://"))?;
    let host_end = rest.find(['/', '?', '#', ':']).unwrap_or(rest.len());
    let host = &rest[..host_end];
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len().max(1))
        .position(|w| w == needle)
}

fn find_subslice_ci(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle))
}

/// Collapses runs of ASCII whitespace to a single space and trims the ends.
/// Not full Unicode whitespace normalization — matches this crate's "static
/// HTML, not full-spec rendering" scope.
fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_space = true; // true so leading whitespace is dropped
    for c in input.chars() {
        if c.is_ascii_whitespace() {
            if !last_was_space {
                out.push(' ');
            }
            last_was_space = true;
        } else {
            out.push(c);
            last_was_space = false;
        }
    }
    while out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Appends `chunk` to `buf`, inserting a joining space when both sides are
/// non-empty so text from adjacent elements never glues together word-to-
/// word. Returns `true` if `max_bytes` was hit and the chunk was truncated
/// (or dropped).
fn append_joined(buf: &mut String, chunk: &str, max_bytes: usize) -> bool {
    if buf.len() >= max_bytes {
        return true;
    }
    if !buf.is_empty() && !chunk.is_empty() {
        buf.push(' ');
    }
    let remaining = max_bytes.saturating_sub(buf.len());
    if chunk.len() <= remaining {
        buf.push_str(chunk);
        false
    } else {
        let mut cut = remaining;
        while cut > 0 && !chunk.is_char_boundary(cut) {
            cut -= 1;
        }
        buf.push_str(&chunk[..cut]);
        true
    }
}

const NAMED_ENTITIES: &[(&str, &str)] = &[
    ("amp", "&"),
    ("lt", "<"),
    ("gt", ">"),
    ("quot", "\""),
    ("apos", "'"),
    ("nbsp", " "),
];

/// Decodes `&amp;`-style named entities and `&#NNN;`/`&#xHH;` numeric
/// references. Anything else beginning with `&` (including a malformed or
/// unterminated reference) is left exactly as written.
fn decode_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let mut chars = input.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c != '&' {
            out.push(c);
            continue;
        }
        // Look ahead at most a short, bounded window for a well-formed
        // reference; anything longer is not a real entity in practice.
        let window_end = (i + 12).min(input.len());
        let window = &input[i..window_end];
        if let Some((decoded, consumed)) = decode_one_entity(window) {
            out.push_str(&decoded);
            for _ in 1..consumed {
                chars.next();
            }
            continue;
        }
        out.push('&');
    }
    out
}

fn decode_one_entity(window: &str) -> Option<(String, usize)> {
    debug_assert!(window.starts_with('&'));
    let semi = window.find(';')?;
    let body = &window[1..semi];
    if let Some(hex) = body.strip_prefix("#x").or_else(|| body.strip_prefix("#X")) {
        let code = u32::from_str_radix(hex, 16).ok()?;
        let ch = char::from_u32(code).unwrap_or('\u{FFFD}');
        return Some((ch.to_string(), semi + 1));
    }
    if let Some(dec) = body.strip_prefix('#') {
        let code: u32 = dec.parse().ok()?;
        let ch = char::from_u32(code).unwrap_or('\u{FFFD}');
        return Some((ch.to_string(), semi + 1));
    }
    for (name, value) in NAMED_ENTITIES {
        if body == *name {
            return Some((value.to_string(), semi + 1));
        }
    }
    None
}
