//! What the isolated worker sends back: either extracted bytes, or a
//! specific, structured reason extraction did not succeed. Both are
//! legitimate protocol outcomes -- [`ExtractionError`] is not this
//! crate's [`crate::ProtocolError`], which covers the wire framing
//! itself breaking, not the worker cleanly reporting "this input could
//! not be extracted."

use core::fmt;

use crate::codec::{put_bytes, put_str, put_u32, put_u8, take_bytes, take_str, take_u32, take_u8};
use crate::error::{ProtocolError, Result};

/// Upper bound on `ExtractionSuccess::extracted_bytes`. Independent of
/// [`crate::request::MAX_SOURCE_BYTES`] -- extraction can plausibly
/// shrink (whitespace normalization) or, for a future format, expand
/// (e.g. embedded-image OCR text) the byte count relative to the source,
/// so this crate does not assume one bounds the other.
pub const MAX_EXTRACTED_BYTES: usize = 64 * 1024 * 1024;

/// Why a worker did not (or could not) produce extracted text.
/// `#[non_exhaustive]` so later extractors (Track B4) can report new
/// failure shapes without a breaking change to this crate.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExtractionError {
    /// The host killed the worker after [`crate::ResourceLimits::
    /// max_wall_clock_ms`] elapsed with no result frame.
    Timeout,
    /// The worker's own declared output length exceeded [`crate::
    /// ResourceLimits::max_output_bytes`] -- the host refuses to read
    /// the payload at all once the declared length is known, the same
    /// discipline [`crate::frame::read_framed`] already applies to the
    /// outer wire frame.
    OutputTooLarge { declared: u32, max: u32 },
    /// The worker decoded the request but could not make sense of
    /// `source_bytes` for the requested [`crate::ExtractorKind`].
    MalformedInput,
    /// The worker process exited (crashed, panicked, was killed by a
    /// signal) before writing a complete result frame.
    ExtractorCrashed { exit_code: Option<i32> },
    /// The host could not even spawn the worker, or a pipe to it failed
    /// outside of the normal protocol framing.
    Io(String),
    /// The worker binary running does not implement the requested
    /// [`crate::ExtractorKind`] (e.g. an older worker binary receiving a
    /// request naming a kind a later Track B4 crate added).
    UnsupportedExtractorKind,
}

impl fmt::Display for ExtractionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtractionError::Timeout => write!(f, "extraction timed out"),
            ExtractionError::OutputTooLarge { declared, max } => write!(
                f,
                "extracted output declares {declared} bytes, over the {max}-byte limit"
            ),
            ExtractionError::MalformedInput => write!(f, "input could not be extracted"),
            ExtractionError::ExtractorCrashed { exit_code } => {
                write!(
                    f,
                    "extractor process exited unexpectedly (code {exit_code:?})"
                )
            }
            ExtractionError::Io(e) => write!(f, "extraction I/O error: {e}"),
            ExtractionError::UnsupportedExtractorKind => {
                write!(f, "worker does not support the requested extractor kind")
            }
        }
    }
}

impl std::error::Error for ExtractionError {}

fn tag(e: &ExtractionError) -> u8 {
    match e {
        ExtractionError::Timeout => 1,
        ExtractionError::OutputTooLarge { .. } => 2,
        ExtractionError::MalformedInput => 3,
        ExtractionError::ExtractorCrashed { .. } => 4,
        ExtractionError::Io(_) => 5,
        ExtractionError::UnsupportedExtractorKind => 6,
    }
}

fn encode_error(w: &mut Vec<u8>, e: &ExtractionError) {
    put_u8(w, tag(e));
    match e {
        ExtractionError::Timeout | ExtractionError::MalformedInput => {}
        ExtractionError::OutputTooLarge { declared, max } => {
            put_u32(w, *declared);
            put_u32(w, *max);
        }
        ExtractionError::ExtractorCrashed { exit_code } => match exit_code {
            None => put_u8(w, 0),
            Some(code) => {
                put_u8(w, 1);
                put_u32(w, *code as u32);
            }
        },
        ExtractionError::Io(msg) => put_str(w, msg),
        ExtractionError::UnsupportedExtractorKind => {}
    }
}

fn decode_error(b: &[u8], off: &mut usize) -> Result<ExtractionError> {
    match take_u8(b, off)? {
        1 => Ok(ExtractionError::Timeout),
        2 => {
            let declared = take_u32(b, off)?;
            let max = take_u32(b, off)?;
            Ok(ExtractionError::OutputTooLarge { declared, max })
        }
        3 => Ok(ExtractionError::MalformedInput),
        4 => {
            let exit_code = match take_u8(b, off)? {
                0 => None,
                1 => Some(take_u32(b, off)? as i32),
                _ => return Err(ProtocolError::BadMessage),
            };
            Ok(ExtractionError::ExtractorCrashed { exit_code })
        }
        5 => Ok(ExtractionError::Io(take_str(b, off)?)),
        6 => Ok(ExtractionError::UnsupportedExtractorKind),
        _ => Err(ProtocolError::BadMessage),
    }
}

/// Extracted output, on success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionSuccess {
    pub extracted_bytes: Vec<u8>,
    pub wall_clock_ms: u32,
}

/// The one message a worker ever sends back for a request: success or a
/// structured, specific failure. Never a generic "it failed" -- every
/// variant of [`ExtractionError`] names exactly what went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractionOutcome {
    Ok(ExtractionSuccess),
    Err(ExtractionError),
}

impl ExtractionOutcome {
    /// Encode to the canonical wire form.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Vec::new();
        match self {
            ExtractionOutcome::Ok(success) => {
                put_u8(&mut w, 0);
                put_bytes(&mut w, &success.extracted_bytes);
                put_u32(&mut w, success.wall_clock_ms);
            }
            ExtractionOutcome::Err(e) => {
                put_u8(&mut w, 1);
                encode_error(&mut w, e);
            }
        }
        w
    }

    /// Decode from [`Self::encode`]'s wire form. Strict: rejects trailing
    /// bytes and an over-bound `extracted_bytes` length.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let mut off = 0usize;
        let outcome = match take_u8(b, &mut off)? {
            0 => {
                let extracted_bytes = take_bytes(b, &mut off, MAX_EXTRACTED_BYTES)?;
                let wall_clock_ms = take_u32(b, &mut off)?;
                ExtractionOutcome::Ok(ExtractionSuccess {
                    extracted_bytes,
                    wall_clock_ms,
                })
            }
            1 => ExtractionOutcome::Err(decode_error(b, &mut off)?),
            _ => return Err(ProtocolError::BadMessage),
        };
        if off != b.len() {
            return Err(ProtocolError::BadMessage);
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_success_round_trips() {
        let outcome = ExtractionOutcome::Ok(ExtractionSuccess {
            extracted_bytes: b"hello".to_vec(),
            wall_clock_ms: 42,
        });
        assert_eq!(
            ExtractionOutcome::decode(&outcome.encode()).unwrap(),
            outcome
        );
    }

    #[test]
    fn every_error_variant_round_trips() {
        for e in [
            ExtractionError::Timeout,
            ExtractionError::OutputTooLarge {
                declared: 100,
                max: 50,
            },
            ExtractionError::MalformedInput,
            ExtractionError::ExtractorCrashed { exit_code: None },
            ExtractionError::ExtractorCrashed {
                exit_code: Some(-1),
            },
            ExtractionError::Io("spawn failed".to_string()),
            ExtractionError::UnsupportedExtractorKind,
        ] {
            let outcome = ExtractionOutcome::Err(e.clone());
            assert_eq!(
                ExtractionOutcome::decode(&outcome.encode()).unwrap(),
                outcome,
                "failed round-trip for {e:?}"
            );
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let outcome = ExtractionOutcome::Err(ExtractionError::Timeout);
        let mut bytes = outcome.encode();
        bytes.push(0xff);
        assert!(ExtractionOutcome::decode(&bytes).is_err());
    }

    #[test]
    fn truncated_bytes_are_rejected() {
        let outcome = ExtractionOutcome::Ok(ExtractionSuccess {
            extracted_bytes: b"hello world".to_vec(),
            wall_clock_ms: 1,
        });
        let mut bytes = outcome.encode();
        bytes.truncate(bytes.len() - 2);
        assert!(ExtractionOutcome::decode(&bytes).is_err());
    }

    #[test]
    fn an_unrecognized_outcome_tag_is_rejected() {
        let bytes = vec![200u8];
        assert_eq!(
            ExtractionOutcome::decode(&bytes),
            Err(ProtocolError::BadMessage)
        );
    }

    #[test]
    fn an_unrecognized_error_tag_is_rejected() {
        let bytes = vec![1u8, 200u8];
        assert_eq!(
            ExtractionOutcome::decode(&bytes),
            Err(ProtocolError::BadMessage)
        );
    }

    #[test]
    fn a_declared_extracted_length_over_the_bound_is_refused_before_allocating() {
        let mut bytes = Vec::new();
        put_u8(&mut bytes, 0);
        put_u32(&mut bytes, u32::MAX);
        assert_eq!(
            ExtractionOutcome::decode(&bytes),
            Err(ProtocolError::BadMessage)
        );
    }
}
