//! The versioned application boundary shared by Mininet's thin mobile shells.
//!
//! This first slice is intentionally a pure command/event reducer. Kotlin and
//! Swift render semantic state and adapt platform facilities; they do not own
//! identity, social, sync, or authorization rules. No mutable Rust object is
//! shared across the FFI boundary, so calls are deterministic and independently
//! testable.
//!
//! Maturity: **prototype foundation**. [`RootCore`] can now create a root
//! identity and delegate/revoke device identities in memory (D-0335), but
//! nothing persists across process death yet, no key is Android
//! Keystore-backed or hardware-proven, and no peer synchronization exists.
//! Those capabilities enter through later, separately reviewed adapters;
//! this API refuses to imply that they already exist.

#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]

/// Version of the typed command/event API.
pub const APP_API_VERSION: u32 = 0;
const MAX_REQUEST_ID_BYTES: usize = 64;

/// Security facilities reported by the platform shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformCapabilities {
    /// The platform offers non-exportable application key storage.
    pub secure_key_storage: bool,
    /// The secure storage reports hardware backing.
    pub hardware_backed_keys: bool,
    /// The device currently has a screen lock configured.
    pub screen_lock: bool,
    /// A biometric may unlock an already-protected local key.
    pub biometric_unlock: bool,
}

/// Honest key-custody readiness shown during onboarding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityReadiness {
    /// No secure key-storage adapter is available; root creation stays blocked.
    SecureStorageUnavailable,
    /// Secure storage exists but does not report hardware backing.
    SoftwareProtected,
    /// Secure storage reports hardware backing.
    HardwareProtected,
}

/// The implemented portion of the first-run experience.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingStage {
    /// Explain that the user, not an account server, controls the identity.
    Welcome,
    /// Explain root/device separation and recovery before creating anything.
    RootSafety,
    /// The platform is ready, but root creation is not wired in this slice.
    RootCreationReady,
}

/// Complete render state returned by the Rust core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSnapshot {
    /// API version that produced this state.
    pub api_version: u32,
    /// Monotonic local UI-state generation.
    pub generation: u64,
    /// Current onboarding stage.
    pub onboarding_stage: OnboardingStage,
    /// Key-storage readiness derived by the Rust core.
    pub security_readiness: SecurityReadiness,
    /// Platform-reported screen-lock state.
    pub screen_lock: bool,
    /// Platform-reported biometric availability.
    pub biometric_unlock: bool,
}

/// Semantic user action sent by a platform shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    /// Re-render the current state without mutation.
    Refresh,
    /// Advance when the current safety gate allows it.
    Continue,
    /// Return to the previous onboarding explanation.
    Back,
}

/// One versioned command. Request identifiers are caller-generated correlation
/// labels, never identity or authorization tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppCommand {
    /// API version expected by the caller.
    pub api_version: u32,
    /// Short printable correlation label.
    pub request_id: String,
    /// Current platform security report, revalidated for every transition.
    pub capabilities: PlatformCapabilities,
    /// Requested semantic action.
    pub action: AppAction,
}

/// A semantic event for the shell to render or announce accessibly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEventKind {
    /// The snapshot was accepted or changed.
    SnapshotChanged,
    /// Root creation is blocked until secure key storage exists.
    SecureStorageRequired,
    /// The next root-creation adapter is intentionally not implemented yet.
    RootCreationPending,
}

/// Event correlated to the command that caused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEvent {
    /// Command correlation label.
    pub request_id: String,
    /// Semantic event kind.
    pub kind: AppEventKind,
}

/// Atomic command result: a complete new snapshot plus ordered events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutcome {
    /// State the shell must render after this command.
    pub snapshot: AppSnapshot,
    /// Ordered semantic events produced by the command.
    pub events: Vec<AppEvent>,
}

/// Stable failure classes exposed to every platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppError {
    /// Caller and core API versions differ.
    UnsupportedApiVersion,
    /// Platform capabilities contradict each other.
    InvalidPlatformCapabilities,
    /// Caller-supplied state contradicts the current platform report.
    InconsistentSnapshot,
    /// The request correlation label is malformed.
    InvalidRequest,
    /// The action is not allowed in the supplied state.
    InvalidTransition,
    /// The state generation cannot be advanced safely.
    GenerationOverflow,
}

impl core::fmt::Display for AppError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            AppError::UnsupportedApiVersion => "unsupported Mini application API version",
            AppError::InvalidPlatformCapabilities => "invalid platform security capabilities",
            AppError::InconsistentSnapshot => "application snapshot contradicts platform state",
            AppError::InvalidRequest => "invalid application request",
            AppError::InvalidTransition => "invalid onboarding transition",
            AppError::GenerationOverflow => "application state generation overflow",
        };
        f.write_str(message)
    }
}

impl std::error::Error for AppError {}

/// Stable failure classes for [`RootCore`]'s identity operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootError {
    /// `create_root` was called but a root already exists in this process.
    RootAlreadyExists,
    /// A device operation was requested before a root exists.
    NoRoot,
    /// `revoke_device` named a DID this `RootCore` never delegated.
    UnknownDevice,
    /// A `did-mini` identity operation failed; message only, no secret state.
    Identity(String),
}

impl core::fmt::Display for RootError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RootError::RootAlreadyExists => f.write_str("a root already exists"),
            RootError::NoRoot => f.write_str("no root exists yet"),
            RootError::UnknownDevice => f.write_str("unknown or already-revoked device"),
            RootError::Identity(msg) => write!(f, "identity operation failed: {msg}"),
        }
    }
}

impl std::error::Error for RootError {}

fn identity_err(err: did_mini::IdentityError) -> RootError {
    RootError::Identity(err.to_string())
}

/// In-memory root and delegated-device identity state for one app process.
///
/// **Software-custody MVP (D-0335):** every key here is an ordinary
/// in-memory `mini_crypto::SigningKey`, exactly like every other identity in
/// this workspace today. No key is Android Keystore-backed, no key is
/// hardware-proven non-exportable, and nothing here persists past process
/// death — closing the app loses the root and every delegated device.
/// `docs/design/android-keystore-signer-adapter.md` (D-0334) named this as
/// the pragmatic first slice specifically because it changes nothing in
/// `mini-crypto`/`did-mini`; genuine hardware-backed custody is tracked
/// separately (hub issue #196) and is not implied by anything this type
/// does. Persistence across restart is the very next slice (issue #198).
#[derive(Debug, Default)]
pub struct RootCore {
    state: std::sync::Mutex<RootState>,
}

#[derive(Debug, Default)]
struct RootState {
    root: Option<did_mini::Controller>,
    devices: Vec<did_mini::Controller>,
}

impl RootCore {
    /// A fresh core with no root yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a root has been created in this process.
    pub fn has_root(&self) -> bool {
        self.lock().root.is_some()
    }

    /// The root's `did:mini:...` string, if one exists.
    pub fn root_did(&self) -> Option<String> {
        self.lock()
            .root
            .as_ref()
            .map(|c| c.did().as_str().to_string())
    }

    /// Create the root identity. Fails if one already exists in this process
    /// — recovery of an existing root is a separate, not-yet-implemented
    /// path (issue #198), never silently overwritten here.
    pub fn create_root(&self) -> Result<String, RootError> {
        let mut state = self.lock();
        if state.root.is_some() {
            return Err(RootError::RootAlreadyExists);
        }
        let controller = did_mini::Controller::incept_single().map_err(identity_err)?;
        let did = controller.did().as_str().to_string();
        state.root = Some(controller);
        Ok(did)
    }

    /// Generate a new delegated device identity and authorize it on the
    /// root's KEL with the default (primary) capability set. Returns the new
    /// device's `did:mini:...` string.
    pub fn create_device(&self) -> Result<String, RootError> {
        let mut state = self.lock();
        let root_did = state.root.as_ref().ok_or(RootError::NoRoot)?.did();
        let current = mini_crypto::SigningKey::generate().map_err(|e| identity_err(e.into()))?;
        let next = mini_crypto::SigningKey::generate().map_err(|e| identity_err(e.into()))?;
        let device =
            did_mini::Controller::incept_device(&root_did, vec![current], 1, vec![next], 1)
                .map_err(identity_err)?;
        let device_did = device.did();
        state
            .root
            .as_mut()
            .expect("checked above")
            .delegate_device(&device_did, did_mini::Capabilities::primary())
            .map_err(identity_err)?;
        let device_did_str = device_did.as_str().to_string();
        state.devices.push(device);
        Ok(device_did_str)
    }

    /// Revoke a previously delegated device by its DID string. The device
    /// must be one this core actually delegated; an unknown or
    /// already-revoked DID is rejected rather than silently accepted.
    pub fn revoke_device(&self, device_did: String) -> Result<(), RootError> {
        let mut state = self.lock();
        let did = did_mini::Did::parse(&device_did).map_err(identity_err)?;
        if !state.devices.iter().any(|d| d.did() == did) {
            return Err(RootError::UnknownDevice);
        }
        state
            .root
            .as_mut()
            .ok_or(RootError::NoRoot)?
            .revoke_device(&did)
            .map_err(identity_err)?;
        state.devices.retain(|d| d.did() != did);
        Ok(())
    }

    /// The DIDs of currently delegated (non-revoked) devices.
    pub fn delegated_devices(&self) -> Vec<String> {
        self.lock()
            .devices
            .iter()
            .map(|d| d.did().as_str().to_string())
            .collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, RootState> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

/// Return the command/event API version without constructing application state.
pub fn api_version() -> u32 {
    APP_API_VERSION
}

/// Create the deterministic first snapshot from explicit platform capabilities.
pub fn start(capabilities: PlatformCapabilities) -> Result<AppSnapshot, AppError> {
    validate_capabilities(&capabilities)?;
    Ok(AppSnapshot {
        api_version: APP_API_VERSION,
        generation: 0,
        onboarding_stage: OnboardingStage::Welcome,
        security_readiness: readiness(&capabilities),
        screen_lock: capabilities.screen_lock,
        biometric_unlock: capabilities.biometric_unlock,
    })
}

/// Reduce one typed command into a complete snapshot and ordered event list.
///
/// The function performs no I/O and holds no global or shared mutable state.
pub fn dispatch(snapshot: AppSnapshot, command: AppCommand) -> Result<CommandOutcome, AppError> {
    validate_version(snapshot.api_version)?;
    validate_version(command.api_version)?;
    validate_request_id(&command.request_id)?;
    validate_capabilities(&command.capabilities)?;
    validate_snapshot(&snapshot, &command.capabilities)?;

    let mut next = snapshot;
    let kind = match command.action {
        AppAction::Refresh => AppEventKind::SnapshotChanged,
        AppAction::Continue => match next.onboarding_stage {
            OnboardingStage::Welcome => {
                advance(&mut next, OnboardingStage::RootSafety)?;
                AppEventKind::SnapshotChanged
            }
            OnboardingStage::RootSafety => {
                if next.security_readiness == SecurityReadiness::SecureStorageUnavailable {
                    AppEventKind::SecureStorageRequired
                } else {
                    advance(&mut next, OnboardingStage::RootCreationReady)?;
                    AppEventKind::SnapshotChanged
                }
            }
            OnboardingStage::RootCreationReady => AppEventKind::RootCreationPending,
        },
        AppAction::Back => match next.onboarding_stage {
            OnboardingStage::Welcome => return Err(AppError::InvalidTransition),
            OnboardingStage::RootSafety => {
                advance(&mut next, OnboardingStage::Welcome)?;
                AppEventKind::SnapshotChanged
            }
            OnboardingStage::RootCreationReady => {
                advance(&mut next, OnboardingStage::RootSafety)?;
                AppEventKind::SnapshotChanged
            }
        },
    };

    Ok(CommandOutcome {
        snapshot: next,
        events: vec![AppEvent {
            request_id: command.request_id,
            kind,
        }],
    })
}

fn validate_capabilities(capabilities: &PlatformCapabilities) -> Result<(), AppError> {
    if capabilities.hardware_backed_keys && !capabilities.secure_key_storage {
        return Err(AppError::InvalidPlatformCapabilities);
    }
    if capabilities.biometric_unlock && !capabilities.screen_lock {
        return Err(AppError::InvalidPlatformCapabilities);
    }
    Ok(())
}

fn readiness(capabilities: &PlatformCapabilities) -> SecurityReadiness {
    if capabilities.hardware_backed_keys {
        SecurityReadiness::HardwareProtected
    } else if capabilities.secure_key_storage {
        SecurityReadiness::SoftwareProtected
    } else {
        SecurityReadiness::SecureStorageUnavailable
    }
}

fn validate_snapshot(
    snapshot: &AppSnapshot,
    capabilities: &PlatformCapabilities,
) -> Result<(), AppError> {
    if snapshot.security_readiness != readiness(capabilities)
        || snapshot.screen_lock != capabilities.screen_lock
        || snapshot.biometric_unlock != capabilities.biometric_unlock
    {
        Err(AppError::InconsistentSnapshot)
    } else {
        Ok(())
    }
}

fn validate_version(version: u32) -> Result<(), AppError> {
    if version == APP_API_VERSION {
        Ok(())
    } else {
        Err(AppError::UnsupportedApiVersion)
    }
}

fn validate_request_id(request_id: &str) -> Result<(), AppError> {
    let valid = !request_id.is_empty()
        && request_id.len() <= MAX_REQUEST_ID_BYTES
        && request_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(AppError::InvalidRequest)
    }
}

fn advance(snapshot: &mut AppSnapshot, stage: OnboardingStage) -> Result<(), AppError> {
    snapshot.generation = snapshot
        .generation
        .checked_add(1)
        .ok_or(AppError::GenerationOverflow)?;
    snapshot.onboarding_stage = stage;
    Ok(())
}

// UniFFI's generated ABI glue necessarily exports `no_mangle` symbols. Keep
// that exception inside one private module: all handwritten code above remains
// under `deny(unsafe_code)`.
#[allow(unsafe_code)]
mod ffi_scaffolding {
    use super::*;
    uniffi::include_scaffolding!("mini_ffi");
}

// UniFFI's derives address this generated marker through `crate::UniFfiTag`.
// Re-exporting the private-module marker keeps the unsafe export lint scoped
// without changing the generated ABI.
use ffi_scaffolding::UniFfiTag;

#[cfg(test)]
mod tests {
    use super::*;

    fn protected_platform() -> PlatformCapabilities {
        PlatformCapabilities {
            secure_key_storage: true,
            hardware_backed_keys: true,
            screen_lock: true,
            biometric_unlock: true,
        }
    }

    fn command(action: AppAction) -> AppCommand {
        AppCommand {
            api_version: APP_API_VERSION,
            request_id: "test-1".to_owned(),
            capabilities: protected_platform(),
            action,
        }
    }

    #[test]
    fn start_reports_an_honest_hardware_backed_state() {
        let snapshot = start(protected_platform()).unwrap();
        assert_eq!(snapshot.api_version, APP_API_VERSION);
        assert_eq!(snapshot.generation, 0);
        assert_eq!(snapshot.onboarding_stage, OnboardingStage::Welcome);
        assert_eq!(
            snapshot.security_readiness,
            SecurityReadiness::HardwareProtected
        );
    }

    #[test]
    fn contradictory_platform_capabilities_fail_closed() {
        let mut capabilities = protected_platform();
        capabilities.secure_key_storage = false;
        assert_eq!(
            start(capabilities),
            Err(AppError::InvalidPlatformCapabilities)
        );

        let mut capabilities = protected_platform();
        capabilities.screen_lock = false;
        assert_eq!(
            start(capabilities),
            Err(AppError::InvalidPlatformCapabilities)
        );
    }

    #[test]
    fn missing_secure_storage_blocks_root_creation_without_advancing() {
        let capabilities = PlatformCapabilities {
            secure_key_storage: false,
            hardware_backed_keys: false,
            screen_lock: false,
            biometric_unlock: false,
        };
        let snapshot = start(capabilities.clone()).unwrap();
        let mut continue_command = command(AppAction::Continue);
        continue_command.capabilities = capabilities;
        let safety = dispatch(snapshot, continue_command.clone()).unwrap();
        let blocked = dispatch(safety.snapshot.clone(), continue_command).unwrap();

        assert_eq!(blocked.snapshot, safety.snapshot);
        assert_eq!(blocked.events[0].kind, AppEventKind::SecureStorageRequired);
    }

    #[test]
    fn root_creation_is_named_as_pending_instead_of_faked() {
        let welcome = start(protected_platform()).unwrap();
        let safety = dispatch(welcome, command(AppAction::Continue)).unwrap();
        let ready = dispatch(safety.snapshot, command(AppAction::Continue)).unwrap();
        let pending = dispatch(ready.snapshot.clone(), command(AppAction::Continue)).unwrap();

        assert_eq!(
            pending.snapshot.onboarding_stage,
            OnboardingStage::RootCreationReady
        );
        assert_eq!(pending.snapshot, ready.snapshot);
        assert_eq!(pending.events[0].kind, AppEventKind::RootCreationPending);
    }

    #[test]
    fn bad_versions_and_request_ids_are_rejected() {
        let snapshot = start(protected_platform()).unwrap();
        let mut bad_version = command(AppAction::Refresh);
        bad_version.api_version = APP_API_VERSION + 1;
        assert_eq!(
            dispatch(snapshot.clone(), bad_version),
            Err(AppError::UnsupportedApiVersion)
        );

        for request_id in ["", "contains space", "line\nbreak"] {
            let mut malformed = command(AppAction::Refresh);
            malformed.request_id = request_id.to_owned();
            assert_eq!(
                dispatch(snapshot.clone(), malformed),
                Err(AppError::InvalidRequest)
            );
        }
    }

    #[test]
    fn caller_cannot_forge_a_stronger_security_snapshot() {
        let capabilities = PlatformCapabilities {
            secure_key_storage: false,
            hardware_backed_keys: false,
            screen_lock: false,
            biometric_unlock: false,
        };
        let mut snapshot = start(capabilities.clone()).unwrap();
        snapshot.security_readiness = SecurityReadiness::HardwareProtected;
        let mut forged = command(AppAction::Continue);
        forged.capabilities = capabilities;

        assert_eq!(
            dispatch(snapshot, forged),
            Err(AppError::InconsistentSnapshot)
        );
    }

    #[test]
    fn generation_overflow_fails_instead_of_wrapping() {
        let mut snapshot = start(protected_platform()).unwrap();
        snapshot.generation = u64::MAX;
        assert_eq!(
            dispatch(snapshot, command(AppAction::Continue)),
            Err(AppError::GenerationOverflow)
        );
    }

    #[test]
    fn reducer_is_deterministic_across_ten_thousand_calls() {
        let snapshot = start(protected_platform()).unwrap();
        let expected = dispatch(snapshot.clone(), command(AppAction::Refresh)).unwrap();
        for _ in 0..10_000 {
            assert_eq!(
                dispatch(snapshot.clone(), command(AppAction::Refresh)).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn back_at_welcome_is_an_invalid_transition() {
        let snapshot = start(protected_platform()).unwrap();
        assert_eq!(
            dispatch(snapshot, command(AppAction::Back)),
            Err(AppError::InvalidTransition)
        );
    }

    #[test]
    fn a_fresh_root_core_has_no_root() {
        let core = RootCore::new();
        assert!(!core.has_root());
        assert_eq!(core.root_did(), None);
        assert!(core.delegated_devices().is_empty());
    }

    #[test]
    fn creating_a_second_root_is_rejected() {
        let core = RootCore::new();
        core.create_root().unwrap();
        assert_eq!(core.create_root(), Err(RootError::RootAlreadyExists));
    }

    #[test]
    fn a_device_cannot_be_created_before_a_root_exists() {
        let core = RootCore::new();
        assert_eq!(core.create_device(), Err(RootError::NoRoot));
    }

    #[test]
    fn the_full_delegation_and_revocation_ceremony() {
        let core = RootCore::new();
        let root_did = core.create_root().unwrap();
        assert!(core.has_root());
        assert_eq!(core.root_did(), Some(root_did));

        let device_did = core.create_device().unwrap();
        assert_eq!(core.delegated_devices(), vec![device_did.clone()]);

        core.revoke_device(device_did.clone()).unwrap();
        assert!(core.delegated_devices().is_empty());

        // A revoked device cannot be revoked again.
        assert_eq!(
            core.revoke_device(device_did),
            Err(RootError::UnknownDevice)
        );
    }

    #[test]
    fn revoking_an_unknown_device_is_rejected() {
        let core = RootCore::new();
        core.create_root().unwrap();
        // A syntactically valid DID that this core never delegated.
        let stranger = did_mini::Controller::incept_single().unwrap();
        let stranger_did = stranger.did().as_str().to_string();
        assert_eq!(
            core.revoke_device(stranger_did),
            Err(RootError::UnknownDevice)
        );
    }

    #[test]
    fn revoking_a_malformed_did_is_rejected() {
        let core = RootCore::new();
        core.create_root().unwrap();
        assert!(matches!(
            core.revoke_device("not-a-did".to_string()),
            Err(RootError::Identity(_))
        ));
    }

    #[test]
    fn two_delegated_devices_have_independent_identities() {
        let core = RootCore::new();
        core.create_root().unwrap();
        let device_a = core.create_device().unwrap();
        let device_b = core.create_device().unwrap();
        assert_ne!(device_a, device_b);
        let mut devices = core.delegated_devices();
        devices.sort();
        let mut expected = vec![device_a, device_b];
        expected.sort();
        assert_eq!(devices, expected);
    }
}
