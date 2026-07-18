//! File-extension-based media-type detection, scoped to exactly what Track
//! B2 is allowed to accept: plain text and Markdown. Anything else is an
//! [`crate::IntakeCoordError::UnsupportedMediaType`] error, not a guess —
//! PDF/HTML/etc. extraction is Track B3/B4's job (research report §25),
//! and a wrong guess here would mislabel bytes Track B5 later trusts as
//! "this is what media_type claims."

use std::path::Path;

use mini_intake_types::MediaType;

use crate::error::{IntakeCoordError, Result};

/// Map a local file path's extension to a Track-B2-supported [`MediaType`].
/// A missing extension is treated as plain text (matches common `README`,
/// `LICENSE`-style extensionless text files); anything else unrecognized is
/// rejected rather than defaulted.
pub fn detect_media_type(path: &Path) -> Result<MediaType> {
    match path.extension().and_then(|ext| ext.to_str()) {
        None => Ok(MediaType::TextPlain),
        Some(ext) => match ext.to_ascii_lowercase().as_str() {
            "txt" => Ok(MediaType::TextPlain),
            "md" | "markdown" => Ok(MediaType::Markdown),
            _ => Err(IntakeCoordError::UnsupportedMediaType),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_txt_extension_is_plain_text() {
        assert_eq!(
            detect_media_type(Path::new("notes.txt")).unwrap(),
            MediaType::TextPlain
        );
    }

    #[test]
    fn md_and_markdown_extensions_are_markdown() {
        assert_eq!(
            detect_media_type(Path::new("README.md")).unwrap(),
            MediaType::Markdown
        );
        assert_eq!(
            detect_media_type(Path::new("NOTES.MARKDOWN")).unwrap(),
            MediaType::Markdown
        );
    }

    #[test]
    fn an_extensionless_file_is_plain_text() {
        assert_eq!(
            detect_media_type(Path::new("LICENSE")).unwrap(),
            MediaType::TextPlain
        );
    }

    #[test]
    fn an_unrecognized_extension_is_rejected() {
        assert!(matches!(
            detect_media_type(Path::new("scan.pdf")),
            Err(IntakeCoordError::UnsupportedMediaType)
        ));
        assert!(matches!(
            detect_media_type(Path::new("photo.PNG")),
            Err(IntakeCoordError::UnsupportedMediaType)
        ));
    }
}
