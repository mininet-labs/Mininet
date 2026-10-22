# Local dependency maintenance

Source: crates.io askama_derive 0.16.0 (MIT OR Apache-2.0; licenses retained).

UniFFI 0.32 requires Askama 0.16. Replace its unmaintained basic-toml
configuration parser (RUSTSEC-2026-0374) with the maintained toml 1.1 parser.
Only Cargo.toml and the parser call in src/config.rs differ from upstream.
No advisory is suppressed. Remove this patch when the UniFFI dependency
chain supports an upstream Askama release without basic-toml.

Validation: generate the mini-ffi bindings using --features bindgen; run
Askama config unit tests with --manifest-path vendor/askama_derive/Cargo.toml.
