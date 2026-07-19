//! `mini-extract-worker` -- reads exactly one framed [`ExtractionRequest`]
//! from stdin, runs the requested built-in extractor, and writes exactly
//! one framed [`ExtractionOutcome`] to stdout, then exits. Never spawned
//! directly by a human; [`mini_extract_host::run_worker`] is the intended
//! caller. Deliberately does nothing else: no filesystem access beyond
//! process startup, no network access, no `mini-store` dependency -- the
//! request's `source_bytes` are the only external input this process
//! ever sees.

use std::io;

use mini_extract_protocol::{
    read_framed, write_framed, ExtractionOutcome, ExtractionRequest, ExtractionSuccess,
    MAX_SOURCE_BYTES,
};

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let stdin = io::stdin();
    let mut stdin_lock = stdin.lock();
    let frame = match read_framed(&mut stdin_lock, MAX_SOURCE_BYTES + 4096) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return 1,
        Err(_) => return 1,
    };
    let request = match ExtractionRequest::decode(&frame) {
        Ok(r) => r,
        Err(_) => return 1,
    };

    let start = std::time::Instant::now();
    let outcome = match mini_extract_host::extractor::run(
        request.kind,
        &request.source_bytes,
        request.limits.max_output_bytes,
    ) {
        Ok(extracted_bytes) => ExtractionOutcome::Ok(ExtractionSuccess {
            extracted_bytes,
            wall_clock_ms: u32::try_from(start.elapsed().as_millis()).unwrap_or(u32::MAX),
        }),
        Err(e) => ExtractionOutcome::Err(e),
    };

    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();
    match write_framed(&mut stdout_lock, &outcome.encode()) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
