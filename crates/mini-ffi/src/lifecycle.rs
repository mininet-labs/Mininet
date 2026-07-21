//! Lifecycle-aware state-machine boundary for backgroundable operations
//! (issue #202, Android beta slice 6).
//!
//! A LAN/QR pairing exchange (`mini-social`'s `pairing` module, D-0340) or a
//! BLE bearer transfer (`mini-bearer`'s `ble` module, D-0342) can be
//! mid-flight when Android decides to background or freeze the app. The
//! platform shell must never let that produce a silent partial/corrupt
//! result: it either keeps
//! the operation alive long enough to reach a safe pause point (a real
//! foreground service, Kotlin-side) or it fails closed with a reason the
//! caller can render — resumable state, never silent data loss. This module
//! is the pure, typed state machine the Kotlin lifecycle glue queries to
//! decide which of those two paths applies; it holds no network/BLE
//! resources itself and drives no I/O.

use std::sync::Mutex;

/// A concrete operation this lifecycle machinery protects. Named for the
/// specific resumable exchanges this workspace already implements, not a
/// generic placeholder — new backgroundable operations get a new named
/// variant here, not a bare string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundableOperation {
    /// The LAN/QR pairing offer/acceptance exchange (`mini-social`'s
    /// `pairing` module).
    LanQrPairingExchange,
    /// A chunked BLE bearer transfer (`mini-bearer`'s `ble` module).
    BleBearerTransfer,
}

/// Typed cause recorded when an operation fails closed. A closed enum,
/// matching this crate's existing `AppError`/`RootError` convention, rather
/// than a free-form string the UI would have to interpret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleFailureReason {
    /// No safe checkpoint was reached before a deadline the caller enforced.
    Timeout,
    /// The remote peer disconnected before a safe checkpoint was reached.
    PeerDisconnected,
    /// The platform tore down the hosting service before a safe checkpoint
    /// was reached (e.g. the foreground-service request was denied or
    /// revoked).
    PlatformTerminated,
}

impl core::fmt::Display for LifecycleFailureReason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::Timeout => "no safe checkpoint was reached before the deadline",
            Self::PeerDisconnected => "the remote peer disconnected before a safe checkpoint",
            Self::PlatformTerminated => {
                "the platform terminated the hosting service before a safe checkpoint"
            }
        };
        f.write_str(message)
    }
}

impl std::error::Error for LifecycleFailureReason {}

/// The verdict [`OperationLifecycle::request_suspend`] returns: whether the
/// caller may let the OS suspend/kill the process right now, or must either
/// buy more time (a foreground service, one more retry) or fail closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspendDecision {
    /// A checkpoint has been reached; nothing is lost if the process is
    /// suspended or killed now.
    SafeToSuspend,
    /// No checkpoint has been reached; suspending now would silently lose
    /// or corrupt in-flight state. The caller must keep running (e.g. via a
    /// declared foreground service) until the next checkpoint, or call
    /// [`OperationLifecycle::fail_closed`] itself.
    MustCompleteOrFailClosed,
}

/// A snapshot-safe phase for Kotlin to render (e.g. a foreground-service
/// notification body), satisfying issue #202's requirement that no
/// background operation run without an explicit, visible reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecyclePhase {
    /// Mid-step; unsafe to suspend right now.
    InFlight,
    /// At a safe pause point; suspending now loses nothing.
    AtCheckpoint,
    /// Suspended at a prior checkpoint; resumable.
    Suspended,
    /// Finished successfully. Terminal.
    Completed,
    /// Failed closed with a recorded, visible reason. Terminal.
    FailedClosed,
}

/// Stable failure classes for [`OperationLifecycle`]'s own transitions —
/// misuse of the state machine itself, distinct from
/// [`LifecycleFailureReason`], which records why the *protected operation*
/// failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleError {
    /// The lifecycle already reached a terminal phase
    /// ([`LifecyclePhase::Completed`] or [`LifecyclePhase::FailedClosed`]);
    /// no further transition is possible.
    AlreadyTerminal,
    /// [`OperationLifecycle::resume_from_suspend`] was called while not
    /// actually suspended.
    NotSuspended,
    /// [`OperationLifecycle::mark_checkpoint`] was called while not
    /// in-flight (e.g. already at a checkpoint, or suspended).
    NotInFlight,
    /// [`OperationLifecycle::begin_next_step`] was called while not at a
    /// checkpoint (e.g. already in-flight, or suspended).
    NotAtCheckpoint,
}

impl core::fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::AlreadyTerminal => "the operation already reached a terminal phase",
            Self::NotSuspended => "the operation is not currently suspended",
            Self::NotInFlight => "the operation is not currently in-flight",
            Self::NotAtCheckpoint => "the operation is not currently at a checkpoint",
        };
        f.write_str(message)
    }
}

impl std::error::Error for LifecycleError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    InFlight,
    AtCheckpoint,
    Suspended,
    Completed,
    FailedClosed(LifecycleFailureReason),
}

impl Phase {
    fn as_public(self) -> LifecyclePhase {
        match self {
            Self::InFlight => LifecyclePhase::InFlight,
            Self::AtCheckpoint => LifecyclePhase::AtCheckpoint,
            Self::Suspended => LifecyclePhase::Suspended,
            Self::Completed => LifecyclePhase::Completed,
            Self::FailedClosed(_) => LifecyclePhase::FailedClosed,
        }
    }

    fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::FailedClosed(_))
    }
}

/// Typed lifecycle tracker for one in-flight [`BackgroundableOperation`].
/// Holds no network/BLE/socket resource itself — the caller drives the
/// actual exchange and reports checkpoints/suspends/failures into this
/// state machine, which only ever answers "is it safe to suspend" and
/// enforces that a corrupt silent transition never happens.
///
/// Exposed as a UniFFI interface object (like [`crate::RootCore`]) so a
/// single instance can be shared across the Kotlin lifecycle callbacks
/// (`onStop`, a foreground-service `onDestroy`, a retry) without threading
/// state through the FFI boundary by hand each time.
#[derive(Debug)]
pub struct OperationLifecycle {
    operation: BackgroundableOperation,
    phase: Mutex<Phase>,
}

impl OperationLifecycle {
    /// Begin tracking `operation`. Starts `InFlight`: the very first step of
    /// any operation is unsafe to interrupt until the caller explicitly
    /// reports a checkpoint.
    pub fn begin(operation: BackgroundableOperation) -> Self {
        Self {
            operation,
            phase: Mutex::new(Phase::InFlight),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Phase> {
        self.phase
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// The operation this instance tracks.
    pub fn operation(&self) -> BackgroundableOperation {
        self.operation
    }

    /// The current phase, safe to render directly in UI/notification text.
    pub fn phase(&self) -> LifecyclePhase {
        self.lock().as_public()
    }

    /// The recorded reason, if this operation failed closed; `None` in
    /// every other phase.
    pub fn failure_reason(&self) -> Option<LifecycleFailureReason> {
        match *self.lock() {
            Phase::FailedClosed(reason) => Some(reason),
            _ => None,
        }
    }

    /// Report that a safe pause point was reached (e.g. one pairing message
    /// round-trip completed, or one BLE chunk was fully written and
    /// acknowledged). Only valid while `InFlight`.
    pub fn mark_checkpoint(&self) -> Result<(), LifecycleError> {
        let mut phase = self.lock();
        match *phase {
            Phase::InFlight => {
                *phase = Phase::AtCheckpoint;
                Ok(())
            }
            Phase::AtCheckpoint | Phase::Suspended => Err(LifecycleError::NotInFlight),
            Phase::Completed | Phase::FailedClosed(_) => Err(LifecycleError::AlreadyTerminal),
        }
    }

    /// Report that the next unsafe-to-interrupt step is starting. Only
    /// valid from `AtCheckpoint` — this is how the caller re-enters unsafe
    /// territory for the next atomic step of the exchange.
    pub fn begin_next_step(&self) -> Result<(), LifecycleError> {
        let mut phase = self.lock();
        match *phase {
            Phase::AtCheckpoint => {
                *phase = Phase::InFlight;
                Ok(())
            }
            Phase::InFlight | Phase::Suspended => Err(LifecycleError::NotAtCheckpoint),
            Phase::Completed | Phase::FailedClosed(_) => Err(LifecycleError::AlreadyTerminal),
        }
    }

    /// Ask whether it is safe to let the OS suspend/kill the process right
    /// now. This is the acceptance-test-1 decision point: `InFlight` always
    /// answers [`SuspendDecision::MustCompleteOrFailClosed`] — the caller
    /// must then either buy time (a real foreground service) or call
    /// [`Self::fail_closed`] itself, never silently let the process die
    /// mid-step. Any other phase answers
    /// [`SuspendDecision::SafeToSuspend`] and, if not already suspended or
    /// terminal, transitions to `Suspended`.
    pub fn request_suspend(&self) -> SuspendDecision {
        let mut phase = self.lock();
        match *phase {
            Phase::InFlight => SuspendDecision::MustCompleteOrFailClosed,
            Phase::AtCheckpoint => {
                *phase = Phase::Suspended;
                SuspendDecision::SafeToSuspend
            }
            Phase::Suspended | Phase::Completed | Phase::FailedClosed(_) => {
                SuspendDecision::SafeToSuspend
            }
        }
    }

    /// Resume after a suspend, returning to the checkpoint the operation
    /// was suspended at. The caller should call [`Self::begin_next_step`]
    /// next to continue the exchange. Only valid while `Suspended`.
    pub fn resume_from_suspend(&self) -> Result<(), LifecycleError> {
        let mut phase = self.lock();
        match *phase {
            Phase::Suspended => {
                *phase = Phase::AtCheckpoint;
                Ok(())
            }
            Phase::InFlight | Phase::AtCheckpoint => Err(LifecycleError::NotSuspended),
            Phase::Completed | Phase::FailedClosed(_) => Err(LifecycleError::AlreadyTerminal),
        }
    }

    /// Fail closed with a visible, typed reason rather than let the
    /// operation be silently abandoned. Valid from any non-terminal phase;
    /// the resulting `FailedClosed` state is exactly the "visible,
    /// resumable state — never a silent partial/corrupt result" issue #202
    /// requires when a checkpoint cannot be reached in time.
    pub fn fail_closed(&self, reason: LifecycleFailureReason) -> Result<(), LifecycleError> {
        let mut phase = self.lock();
        if phase.is_terminal() {
            return Err(LifecycleError::AlreadyTerminal);
        }
        *phase = Phase::FailedClosed(reason);
        Ok(())
    }

    /// Mark the operation as finished successfully. Valid from `InFlight`
    /// or `AtCheckpoint`; not from `Suspended` (resume first) or an
    /// already-terminal phase.
    pub fn complete(&self) -> Result<(), LifecycleError> {
        let mut phase = self.lock();
        match *phase {
            Phase::InFlight | Phase::AtCheckpoint => {
                *phase = Phase::Completed;
                Ok(())
            }
            Phase::Suspended => Err(LifecycleError::NotInFlight),
            Phase::Completed | Phase::FailedClosed(_) => Err(LifecycleError::AlreadyTerminal),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begins_in_flight() {
        let lifecycle = OperationLifecycle::begin(BackgroundableOperation::LanQrPairingExchange);
        assert_eq!(lifecycle.phase(), LifecyclePhase::InFlight);
        assert_eq!(
            lifecycle.operation(),
            BackgroundableOperation::LanQrPairingExchange
        );
    }

    #[test]
    fn in_flight_must_complete_or_fail_closed_on_suspend_request() {
        let lifecycle = OperationLifecycle::begin(BackgroundableOperation::BleBearerTransfer);
        assert_eq!(
            lifecycle.request_suspend(),
            SuspendDecision::MustCompleteOrFailClosed
        );
        // Nothing was silently lost: still in-flight, not suspended.
        assert_eq!(lifecycle.phase(), LifecyclePhase::InFlight);
    }

    #[test]
    fn checkpoint_then_suspend_is_safe_and_resumable() {
        let lifecycle = OperationLifecycle::begin(BackgroundableOperation::LanQrPairingExchange);
        lifecycle.mark_checkpoint().unwrap();
        assert_eq!(lifecycle.phase(), LifecyclePhase::AtCheckpoint);
        assert_eq!(lifecycle.request_suspend(), SuspendDecision::SafeToSuspend);
        assert_eq!(lifecycle.phase(), LifecyclePhase::Suspended);

        lifecycle.resume_from_suspend().unwrap();
        assert_eq!(lifecycle.phase(), LifecyclePhase::AtCheckpoint);
        lifecycle.begin_next_step().unwrap();
        assert_eq!(lifecycle.phase(), LifecyclePhase::InFlight);
    }

    #[test]
    fn cannot_mark_checkpoint_twice_without_a_step_between() {
        let lifecycle = OperationLifecycle::begin(BackgroundableOperation::LanQrPairingExchange);
        lifecycle.mark_checkpoint().unwrap();
        assert_eq!(
            lifecycle.mark_checkpoint().unwrap_err(),
            LifecycleError::NotInFlight
        );
    }

    #[test]
    fn cannot_begin_next_step_while_already_in_flight() {
        let lifecycle = OperationLifecycle::begin(BackgroundableOperation::LanQrPairingExchange);
        assert_eq!(
            lifecycle.begin_next_step().unwrap_err(),
            LifecycleError::NotAtCheckpoint
        );
    }

    #[test]
    fn cannot_resume_when_not_suspended() {
        let lifecycle = OperationLifecycle::begin(BackgroundableOperation::LanQrPairingExchange);
        assert_eq!(
            lifecycle.resume_from_suspend().unwrap_err(),
            LifecycleError::NotSuspended
        );
        lifecycle.mark_checkpoint().unwrap();
        assert_eq!(
            lifecycle.resume_from_suspend().unwrap_err(),
            LifecycleError::NotSuspended
        );
    }

    #[test]
    fn fail_closed_from_in_flight_is_visible_and_terminal() {
        let lifecycle = OperationLifecycle::begin(BackgroundableOperation::BleBearerTransfer);
        lifecycle
            .fail_closed(LifecycleFailureReason::PeerDisconnected)
            .unwrap();
        assert_eq!(lifecycle.phase(), LifecyclePhase::FailedClosed);
        assert_eq!(
            lifecycle.failure_reason(),
            Some(LifecycleFailureReason::PeerDisconnected)
        );
    }

    #[test]
    fn fail_closed_from_checkpoint_and_from_suspended_both_work() {
        let at_checkpoint =
            OperationLifecycle::begin(BackgroundableOperation::LanQrPairingExchange);
        at_checkpoint.mark_checkpoint().unwrap();
        at_checkpoint
            .fail_closed(LifecycleFailureReason::Timeout)
            .unwrap();
        assert_eq!(at_checkpoint.phase(), LifecyclePhase::FailedClosed);

        let suspended = OperationLifecycle::begin(BackgroundableOperation::LanQrPairingExchange);
        suspended.mark_checkpoint().unwrap();
        suspended.request_suspend();
        suspended
            .fail_closed(LifecycleFailureReason::PlatformTerminated)
            .unwrap();
        assert_eq!(suspended.phase(), LifecyclePhase::FailedClosed);
    }

    #[test]
    fn terminal_phases_reject_every_further_transition() {
        let completed = OperationLifecycle::begin(BackgroundableOperation::LanQrPairingExchange);
        completed.complete().unwrap();
        assert_eq!(
            completed.mark_checkpoint().unwrap_err(),
            LifecycleError::AlreadyTerminal
        );
        assert_eq!(
            completed.begin_next_step().unwrap_err(),
            LifecycleError::AlreadyTerminal
        );
        assert_eq!(
            completed.resume_from_suspend().unwrap_err(),
            LifecycleError::AlreadyTerminal
        );
        assert_eq!(
            completed.complete().unwrap_err(),
            LifecycleError::AlreadyTerminal
        );
        assert_eq!(
            completed
                .fail_closed(LifecycleFailureReason::Timeout)
                .unwrap_err(),
            LifecycleError::AlreadyTerminal
        );
        // A terminal phase is always safe to "suspend" - there is nothing
        // left to protect.
        assert_eq!(completed.request_suspend(), SuspendDecision::SafeToSuspend);

        let failed = OperationLifecycle::begin(BackgroundableOperation::BleBearerTransfer);
        failed.fail_closed(LifecycleFailureReason::Timeout).unwrap();
        assert_eq!(
            failed.complete().unwrap_err(),
            LifecycleError::AlreadyTerminal
        );
        assert_eq!(failed.request_suspend(), SuspendDecision::SafeToSuspend);
    }

    #[test]
    fn cannot_complete_while_suspended() {
        let lifecycle = OperationLifecycle::begin(BackgroundableOperation::LanQrPairingExchange);
        lifecycle.mark_checkpoint().unwrap();
        lifecycle.request_suspend();
        assert_eq!(lifecycle.phase(), LifecyclePhase::Suspended);
        assert_eq!(
            lifecycle.complete().unwrap_err(),
            LifecycleError::NotInFlight
        );
    }

    #[test]
    fn full_two_step_exchange_reaches_completion() {
        let lifecycle = OperationLifecycle::begin(BackgroundableOperation::BleBearerTransfer);
        // Step 1: send first chunk.
        lifecycle.mark_checkpoint().unwrap();
        // App backgrounded here; safe.
        assert_eq!(lifecycle.request_suspend(), SuspendDecision::SafeToSuspend);
        lifecycle.resume_from_suspend().unwrap();
        // Step 2: send final chunk.
        lifecycle.begin_next_step().unwrap();
        lifecycle.complete().unwrap();
        assert_eq!(lifecycle.phase(), LifecyclePhase::Completed);
        assert_eq!(lifecycle.request_suspend(), SuspendDecision::SafeToSuspend);
    }
}
