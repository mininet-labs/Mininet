//! The built-in extractors this crate ships. Runs inside the worker
//! process only -- never called by the host directly, since the whole
//! point of `mini-extract-host` is that this code executes in a separate
//! address space from whatever called `mini-extract-worker`.

use mini_extract_protocol::{ExtractionError, ExtractorKind};

/// Run `kind` over `source_bytes`, enforcing `max_output_bytes` on the
/// result before returning it.
pub fn run(
    kind: ExtractorKind,
    source_bytes: &[u8],
    max_output_bytes: u32,
) -> Result<Vec<u8>, ExtractionError> {
    let extracted = match kind {
        ExtractorKind::PlainTextNormalize => plain_text_normalize(source_bytes),
        // `ExtractorKind` is `#[non_exhaustive]`: a future Track B4 kind
        // this worker binary predates must fail cleanly and specifically
        // (`UnsupportedExtractorKind`), never silently fall through to
        // one of today's extractors and run the wrong logic over the
        // bytes.
        _ => return Err(ExtractionError::UnsupportedExtractorKind),
    };
    let declared = u32::try_from(extracted.len()).unwrap_or(u32::MAX);
    if declared > max_output_bytes {
        return Err(ExtractionError::OutputTooLarge {
            declared,
            max: max_output_bytes,
        });
    }
    Ok(extracted)
}

/// Decode `source_bytes` as UTF-8 (lossy replacement for invalid
/// sequences -- this extractor never rejects input as malformed, since
/// "best-effort readable text from arbitrary bytes" is exactly its job),
/// strip control characters other than tab (`\t`) and newline (`\n`),
/// and collapse runs of horizontal whitespace to a single space.
/// Deliberately trivial: proving the isolation host works end-to-end
/// before Track B4's real, much higher-risk PDF/HTML parsers are wired
/// in, not a general-purpose text-cleaning tool.
fn plain_text_normalize(source_bytes: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(source_bytes);
    let mut out = String::with_capacity(text.len());
    let mut last_was_space = false;
    for c in text.chars() {
        if c == '\n' {
            out.push('\n');
            last_was_space = false;
            continue;
        }
        if c == '\t' || c.is_whitespace() {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
            continue;
        }
        if c.is_control() {
            continue;
        }
        out.push(c);
        last_was_space = false;
    }
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_normalize_collapses_whitespace_and_preserves_newlines() {
        let input = b"hello   world\t\tagain\nsecond   line";
        let out = plain_text_normalize(input);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "hello world again\nsecond line"
        );
    }

    #[test]
    fn plain_text_normalize_strips_control_characters() {
        let input = b"a\x01\x02b\x7f c";
        let out = plain_text_normalize(input);
        assert_eq!(String::from_utf8(out).unwrap(), "ab c");
    }

    #[test]
    fn plain_text_normalize_lossy_decodes_invalid_utf8_instead_of_failing() {
        let input = [b'a', 0xff, 0xfe, b'b'];
        let out = plain_text_normalize(&input);
        // Never panics or errors; invalid bytes become U+FFFD, which is
        // not a control character, so it survives into the output.
        assert!(String::from_utf8(out).is_ok());
    }

    #[test]
    fn run_rejects_output_over_the_declared_limit() {
        let source = b"hello world, this is a fairly long line of text";
        let err = run(ExtractorKind::PlainTextNormalize, source, 5).unwrap_err();
        assert!(matches!(
            err,
            ExtractionError::OutputTooLarge { max: 5, .. }
        ));
    }

    #[test]
    fn run_succeeds_when_output_fits_the_limit() {
        let source = b"short";
        let out = run(ExtractorKind::PlainTextNormalize, source, 100).unwrap();
        assert_eq!(out, b"short");
    }
}
