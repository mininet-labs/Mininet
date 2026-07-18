//! [`MediaType`]: the closed-but-extensible set of formats Mininet
//! Intake's Phase A/B/C extractors are scoped to recognize (`docs/
//! research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_
//! 20260718.md` §5). `#[non_exhaustive]` so later phases (audio/video/
//! web-capture formats) can add variants without a breaking change.

use crate::codec::{Reader, Writer};
use crate::error::{IntakeError, Result};

const MAX_OTHER_MEDIA_TYPE_BYTES: usize = 255;

/// A source's declared or sniffed media type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MediaType {
    /// Phase A.
    TextPlain,
    Markdown,
    Json,
    Png,
    Jpeg,
    Pdf,
    /// Phase B.
    Html,
    GitPatch,
    /// An IANA media type this crate does not yet name a variant for.
    /// Bounded to keep decode allocation cost fixed regardless of caller
    /// input.
    Other(String),
}

impl MediaType {
    fn tag(&self) -> u8 {
        match self {
            MediaType::TextPlain => 1,
            MediaType::Markdown => 2,
            MediaType::Json => 3,
            MediaType::Png => 4,
            MediaType::Jpeg => 5,
            MediaType::Pdf => 6,
            MediaType::Html => 7,
            MediaType::GitPatch => 8,
            MediaType::Other(_) => 255,
        }
    }

    pub(crate) fn encode(&self, w: &mut Writer) {
        w.u8(self.tag());
        if let MediaType::Other(s) = self {
            w.str(s);
        }
    }

    pub(crate) fn decode(r: &mut Reader) -> Result<Self> {
        match r.u8()? {
            1 => Ok(MediaType::TextPlain),
            2 => Ok(MediaType::Markdown),
            3 => Ok(MediaType::Json),
            4 => Ok(MediaType::Png),
            5 => Ok(MediaType::Jpeg),
            6 => Ok(MediaType::Pdf),
            7 => Ok(MediaType::Html),
            8 => Ok(MediaType::GitPatch),
            255 => Ok(MediaType::Other(r.str_limited(MAX_OTHER_MEDIA_TYPE_BYTES)?)),
            _ => Err(IntakeError::BadMediaType),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(m: MediaType) {
        let mut w = Writer::new();
        m.encode(&mut w);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        let decoded = MediaType::decode(&mut r).unwrap();
        assert!(r.finished());
        assert_eq!(decoded, m);
    }

    #[test]
    fn every_named_variant_round_trips() {
        round_trip(MediaType::TextPlain);
        round_trip(MediaType::Markdown);
        round_trip(MediaType::Json);
        round_trip(MediaType::Png);
        round_trip(MediaType::Jpeg);
        round_trip(MediaType::Pdf);
        round_trip(MediaType::Html);
        round_trip(MediaType::GitPatch);
        round_trip(MediaType::Other("application/x-custom".to_string()));
    }

    #[test]
    fn an_unrecognized_tag_is_rejected() {
        let mut r = Reader::new(&[200u8]);
        assert_eq!(MediaType::decode(&mut r), Err(IntakeError::BadMediaType));
    }

    #[test]
    fn an_oversized_other_string_is_rejected_before_allocating() {
        let mut w = Writer::new();
        w.u8(255);
        w.u32((MAX_OTHER_MEDIA_TYPE_BYTES + 1) as u32);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        assert_eq!(MediaType::decode(&mut r), Err(IntakeError::LimitExceeded));
    }
}
