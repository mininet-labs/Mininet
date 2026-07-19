//! What the coordinator sends the isolated extractor worker: exactly
//! which built-in extractor to run, the raw source bytes to run it over,
//! and the resource limits the host enforces around the child process.
//! The worker never reads anything from the filesystem, the network, or
//! `mini-store` itself -- every byte it can see arrives in this message.

use crate::codec::{put_bytes, put_u32, put_u8, take_bytes, take_u32, take_u8};
use crate::error::{ProtocolError, Result};

/// Upper bound on `ExtractionRequest::source_bytes` -- refused before
/// allocating, the same discipline every decoder in this tree applies to
/// attacker-controlled length prefixes. 64 MiB is generous for the "one
/// simple extractor" this crate ships (Track B4's real PDF/HTML backends
/// may need their own, separately-reviewed bound).
pub const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;

/// The built-in extractor to run. `#[non_exhaustive]` so Track B4 (PDF,
/// HTML) can add variants without a breaking change to this crate.
/// Mapping a `mini_intake_types::MediaType` to an `ExtractorKind` is
/// deliberately not this crate's job -- that belongs to whatever later
/// integration PR wires `mini-intake`'s coordinator to this host, keeping
/// this protocol crate free of a dependency edge to `mini-intake-types`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExtractorKind {
    /// Decode `source_bytes` as UTF-8 (lossy), strip control characters
    /// other than tab/newline, and collapse runs of whitespace. The one
    /// simple extractor Track B3 ships to prove the isolation host works
    /// end-to-end before Track B4's real, much higher-risk PDF/HTML
    /// parsers are wired in.
    PlainTextNormalize,
}

impl ExtractorKind {
    fn tag(&self) -> u8 {
        match self {
            ExtractorKind::PlainTextNormalize => 1,
        }
    }

    fn decode_tag(tag: u8) -> Result<Self> {
        match tag {
            1 => Ok(ExtractorKind::PlainTextNormalize),
            _ => Err(ProtocolError::BadMessage),
        }
    }
}

/// Resource limits the isolated host enforces around the worker child
/// process. Distinct from `mini_pipeline::ResourceLimits` (that type's
/// `max_fuel`/`max_memory_bytes` are Wasmtime-specific concepts this
/// crate's process-boundary isolation has no equivalent for) -- this is a
/// smaller, OS-process-shaped set: a wall-clock kill timer and an
/// output-size cap the host enforces by watching the byte count as it
/// reads the worker's response frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    /// Kill the worker process if it has not produced a complete result
    /// frame within this many milliseconds.
    pub max_wall_clock_ms: u32,
    /// Reject (and kill the worker) if its result frame declares more
    /// than this many bytes of extracted output.
    pub max_output_bytes: u32,
}

impl ResourceLimits {
    /// A conservative default suitable for the plain-text extractor:
    /// five seconds, eight mebibytes of output.
    pub fn conservative_default() -> Self {
        ResourceLimits {
            max_wall_clock_ms: 5_000,
            max_output_bytes: 8 * 1024 * 1024,
        }
    }
}

fn encode_limits(w: &mut Vec<u8>, limits: &ResourceLimits) {
    put_u32(w, limits.max_wall_clock_ms);
    put_u32(w, limits.max_output_bytes);
}

fn decode_limits(b: &[u8], off: &mut usize) -> Result<ResourceLimits> {
    let max_wall_clock_ms = take_u32(b, off)?;
    let max_output_bytes = take_u32(b, off)?;
    Ok(ResourceLimits {
        max_wall_clock_ms,
        max_output_bytes,
    })
}

/// One request to extract text from `source_bytes` using `kind`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionRequest {
    pub kind: ExtractorKind,
    pub source_bytes: Vec<u8>,
    pub limits: ResourceLimits,
}

impl ExtractionRequest {
    /// Encode to the canonical wire form.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Vec::new();
        put_u8(&mut w, self.kind.tag());
        put_bytes(&mut w, &self.source_bytes);
        encode_limits(&mut w, &self.limits);
        w
    }

    /// Decode from [`Self::encode`]'s wire form. Strict: rejects trailing
    /// bytes and an over-bound `source_bytes` length.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let mut off = 0usize;
        let kind = ExtractorKind::decode_tag(take_u8(b, &mut off)?)?;
        let source_bytes = take_bytes(b, &mut off, MAX_SOURCE_BYTES)?;
        let limits = decode_limits(b, &mut off)?;
        if off != b.len() {
            return Err(ProtocolError::BadMessage);
        }
        Ok(ExtractionRequest {
            kind,
            source_bytes,
            limits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_request() -> ExtractionRequest {
        ExtractionRequest {
            kind: ExtractorKind::PlainTextNormalize,
            source_bytes: b"hello world".to_vec(),
            limits: ResourceLimits::conservative_default(),
        }
    }

    #[test]
    fn round_trips() {
        let req = a_request();
        let decoded = ExtractionRequest::decode(&req.encode()).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let req = a_request();
        let mut bytes = req.encode();
        bytes.push(0xff);
        assert!(ExtractionRequest::decode(&bytes).is_err());
    }

    #[test]
    fn truncated_bytes_are_rejected() {
        let req = a_request();
        let mut bytes = req.encode();
        bytes.truncate(bytes.len() - 3);
        assert!(ExtractionRequest::decode(&bytes).is_err());
    }

    #[test]
    fn an_unrecognized_extractor_kind_tag_is_rejected() {
        let mut bytes = Vec::new();
        put_u8(&mut bytes, 200);
        put_bytes(&mut bytes, b"x");
        encode_limits(&mut bytes, &ResourceLimits::conservative_default());
        assert_eq!(
            ExtractionRequest::decode(&bytes),
            Err(ProtocolError::BadMessage)
        );
    }

    #[test]
    fn a_declared_source_length_over_the_bound_is_refused_before_allocating() {
        let mut bytes = Vec::new();
        put_u8(&mut bytes, ExtractorKind::PlainTextNormalize.tag());
        // Declare a length far beyond MAX_SOURCE_BYTES without actually
        // supplying that many bytes -- decode must reject the declared
        // length itself, not attempt to allocate/read it.
        put_u32(&mut bytes, u32::MAX);
        assert_eq!(
            ExtractionRequest::decode(&bytes),
            Err(ProtocolError::BadMessage)
        );
    }
}
