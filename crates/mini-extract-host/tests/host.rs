//! Adversarial/integration tests against the real compiled
//! `mini-extract-worker` binary, spawned as a genuine child process --
//! not an in-process mock. `env!("CARGO_BIN_EXE_mini-extract-worker")` is
//! Cargo's own guarantee that the binary named by this crate's `[[bin]]`
//! is built before these tests run.

use std::path::{Path, PathBuf};

use mini_extract_host::{run_worker, HostError};
use mini_extract_protocol::{ExtractionOutcome, ExtractionRequest, ExtractorKind, ResourceLimits};

fn worker_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mini-extract-worker"))
}

#[test]
fn a_real_end_to_end_extraction_normalizes_whitespace() {
    let request = ExtractionRequest {
        kind: ExtractorKind::PlainTextNormalize,
        source_bytes: b"hello   world\n\ttabbed".to_vec(),
        limits: ResourceLimits::conservative_default(),
    };
    let outcome = run_worker(&worker_binary(), &request).unwrap();
    match outcome {
        ExtractionOutcome::Ok(success) => {
            assert_eq!(
                String::from_utf8(success.extracted_bytes).unwrap(),
                "hello world\n tabbed"
            );
        }
        other => panic!("expected success, got {other:?}"),
    }
}

#[test]
fn empty_input_extracts_to_empty_output() {
    let request = ExtractionRequest {
        kind: ExtractorKind::PlainTextNormalize,
        source_bytes: Vec::new(),
        limits: ResourceLimits::conservative_default(),
    };
    let outcome = run_worker(&worker_binary(), &request).unwrap();
    match outcome {
        ExtractionOutcome::Ok(success) => assert!(success.extracted_bytes.is_empty()),
        other => panic!("expected success, got {other:?}"),
    }
}

#[test]
fn invalid_utf8_source_bytes_are_lossy_decoded_not_rejected() {
    let request = ExtractionRequest {
        kind: ExtractorKind::PlainTextNormalize,
        source_bytes: vec![b'a', 0xff, 0xfe, b'b'],
        limits: ResourceLimits::conservative_default(),
    };
    let outcome = run_worker(&worker_binary(), &request).unwrap();
    assert!(matches!(outcome, ExtractionOutcome::Ok(_)));
}

#[test]
fn output_over_the_declared_limit_is_reported_as_output_too_large() {
    let request = ExtractionRequest {
        kind: ExtractorKind::PlainTextNormalize,
        source_bytes: b"this line of text is longer than five bytes".to_vec(),
        limits: ResourceLimits {
            max_wall_clock_ms: 5_000,
            max_output_bytes: 5,
        },
    };
    let outcome = run_worker(&worker_binary(), &request).unwrap();
    assert!(matches!(
        outcome,
        ExtractionOutcome::Err(mini_extract_protocol::ExtractionError::OutputTooLarge {
            max: 5,
            ..
        })
    ));
}

#[test]
fn a_zero_millisecond_deadline_is_reported_as_timeout() {
    let request = ExtractionRequest {
        kind: ExtractorKind::PlainTextNormalize,
        source_bytes: b"anything".to_vec(),
        limits: ResourceLimits {
            max_wall_clock_ms: 0,
            max_output_bytes: 1024,
        },
    };
    let outcome = run_worker(&worker_binary(), &request).unwrap();
    assert!(matches!(
        outcome,
        ExtractionOutcome::Err(mini_extract_protocol::ExtractionError::Timeout)
    ));
}

#[test]
fn a_missing_worker_binary_is_a_host_error_not_a_panic() {
    let request = ExtractionRequest {
        kind: ExtractorKind::PlainTextNormalize,
        source_bytes: b"x".to_vec(),
        limits: ResourceLimits::conservative_default(),
    };
    let result = run_worker(
        Path::new("/nonexistent/mini-extract-worker-binary"),
        &request,
    );
    assert!(matches!(result, Err(HostError::Spawn(_))));
}

#[cfg(unix)]
#[test]
fn a_process_that_exits_without_a_result_frame_is_reported_as_extractor_crashed() {
    let request = ExtractionRequest {
        kind: ExtractorKind::PlainTextNormalize,
        source_bytes: b"x".to_vec(),
        limits: ResourceLimits::conservative_default(),
    };
    // `true` is present on every POSIX system this workspace targets; it
    // exits 0 immediately without reading stdin or writing to stdout, so
    // the host sees a clean EOF with no result frame at all -- exactly
    // the "worker crashed/exited early" case, without needing a purpose-
    // built broken-worker test fixture binary.
    let outcome = run_worker(Path::new("true"), &request).unwrap();
    assert!(matches!(
        outcome,
        ExtractionOutcome::Err(mini_extract_protocol::ExtractionError::ExtractorCrashed {
            exit_code: Some(0)
        })
    ));
}

#[test]
fn two_concurrent_extractions_do_not_interfere() {
    let make_request = |text: &str| ExtractionRequest {
        kind: ExtractorKind::PlainTextNormalize,
        source_bytes: text.as_bytes().to_vec(),
        limits: ResourceLimits::conservative_default(),
    };
    let binary = worker_binary();
    let b1 = binary.clone();
    let b2 = binary.clone();
    let h1 = std::thread::spawn(move || run_worker(&b1, &make_request("first   run")).unwrap());
    let h2 = std::thread::spawn(move || run_worker(&b2, &make_request("second   run")).unwrap());
    let out1 = h1.join().unwrap();
    let out2 = h2.join().unwrap();
    match (out1, out2) {
        (ExtractionOutcome::Ok(a), ExtractionOutcome::Ok(b)) => {
            assert_eq!(String::from_utf8(a.extracted_bytes).unwrap(), "first run");
            assert_eq!(String::from_utf8(b.extracted_bytes).unwrap(), "second run");
        }
        other => panic!("expected both to succeed, got {other:?}"),
    }
}
