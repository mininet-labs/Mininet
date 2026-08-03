//! Pulling from several peers in one federation-refresh session, bounded by
//! how many distinct sources a caller may contact at once.
//!
//! Fault isolation across peers is deliberately **not** attempted here: the
//! first peer that errors aborts the whole session, the same fail-fast
//! behavior [`crate::pull_source`] already has for one peer. Tolerating a
//! misbehaving or offline peer while still finishing the others is a real,
//! separate design question (which errors are peer-local vs. session-fatal,
//! how a partial session is reported) left to later, more targeted work
//! rather than guessed at here.

use did_mini::Did;
use mini_bearer::{Bearer, Channel};
use mini_store::{Backend, Store};
use mini_sync::KelCache;

use crate::error::{NetError, Result};
use crate::session::{pull_source, SourcePullReport};

/// One already-connected peer to pull from: the caller owns connection setup
/// (dialing, the `mini_bearer` handshake) exactly as `mini-sync`'s own
/// callers do — this crate only ever takes an established `Bearer`/`Channel`
/// pair, never opens a socket itself.
#[allow(missing_debug_implementations)] // `dyn Bearer`/`Channel` carry no printable state worth deriving
pub struct PeerSource<'a> {
    pub bearer: &'a mut dyn Bearer,
    pub chan: &'a mut Channel,
    /// Binds this peer's session to a specific identity; `None` accepts
    /// F1/F2 objects from whichever identity actually signed them (still
    /// KEL-verified by `mini-sync`, just not attributed to an expected
    /// provider).
    pub expected_provider: Option<Did>,
    /// Per-peer cap passed through to [`pull_source`].
    pub max_objects: usize,
}

/// One session's worth of per-peer results.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FederationPullReport {
    pub sources: Vec<SourcePullReport>,
}

/// Pull from every source in `sources`, refusing (not truncating) a session
/// that names more than `max_sources` distinct peers. A caller wanting to
/// federate more sources than its own policy allows must split the work
/// into multiple sessions with a lower per-session count, not rely on this
/// function silently dropping the tail of its own list.
pub fn pull_from_sources<B: Backend>(
    sources: Vec<PeerSource>,
    store: &mut Store<B>,
    cache: &mut KelCache,
    max_sources: usize,
) -> Result<FederationPullReport> {
    if max_sources == 0 || sources.len() > max_sources {
        return Err(NetError::TooManySources);
    }

    let mut reports = Vec::with_capacity(sources.len());
    for source in sources {
        let report = pull_source(
            source.bearer,
            source.chan,
            store,
            cache,
            source.expected_provider.as_ref(),
            source.max_objects,
        )?;
        reports.push(report);
    }
    Ok(FederationPullReport { sources: reports })
}
