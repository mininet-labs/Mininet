//! The isolated build runner's library surface (D-0069, self-hosted forge
//! spine Batch 2b.2). `main.rs` is a thin binary entry point over this;
//! the library exists so integration tests (Batch 2b.3) can drive the
//! same content-store and digest helpers the real binary uses, without
//! duplicating that logic.
//!
//! This is the only crate in the tree permitted to depend on `wasmtime`/
//! `wasmtime-wasi`. `mini-cli`, `mini-forge`, `mini-chain`, identity, and
//! every ordinary node binary must never gain a dependency edge to this
//! crate for anything beyond spawning its compiled binary as a
//! subprocess and speaking `mini-pipeline-protocol` over its stdin/
//! stdout.

pub mod content_store;
pub mod error;
pub mod limiter;
pub mod random;
pub mod sandbox;
