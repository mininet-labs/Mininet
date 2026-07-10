//! `wasmtime::ResourceLimiter` implementation enforcing
//! `ResourceLimits::max_memory_bytes` (D-0069 requirement: memory/table/
//! instance caps live here, separate from fuel/epoch CPU limits).

use wasmtime::ResourceLimiter;

pub struct MemoryLimiter {
    max_memory_bytes: usize,
    hit_limit: bool,
}

impl MemoryLimiter {
    pub fn new(max_memory_bytes: u64) -> Self {
        MemoryLimiter {
            max_memory_bytes: max_memory_bytes as usize,
            hit_limit: false,
        }
    }

    /// Whether growth was ever refused -- lets the caller distinguish "the
    /// guest legitimately finished" from "the guest was memory-capped",
    /// even though a refused `memory.grow` returns -1 to the guest rather
    /// than trapping (matching a well-behaved allocator's expectations).
    pub fn hit_limit(&self) -> bool {
        self.hit_limit
    }
}

impl ResourceLimiter for MemoryLimiter {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > self.max_memory_bytes {
            self.hit_limit = true;
            return Ok(false);
        }
        Ok(true)
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        // A generous, fixed table cap: pipeline steps are build-tool
        // components, not workloads with a legitimate need for huge
        // indirect-call tables. Not separately configurable via
        // `ResourceLimits` today -- the founder's spec named memory,
        // fuel, and wall clock explicitly; this is a conservative
        // structural backstop, not a policy knob.
        const MAX_TABLE_ELEMENTS: usize = 1_000_000;
        if desired > MAX_TABLE_ELEMENTS {
            self.hit_limit = true;
            return Ok(false);
        }
        Ok(true)
    }

    fn instances(&self) -> usize {
        // A single `wasi:cli/command` component typically links several
        // core-wasm instances under the hood (the component itself plus
        // WASI adapter/shim instances) -- one instance is too tight, as
        // discovered empirically running the Batch 2b.3 adversarial
        // suite. 32 is still far below Wasmtime's own default (10,000)
        // and generous enough for any single build-tool component.
        32
    }

    fn tables(&self) -> usize {
        32
    }

    fn memories(&self) -> usize {
        32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn growth_within_the_limit_is_allowed() {
        let mut limiter = MemoryLimiter::new(1024);
        assert!(limiter.memory_growing(0, 512, None).unwrap());
        assert!(!limiter.hit_limit());
    }

    #[test]
    fn growth_past_the_limit_is_refused_and_recorded() {
        let mut limiter = MemoryLimiter::new(1024);
        assert!(!limiter.memory_growing(0, 2048, None).unwrap());
        assert!(limiter.hit_limit());
    }
}
