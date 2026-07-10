//! Declared resource limits for one pipeline step. These are *policy* --
//! what a step's author asks for and what a reviewer approves. Actually
//! enforcing them (fuel/epoch interruption, a `wasmtime::ResourceLimiter`,
//! parent-side wall-clock termination) is `mini-build-runner-wasmtime`'s
//! job; this crate only validates that the declared numbers are sane.

use crate::error::{PipelineError, Result};

/// Declared resource limits for one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    /// Fuel budget (an abstract, deterministic CPU-work unit Wasmtime
    /// counts down as the guest executes) -- the primary, reproducible
    /// termination mechanism. Never zero: a step needs to be able to do
    /// at least some work.
    pub max_fuel: u64,
    /// Maximum linear memory the guest instance may grow to, in bytes.
    pub max_memory_bytes: u64,
    /// Parent-enforced wall-clock timeout, in milliseconds -- the
    /// *emergency* stop for cases fuel accounting doesn't catch (e.g. a
    /// tight host-call loop), never the primary reproducibility mechanism.
    pub max_wall_clock_ms: u64,
    /// Maximum total bytes across every file written under
    /// `artifacts:write`.
    pub max_output_bytes: u64,
    /// Maximum stdout bytes captured before truncation/failure.
    pub max_stdout_bytes: u64,
    /// Maximum stderr bytes captured before truncation/failure.
    pub max_stderr_bytes: u64,
    /// Maximum simultaneously open files/descriptors.
    pub max_open_files: u32,
}

impl ResourceLimits {
    /// A conservative default: enough fuel and memory for a small,
    /// well-behaved build step; tight enough that a runaway step is
    /// caught quickly. Deployments are expected to tune this per step,
    /// not treat it as universally correct.
    pub fn conservative_default() -> Self {
        ResourceLimits {
            max_fuel: 10_000_000_000,
            max_memory_bytes: 256 * 1024 * 1024,
            max_wall_clock_ms: 60_000,
            max_output_bytes: 64 * 1024 * 1024,
            max_stdout_bytes: 1024 * 1024,
            max_stderr_bytes: 1024 * 1024,
            max_open_files: 64,
        }
    }

    /// Every limit must be nonzero -- a zero limit would make the step
    /// fail trivially, which is never a meaningful policy choice (use
    /// `NativeTool`'s absence, or simply omit the step, instead).
    pub fn validate(&self) -> Result<()> {
        let all_nonzero = self.max_fuel != 0
            && self.max_memory_bytes != 0
            && self.max_wall_clock_ms != 0
            && self.max_output_bytes != 0
            && self.max_stdout_bytes != 0
            && self.max_stderr_bytes != 0
            && self.max_open_files != 0;
        if !all_nonzero {
            return Err(PipelineError::BadResourceLimit);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conservative_default_validates() {
        assert!(ResourceLimits::conservative_default().validate().is_ok());
    }

    #[test]
    fn a_zero_limit_in_any_field_is_rejected() {
        let base = ResourceLimits::conservative_default();
        let mut with_zero_fuel = base;
        with_zero_fuel.max_fuel = 0;
        assert!(with_zero_fuel.validate().is_err());

        let mut with_zero_memory = base;
        with_zero_memory.max_memory_bytes = 0;
        assert!(with_zero_memory.validate().is_err());

        let mut with_zero_files = base;
        with_zero_files.max_open_files = 0;
        assert!(with_zero_files.validate().is_err());
    }
}
