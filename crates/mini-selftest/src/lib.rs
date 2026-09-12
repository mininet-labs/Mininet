//! Diagnostics that run the real Mininet stack and report what happened.
//!
//! ## What this is for
//!
//! A protocol whose guarantees can only be confirmed by reading its test
//! suite is a protocol most of its users cannot confirm at all. This crate
//! exists so a person holding the client --- not a contributor with a Rust
//! toolchain --- can press a button, watch identity, storage, social objects,
//! chunked media, encrypted messaging, peer sync over a real socket, governed
//! review, erasure coding, storage proofs, and the Windows install path all
//! actually execute, and read the result.
//!
//! It is also the honest answer to "which of these features really work?".
//! Every check runs the shipped library code against a temporary directory or
//! a loopback socket. A check that cannot run says so and why
//! ([`Outcome::Skipped`]); none of them fake a pass.
//!
//! ## Negative checks carry most of the weight
//!
//! Roughly half of these prove a *refusal*: one approval does not reach the
//! two-approval protocol floor, an approval bound to one commit does not
//! carry to another, a message key for one conversation does not read
//! another's, a tampered package does not install. A suite of happy paths
//! tells you the code can succeed; it is the refusals that tell you the
//! guarantees are load-bearing, and those are exactly the properties a user
//! is being asked to trust.
//!
//! ## Deliberately not covered
//!
//! This crate has a governance-crate edge (`mini-forge`), so under P1 /
//! Directive 16 --- the voice/value wall --- it must never gain an edge to
//! `mini-value`, `mini-bounty`, or `mini-treasury`. There is therefore no
//! shielded-payment or bounty check here, and adding one would be a wall
//! violation rather than a missing feature. Those crates have their own
//! suites; a diagnostics front end must not be the seam where value and
//! governance meet.
//!
//! Nothing here touches the user's real data. Every check builds its own
//! throwaway state, and the two checks that read an existing installation
//! only read it.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]
#![warn(missing_docs)]

mod checks;

pub use checks::{all_checks, default_scratch, run_all, run_area, AREAS};

/// How one check ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The check ran and the property held.
    Passed {
        /// What was observed, concretely enough to be worth reading.
        detail: String,
    },
    /// The check ran and the property did not hold.
    Failed {
        /// What went wrong.
        detail: String,
    },
    /// The check could not run here.
    ///
    /// A first-class outcome rather than a silent pass: "the Windows identity
    /// vault was not exercised because this is not Windows" is information,
    /// and reporting it as a pass would be a lie of exactly the kind this
    /// crate exists to prevent.
    Skipped {
        /// Why not.
        reason: String,
    },
}

impl Outcome {
    /// A short, stable machine name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Passed { .. } => "passed",
            Self::Failed { .. } => "failed",
            Self::Skipped { .. } => "skipped",
        }
    }

    /// The explanatory text, whichever variant this is.
    pub fn detail(&self) -> &str {
        match self {
            Self::Passed { detail } | Self::Failed { detail } => detail,
            Self::Skipped { reason } => reason,
        }
    }

    /// True for [`Outcome::Failed`].
    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

/// One diagnostic and its result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// Which subsystem: `identity`, `social`, `forge`, `install`, ...
    pub area: &'static str,
    /// What property this check establishes, written as a claim.
    pub name: &'static str,
    /// Whether the claim is a refusal the protocol must enforce.
    ///
    /// Surfaced so a reader can see at a glance that the suite is not only
    /// happy paths.
    pub negative: bool,
    /// What happened.
    pub outcome: Outcome,
}

/// A whole run.
#[derive(Debug, Clone)]
pub struct Report {
    /// Every check, in the order it ran.
    pub checks: Vec<Check>,
    /// Milliseconds the run took.
    pub elapsed_ms: u64,
}

impl Report {
    /// How many passed.
    pub fn passed(&self) -> usize {
        self.checks
            .iter()
            .filter(|check| matches!(check.outcome, Outcome::Passed { .. }))
            .count()
    }

    /// How many failed.
    pub fn failed(&self) -> usize {
        self.checks
            .iter()
            .filter(|check| check.outcome.is_failure())
            .count()
    }

    /// How many could not run here.
    pub fn skipped(&self) -> usize {
        self.checks
            .iter()
            .filter(|check| matches!(check.outcome, Outcome::Skipped { .. }))
            .count()
    }

    /// How many of the checks that ran were refusals.
    pub fn negative_checks(&self) -> usize {
        self.checks
            .iter()
            .filter(|check| check.negative && !matches!(check.outcome, Outcome::Skipped { .. }))
            .count()
    }

    /// True when nothing failed. A skip is not a failure.
    pub fn is_clean(&self) -> bool {
        self.failed() == 0
    }

    /// A one-line summary.
    pub fn summary(&self) -> String {
        format!(
            "{} passed, {} failed, {} skipped ({} of them refusal checks) in {} ms",
            self.passed(),
            self.failed(),
            self.skipped(),
            self.negative_checks(),
            self.elapsed_ms
        )
    }
}
