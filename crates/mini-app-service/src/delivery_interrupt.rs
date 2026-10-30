//! Stop-only coordination: possession cannot authorize signing or networking.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

/// Counts pending owner stop requests across the reader and authority thread.
#[derive(Clone, Debug, Default)]
pub struct DeliveryInterrupt {
    pending: Arc<AtomicUsize>,
}
impl DeliveryInterrupt {
    /// Hold this guard until its corresponding control request is processed.
    /// Overlapping guards cannot clear one another's pending stop requests.
    pub fn stop(&self) -> DeliveryStopGuard {
        self.pending
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .expect("delivery stop request count exhausted");
        DeliveryStopGuard {
            pending: self.pending.clone(),
        }
    }
    pub fn is_stopped(&self) -> bool {
        self.pending.load(Ordering::Acquire) != 0
    }
}
#[must_use = "hold the guard until the corresponding stop request is processed"]
#[derive(Debug)]
pub struct DeliveryStopGuard {
    pending: Arc<AtomicUsize>,
}
impl Drop for DeliveryStopGuard {
    fn drop(&mut self) {
        self.pending.fetch_sub(1, Ordering::AcqRel);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_requests_and_clones_cannot_lose_a_pending_stop() {
        let interrupt = DeliveryInterrupt::default();
        let clone = interrupt.clone();
        assert!(!interrupt.is_stopped());
        let first = interrupt.stop();
        let second = clone.stop();
        assert!(clone.is_stopped());
        drop(first);
        assert!(interrupt.is_stopped());
        let third = interrupt.stop();
        drop(second);
        assert!(clone.is_stopped());
        drop(interrupt);
        assert!(clone.is_stopped());
        drop(third);
        assert!(!clone.is_stopped());
    }
}
