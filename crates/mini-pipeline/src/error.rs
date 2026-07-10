//! Error type for `mini-pipeline`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineError {
    /// A capability string did not parse (see [`crate::Capability::parse`]).
    BadCapability(String),
    /// A manifest field exceeded its bound.
    FieldTooLarge,
    /// A step name was empty, too long, or duplicated another step's name.
    BadStepName(String),
    /// A step's `depends_on` named a step that doesn't exist, or that
    /// would only be defined later (no forward references) -- lineage
    /// must be resolvable by construction, the same discipline
    /// `mini-forge`'s PR/chain-entry lineage checks use.
    UnknownDependency { step: String, dependency: String },
    /// The step dependency graph contains a cycle.
    DependencyCycle,
    /// A resource limit was zero or otherwise nonsensical (e.g. a cap of
    /// zero bytes of output, which would make every step fail trivially).
    BadResourceLimit,
    /// A manifest had zero steps.
    EmptyManifest,
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PipelineError::BadCapability(s) => write!(f, "invalid capability string: {s:?}"),
            PipelineError::FieldTooLarge => write!(f, "manifest field exceeds its size bound"),
            PipelineError::BadStepName(s) => write!(f, "invalid or duplicate step name: {s:?}"),
            PipelineError::UnknownDependency { step, dependency } => write!(
                f,
                "step {step:?} depends on {dependency:?}, which is not an earlier step in this manifest"
            ),
            PipelineError::DependencyCycle => write!(f, "step dependency graph contains a cycle"),
            PipelineError::BadResourceLimit => write!(f, "a resource limit is zero or otherwise invalid"),
            PipelineError::EmptyManifest => write!(f, "a pipeline manifest must have at least one step"),
        }
    }
}

impl std::error::Error for PipelineError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, PipelineError>;
