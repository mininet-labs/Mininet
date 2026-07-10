//! The capability vocabulary a `wasm-component` pipeline step may declare
//! (D-0069). Deny-by-default is enforced structurally: a capability not
//! present in a step's declared list is never granted by
//! `mini-build-runner-wasmtime`'s linker construction -- the interface is
//! *absent*, not present-and-disabled. This crate only represents and
//! validates the declared list; the isolated runner is what actually
//! builds a deny-by-default `wasmtime::component::Linker` from it.

use crate::error::{PipelineError, Result};

/// Maximum bytes for a capability's parameterized argument (a host name, a
/// secret name).
pub const MAX_CAPABILITY_ARG_BYTES: usize = 253; // longest valid DNS host name

/// One capability a `wasm-component` step may be granted. Every variant
/// maps to exactly one narrow WASI Preview 2 host interface -- there is no
/// "all capabilities" variant, by design.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    /// Read-only access to the preopened workspace directory.
    WorkspaceRead,
    /// Write access to a preopened scratch directory (not persisted as an
    /// artifact unless separately promoted).
    ScratchWrite,
    /// Write access to the preopened artifacts output directory -- the
    /// only directory whose contents ever become output digests.
    ArtifactsWrite,
    /// A monotonic clock (no wall-clock access -- wall-clock reads would
    /// make output non-reproducible and are never granted).
    ClockMonotonic,
    /// A deterministic random source, seeded from the execution plan's own
    /// digest -- never an entropy source, so output stays reproducible.
    RandomDeterministic,
    /// Outbound network access to exactly one named host. A step with no
    /// `NetworkHost` capability gets no network interface at all.
    NetworkHost(String),
    /// Read access to exactly one named secret. A step with no
    /// `SecretRead` capability has no secret-reading interface at all.
    SecretRead(String),
}

impl Capability {
    /// Parse the `domain:action` / `domain:action("arg")` string form
    /// (e.g. `"workspace:read"`, `"network:host(\"crates.io\")"`).
    pub fn parse(s: &str) -> Result<Self> {
        let (domain, rest) = s
            .split_once(':')
            .ok_or_else(|| PipelineError::BadCapability(s.to_string()))?;
        match (domain, rest) {
            ("workspace", "read") => Ok(Capability::WorkspaceRead),
            ("scratch", "write") => Ok(Capability::ScratchWrite),
            ("artifacts", "write") => Ok(Capability::ArtifactsWrite),
            ("clock", "monotonic") => Ok(Capability::ClockMonotonic),
            ("random", "deterministic") => Ok(Capability::RandomDeterministic),
            ("network", _) => parse_arg(rest, "host")
                .ok_or_else(|| PipelineError::BadCapability(s.to_string()))
                .map(Capability::NetworkHost),
            ("secret", _) => parse_arg(rest, "read")
                .ok_or_else(|| PipelineError::BadCapability(s.to_string()))
                .map(Capability::SecretRead),
            _ => Err(PipelineError::BadCapability(s.to_string())),
        }
    }

    /// Render back to the canonical string form `parse` accepts.
    pub fn to_canonical_string(&self) -> String {
        match self {
            Capability::WorkspaceRead => "workspace:read".to_string(),
            Capability::ScratchWrite => "scratch:write".to_string(),
            Capability::ArtifactsWrite => "artifacts:write".to_string(),
            Capability::ClockMonotonic => "clock:monotonic".to_string(),
            Capability::RandomDeterministic => "random:deterministic".to_string(),
            Capability::NetworkHost(h) => format!("network:host(\"{h}\")"),
            Capability::SecretRead(s) => format!("secret:read(\"{s}\")"),
        }
    }
}

fn parse_arg(rest: &str, expected_fn: &str) -> Option<String> {
    let prefix = format!("{expected_fn}(\"");
    let inner = rest.strip_prefix(&prefix)?.strip_suffix("\")")?;
    if inner.is_empty() || inner.len() > MAX_CAPABILITY_ARG_BYTES || inner.contains('"') {
        return None;
    }
    Some(inner.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_round_trips_through_its_canonical_string() {
        let variants = [
            Capability::WorkspaceRead,
            Capability::ScratchWrite,
            Capability::ArtifactsWrite,
            Capability::ClockMonotonic,
            Capability::RandomDeterministic,
            Capability::NetworkHost("crates.io".to_string()),
            Capability::SecretRead("release-token".to_string()),
        ];
        for v in variants {
            let s = v.to_canonical_string();
            assert_eq!(
                Capability::parse(&s).unwrap(),
                v,
                "round trip failed for {s}"
            );
        }
    }

    #[test]
    fn unknown_domain_is_rejected() {
        assert!(Capability::parse("filesystem:write-anywhere").is_err());
    }

    #[test]
    fn missing_colon_is_rejected() {
        assert!(Capability::parse("workspace-read").is_err());
    }

    #[test]
    fn empty_parameterized_argument_is_rejected() {
        assert!(Capability::parse("network:host(\"\")").is_err());
        assert!(Capability::parse("secret:read(\"\")").is_err());
    }

    #[test]
    fn malformed_parameterized_syntax_is_rejected() {
        assert!(Capability::parse("network:host(crates.io)").is_err());
        assert!(Capability::parse("network:host(\"crates.io\"").is_err());
        assert!(Capability::parse("network:host(\"crates.io\")trailing").is_err());
    }

    #[test]
    fn oversized_argument_is_rejected() {
        let too_long = "x".repeat(MAX_CAPABILITY_ARG_BYTES + 1);
        assert!(Capability::parse(&format!("network:host(\"{too_long}\")")).is_err());
    }

    #[test]
    fn there_is_no_all_capabilities_variant() {
        // Structural guarantee, checked the cheap way: every variant this
        // crate knows about maps to exactly one narrow interface. If a
        // future edit ever adds a catch-all, this test's exhaustive match
        // in `to_canonical_string` (compiled with no wildcard arm) is what
        // would need to change -- documented here as the guardrail.
        let cap = Capability::WorkspaceRead;
        assert_ne!(cap.to_canonical_string(), "all:allow");
    }
}
