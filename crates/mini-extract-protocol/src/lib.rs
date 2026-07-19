//! Length-delimited request/result messages between the Mininet Intake
//! coordinator and an isolated document-extractor worker process --
//! native-intake Track B3 (`docs/research/
//! MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
//! §25, "PR B3 — Extractor protocol and host").
//!
//! Mirrors `mini-pipeline-protocol`'s coordinator/isolated-runner IPC
//! discipline (self-hosted forge spine Batch 2b.1, D-0069) rather than
//! depending on it: each isolated-process protocol crate in this tree
//! hand-rolls its own small wire codec (the same choice `mini-intake-
//! types`, `mini-relay`, `mini-bridge`, and `mini-private-index` already
//! made) instead of creating a cross-domain dependency edge between
//! unrelated subsystems purely to reuse a few lines of framing code.
//!
//! [`frame::write_framed`]/[`frame::read_framed`] handle the length-
//! delimited, size-bounded framing. [`ExtractionRequest`] is what the
//! host sends the worker over its stdin; [`ExtractionOutcome`] is what
//! the worker sends back over its stdout -- either [`ExtractionSuccess`]
//! or a specific, structured [`ExtractionError`], never a generic
//! failure.
//!
//! This crate has zero I/O, zero process-spawning, and zero filesystem
//! access of its own -- it is purely the wire format. `mini-extract-
//! host` is the crate that actually spawns and talks to a worker.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod codec;
mod error;
mod frame;
mod request;
mod result;

pub use error::{ProtocolError, Result};
pub use frame::{read_framed, write_framed};
pub use request::{ExtractionRequest, ExtractorKind, ResourceLimits, MAX_SOURCE_BYTES};
pub use result::{ExtractionError, ExtractionOutcome, ExtractionSuccess, MAX_EXTRACTED_BYTES};
