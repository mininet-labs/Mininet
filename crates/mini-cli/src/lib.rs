//! `mini` — the developer spine's command-line tool (Batch 1,
//! `docs/design/self-hosted-forge-spine.md`, D-0066). Wraps already-real
//! library code (`did-mini`, `mini-forge`, `mini-store`, `mini-objects`) so
//! a human can actually drive identity, repo, and governed-review
//! operations without hand-writing Rust against the library API.
//!
//! ## What this proves
//!
//! Batch 1's exit condition: two developers can exchange a signed proposed
//! commit, review the exact commit, and reach a governed canonical branch
//! head without GitHub being the authority. That exchange can use either a
//! shared `--store` path (a synced folder, a USB stick, anything that
//! copies files — content-addressed signed objects are safe to share via
//! any medium) or, as of `mini sync` (Batch 5, `crate::sync`), a real TCP
//! connection between two `mini` homes with no shared filesystem at all.
//!
//! ## Honest limits
//!
//! - No key rotation from the CLI yet (`crate::identity`'s module docs).
//! - No daemon (`mini-devd`): every invocation is a fresh process reading
//!   local files: acceptable for solo/small-group use, not for background
//!   sync or live event subscriptions. `mini sync` handles exactly one
//!   connection per invocation, then exits (`crate::sync`'s module docs).
//! - The per-home sequence counter (`crate::sequence`) is not safe for
//!   concurrent invocations against the same home.
//! - `repo branch --set` is a raw, ungoverned pointer move (the same
//!   primitive `mini-forge::set_branch` always was) — only `repo status`'s
//!   governed canonical heads (via `resolve_project`) are authoritative.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod cli;
mod error;
pub mod identity;
mod pr;
mod project;
mod repo;
mod sequence;
pub mod store;
mod sync;

pub use cli::run;
pub use error::{CliError, Result};
