#![no_main]
//! Fetch messages: never panic; whatever decodes re-encodes to the same bytes.
use libfuzzer_sys::fuzz_target;
use mini_shielded_verify::exchange::{ClaimRequest, ClaimResponse};

fuzz_target!(|data: &[u8]| {
    if let Ok(request) = ClaimRequest::decode(data) {
        assert_eq!(request.encode(), data);
    }
    if let Ok(response) = ClaimResponse::decode(data) {
        assert_eq!(response.encode(), data);
    }
});
