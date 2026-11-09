# Fuzz targets for the byte-facing decoders (D-0553)

Coverage-guided `cargo-fuzz` targets, kept outside the main workspace because they
need a nightly toolchain and `cargo-fuzz`:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run v3_claim_decode        # likewise: claim_fetch_messages,
                                               # evidence_file_load, observer_journal_open
```

| Target | Property |
| --- | --- |
| `v3_claim_decode` | decoding never panics; a claim that re-encodes to its own bytes has a stable `claim_id` |
| `claim_fetch_messages` | fetch request/response codecs never panic and round-trip exactly |
| `evidence_file_load` | an arbitrary evidence file fails closed or loads a pool that re-saves and reloads identically |
| `observer_journal_open` | an arbitrary receipt journal fails closed or opens; never panics |

## What has and has not been run

The targets compile on stable and were smoke-run there (20,000 random inputs each,
no crashes). Stable has no sanitizer-coverage instrumentation, so that run was
**not** coverage-guided: it proves the harnesses work, not that the decoders have
been fuzzed. A real campaign needs the nightly commands above and enough
wall-clock time, and should add any crash it finds as a regression test beside the
decoder. Nothing here is a claim of fuzz-testing assurance yet.
