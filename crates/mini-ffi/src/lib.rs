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

use zeroize::Zeroize;

mod lifecycle;
pub use lifecycle::{
    BackgroundableOperation, LifecycleError, LifecycleFailureReason, LifecyclePhase,
    OperationLifecycle, SuspendDecision,
};

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
    /// Decrypted persisted-state bytes are malformed, truncated, or exceed
    /// a declared bound — never trusted enough to even attempt decoding
    /// further.
    CorruptState,
    /// The caller-supplied [`StorageCipher`] failed to encrypt or decrypt.
    Storage,
    /// `begin_device_enrollment` was called while an enrollment was already
    /// pending in this process.
    EnrollmentAlreadyPending,
    /// `finish_device_enrollment` was called with no enrollment pending.
    NoPendingEnrollment,
    /// A device enrollment request's KEL names a different root as its
    /// delegator than the one presenting this root's approval.
    DelegatorMismatch,
}

impl core::fmt::Display for RootError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RootError::RootAlreadyExists => f.write_str("a root already exists"),
            RootError::NoRoot => f.write_str("no root exists yet"),
            RootError::UnknownDevice => f.write_str("unknown or already-revoked device"),
            RootError::Identity(msg) => write!(f, "identity operation failed: {msg}"),
            RootError::CorruptState => f.write_str("persisted state is malformed"),
            RootError::Storage => f.write_str("storage cipher operation failed"),
            RootError::EnrollmentAlreadyPending => {
                f.write_str("a device enrollment is already pending")
            }
            RootError::NoPendingEnrollment => f.write_str("no device enrollment is pending"),
            RootError::DelegatorMismatch => {
                f.write_str("the enrollment request does not name this root as its delegator")
            }
        }
    }
}

impl std::error::Error for RootError {}

fn identity_err(err: did_mini::IdentityError) -> RootError {
    RootError::Identity(err.to_string())
}

/// Failure reported by a caller-implemented [`StorageCipher`].
///
/// Real causes (a revoked Android Keystore entry, a corrupted ciphertext,
/// an AEAD tag mismatch) are platform-specific detail that `mini-ffi` has
/// no use for beyond "did this succeed" — so, deliberately, this carries no
/// message, matching [`RootError`]'s general shape but without wrapping a
/// caller-supplied string across the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageCipherError {
    /// Encryption or decryption failed.
    Failed,
}

impl core::fmt::Display for StorageCipherError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("storage cipher operation failed")
    }
}

impl std::error::Error for StorageCipherError {}

/// Caller-implemented encrypt/decrypt boundary for persisting [`RootCore`]
/// state (issue #198, following D-0337's `Controller::restore`).
///
/// `mini-ffi` never encrypts or decrypts anything itself, holds no storage
/// key, and never touches disk. On Android, the intended implementation
/// wraps Android Keystore's hardware- or software-backed AES-GCM `Cipher`:
/// `encrypt`/`decrypt` cross the UniFFI boundary as ordinary `Vec<u8>`
/// arguments, so the plaintext byte buffer necessarily exists on the
/// Kotlin/JVM side for the duration of the call — this crate cannot
/// zeroize memory it does not own past that point. What it *can* and does
/// zeroize is its own local plaintext copy immediately after
/// [`RootCore::restore`] decodes it (see that method).
pub trait StorageCipher: Send + Sync {
    /// Encrypt `plaintext`, returning an opaque ciphertext blob.
    fn encrypt(&self, plaintext: Vec<u8>) -> Result<Vec<u8>, StorageCipherError>;
    /// Decrypt a blob previously returned by `encrypt`.
    fn decrypt(&self, ciphertext: Vec<u8>) -> Result<Vec<u8>, StorageCipherError>;
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
    /// This process's own not-yet-confirmed delegated device identity,
    /// while it is enrolling against a root held by a *different* device
    /// (issue #199) — distinct from `devices`, which holds secret
    /// controllers for devices *this* process created and fully custodies
    /// (the single-process convenience path from D-0335). Not persisted by
    /// `persist_state`/`restore` (D-0338): an interrupted enrollment is
    /// meant to be retried, not silently resumed from disk.
    pending_enrollment: Option<did_mini::Controller>,
}

/// Marks the persisted-state plaintext format; bumped if the layout ever
/// changes so a future `RootCore` can reject bytes it no longer understands
/// instead of misparsing them.
const PERSIST_MAGIC: [u8; 4] = *b"MFP1";
const PERSIST_VERSION: u8 = 1;
/// Generous but bounded — a corrupted or hostile ciphertext must not drive
/// unbounded allocation once decrypted, the same discipline `did-mini`'s
/// own `Kel::from_bytes` already applies to its inputs.
const MAX_PERSISTED_DEVICES: usize = 256;
const MAX_PERSISTED_KEL_BYTES: usize = 1 << 20;
/// Matches `did-mini::limits`'s own `MAX_KEYS`/`MAX_NEXT` ceiling for one
/// identity's key set, even though `RootCore`'s MVP only ever creates
/// 1-of-1 identities today.
const MAX_PERSISTED_KEYS_PER_RECORD: usize = 32;

/// Minimal bounds-checked cursor over decrypted persisted-state bytes.
/// Deliberately not a generic serialization framework — the format above
/// is small and fixed, so a hand-rolled reader (matching this workspace's
/// existing preference for hand-rolled encodings over adding a dependency)
/// is simpler than pulling one in for four field types.
struct PersistReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> PersistReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], RootError> {
        let end = self.pos.checked_add(n).ok_or(RootError::CorruptState)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(RootError::CorruptState)?;
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, RootError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, RootError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn seed(&mut self) -> Result<[u8; 32], RootError> {
        let b = self.take(32)?;
        let mut seed = [0u8; 32];
        seed.copy_from_slice(b);
        Ok(seed)
    }

    fn finished(&self) -> bool {
        self.pos == self.bytes.len()
    }
}

fn encode_identity_record(out: &mut Vec<u8>, controller: &did_mini::Controller) {
    let kel_bytes = controller.kel().to_bytes();
    out.extend_from_slice(&(kel_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&kel_bytes);

    let (current, next) = controller.export_current_and_next_keys_for_storage();
    out.extend_from_slice(&(current.len() as u32).to_le_bytes());
    for key in &current {
        out.extend_from_slice(&key.to_seed_bytes());
    }
    out.extend_from_slice(&(next.len() as u32).to_le_bytes());
    for key in &next {
        out.extend_from_slice(&key.to_seed_bytes());
    }
}

fn decode_identity_record(r: &mut PersistReader<'_>) -> Result<did_mini::Controller, RootError> {
    let kel_len = r.u32()? as usize;
    if kel_len > MAX_PERSISTED_KEL_BYTES {
        return Err(RootError::CorruptState);
    }
    let kel = did_mini::Kel::from_bytes(r.take(kel_len)?).map_err(identity_err)?;

    let current_count = r.u32()? as usize;
    if current_count == 0 || current_count > MAX_PERSISTED_KEYS_PER_RECORD {
        return Err(RootError::CorruptState);
    }
    let mut current = Vec::with_capacity(current_count);
    for _ in 0..current_count {
        current.push(mini_crypto::SigningKey::from_seed(&r.seed()?));
    }

    let next_count = r.u32()? as usize;
    if next_count == 0 || next_count > MAX_PERSISTED_KEYS_PER_RECORD {
        return Err(RootError::CorruptState);
    }
    let mut next = Vec::with_capacity(next_count);
    for _ in 0..next_count {
        next.push(mini_crypto::SigningKey::from_seed(&r.seed()?));
    }

    did_mini::Controller::restore(&kel, current, next).map_err(identity_err)
}

fn encode_state(state: &RootState) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&PERSIST_MAGIC);
    out.push(PERSIST_VERSION);
    match &state.root {
        Some(root) => {
            out.push(1);
            encode_identity_record(&mut out, root);
        }
        None => out.push(0),
    }
    out.extend_from_slice(&(state.devices.len() as u32).to_le_bytes());
    for device in &state.devices {
        encode_identity_record(&mut out, device);
    }
    out
}

fn decode_state(bytes: &[u8]) -> Result<RootState, RootError> {
    let mut r = PersistReader::new(bytes);
    if r.take(PERSIST_MAGIC.len())? != PERSIST_MAGIC {
        return Err(RootError::CorruptState);
    }
    if r.u8()? != PERSIST_VERSION {
        return Err(RootError::CorruptState);
    }
    let root = match r.u8()? {
        0 => None,
        1 => Some(decode_identity_record(&mut r)?),
        _ => return Err(RootError::CorruptState),
    };
    let device_count = r.u32()? as usize;
    if device_count > MAX_PERSISTED_DEVICES {
        return Err(RootError::CorruptState);
    }
    let mut devices = Vec::with_capacity(device_count);
    for _ in 0..device_count {
        devices.push(decode_identity_record(&mut r)?);
    }
    if !r.finished() {
        return Err(RootError::CorruptState);
    }
    Ok(RootState {
        root,
        devices,
        pending_enrollment: None,
    })
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

    /// This root's own current KEL bytes — no secrets, safe to hand to any
    /// party (the enrolling device, a sync peer, an observer checking
    /// delegation status). Requires a root to already exist in this
    /// process.
    pub fn root_kel(&self) -> Result<Vec<u8>, RootError> {
        self.lock()
            .root
            .as_ref()
            .map(|root| root.kel().to_bytes())
            .ok_or(RootError::NoRoot)
    }

    /// Device-side, issue #199: begin enrolling *this* process as a new
    /// delegated device of `root_did` — a root controlled by a *different*
    /// device or process. Generates a fresh local device identity and
    /// returns its public KEL bytes: self-certifying proof of the device's
    /// own identity and its claimed delegator, containing no secret
    /// material, for the caller to send to the root holder over any
    /// transport (LAN/QR/BLE — slice 4/5, out of scope here; this method
    /// assumes an already-established channel and only produces the bytes
    /// that travel over it).
    ///
    /// Fails if an enrollment is already pending in this process; finish
    /// or effectively abandon (a fresh `RootCore`) the existing one first.
    pub fn begin_device_enrollment(&self, root_did: String) -> Result<Vec<u8>, RootError> {
        let mut state = self.lock();
        if state.pending_enrollment.is_some() {
            return Err(RootError::EnrollmentAlreadyPending);
        }
        let root_did = did_mini::Did::parse(&root_did).map_err(identity_err)?;
        let current = mini_crypto::SigningKey::generate().map_err(|e| identity_err(e.into()))?;
        let next = mini_crypto::SigningKey::generate().map_err(|e| identity_err(e.into()))?;
        let device =
            did_mini::Controller::incept_device(&root_did, vec![current], 1, vec![next], 1)
                .map_err(identity_err)?;
        let request = device.kel().to_bytes();
        state.pending_enrollment = Some(device);
        Ok(request)
    }

    /// Root-side, issue #199: approve a device enrollment request (raw KEL
    /// bytes from [`RootCore::begin_device_enrollment`]), delegating it on
    /// this root's own KEL with the default (primary) capability set.
    /// Returns the root's now-updated KEL bytes — the confirmation the
    /// candidate device needs to finish enrollment and later prove or
    /// observe its own delegated status.
    ///
    /// The request's KEL must verify on its own and must name this root as
    /// its delegator; a request built against a different root is rejected
    /// with [`RootError::DelegatorMismatch`] rather than silently
    /// delegated. Requires a root to already exist in this process.
    pub fn approve_device_enrollment(&self, request: Vec<u8>) -> Result<Vec<u8>, RootError> {
        let mut state = self.lock();
        let root = state.root.as_mut().ok_or(RootError::NoRoot)?;
        let device_kel = did_mini::Kel::from_bytes(&request).map_err(identity_err)?;
        device_kel.verify().map_err(identity_err)?;
        if device_kel.delegator() != Some(root.did()) {
            return Err(RootError::DelegatorMismatch);
        }
        root.delegate_device(&device_kel.did(), did_mini::Capabilities::primary())
            .map_err(identity_err)?;
        Ok(root.kel().to_bytes())
    }

    /// Device-side, issue #199: finish enrollment using the root's approval
    /// bytes ([`RootCore::approve_device_enrollment`]'s return value).
    /// Verifies the mutual delegation link
    /// ([`did_mini::verify_delegation`]) before promoting the pending
    /// device identity into this core's confirmed device list — approval
    /// bytes that don't actually authorize *this* device (wrong root,
    /// stale KEL missing the delegation, tampered bytes) are rejected,
    /// leaving the pending enrollment untouched rather than silently
    /// promoting an unconfirmed identity. Returns the device's own
    /// `did:mini:...` string on success.
    pub fn finish_device_enrollment(&self, approval: Vec<u8>) -> Result<String, RootError> {
        let mut state = self.lock();
        let device = state
            .pending_enrollment
            .as_ref()
            .ok_or(RootError::NoPendingEnrollment)?;
        let root_kel = did_mini::Kel::from_bytes(&approval).map_err(identity_err)?;
        did_mini::verify_delegation(&root_kel, &device.kel()).map_err(identity_err)?;
        let device_did = device.did().as_str().to_string();
        let device = state.pending_enrollment.take().expect("checked above");
        state.devices.push(device);
        Ok(device_did)
    }

    /// Root-side, issue #199: revoke a delegated device this root
    /// authorized elsewhere (e.g. via [`RootCore::approve_device_enrollment`]
    /// on a *different* process/device than the one that created it) —
    /// unlike [`RootCore::revoke_device`], this does not require the
    /// device's secret controller to be held locally, since revocation is
    /// entirely the root's own unilateral act on its own KEL and never
    /// needed the device's cooperation or secrets in the first place.
    pub fn revoke_delegated_device(&self, device_did: String) -> Result<(), RootError> {
        let mut state = self.lock();
        let did = did_mini::Did::parse(&device_did).map_err(identity_err)?;
        state
            .root
            .as_mut()
            .ok_or(RootError::NoRoot)?
            .revoke_device(&did)
            .map_err(identity_err)?;
        state.devices.retain(|d| d.did() != did);
        Ok(())
    }

    /// Reconstruct a `RootCore` from a ciphertext blob previously produced
    /// by [`RootCore::persist_state`], resuming exactly where the process
    /// left off — no rotation, no appended event, matching
    /// [`did_mini::Controller::restore`]'s own semantics (D-0337). `cipher`
    /// performs the actual decryption (e.g. Android Keystore-backed
    /// AES-GCM); this method never sees or handles a storage key itself.
    ///
    /// The decrypted plaintext is decoded immediately and zeroized
    /// afterward regardless of whether decoding succeeded — a malformed
    /// blob still had real secret seed bytes in it up to the point
    /// decoding rejected it.
    pub fn restore(ciphertext: Vec<u8>, cipher: Box<dyn StorageCipher>) -> Result<Self, RootError> {
        let mut plaintext = cipher.decrypt(ciphertext).map_err(|_| RootError::Storage)?;
        let decoded = decode_state(&plaintext);
        plaintext.zeroize();
        Ok(Self {
            state: std::sync::Mutex::new(decoded?),
        })
    }

    /// Encrypt this core's current root/device state into an opaque blob
    /// for the caller to write to app-private storage. `cipher` performs
    /// the actual encryption; this method only builds the plaintext
    /// structure and hands it to `cipher.encrypt`, never writing it
    /// anywhere itself.
    ///
    /// The plaintext necessarily crosses the UniFFI boundary as `cipher`'s
    /// argument — it must, since `cipher` is what encrypts it — so it
    /// briefly exists in Kotlin/JVM memory this crate cannot reach to
    /// zeroize; see [`StorageCipher`]'s own doc comment.
    pub fn persist_state(&self, cipher: Box<dyn StorageCipher>) -> Result<Vec<u8>, RootError> {
        let plaintext = encode_state(&self.lock());
        cipher.encrypt(plaintext).map_err(|_| RootError::Storage)
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

/// Issue #199: check whether `device_did` is currently an active delegated
/// device according to `root_kel_bytes` — lets any party (the root holder
/// checking its own state, or a delegated device checking on itself with a
/// freshly-fetched copy of the root's KEL) observe a revocation's effect
/// without needing a `RootCore` instance at all, since this is a pure
/// query over public KEL bytes.
///
/// **Freshness is the caller's problem**, the same documented limitation
/// [`did_mini::verify_delegation`] states: a stale root KEL from before a
/// revocation still shows the device as delegated. Callers must obtain the
/// freshest root KEL they can.
pub fn is_device_delegated(root_kel_bytes: Vec<u8>, device_did: String) -> Result<bool, RootError> {
    let root_kel = did_mini::Kel::from_bytes(&root_kel_bytes).map_err(identity_err)?;
    root_kel.verify().map_err(identity_err)?;
    let did = did_mini::Did::parse(&device_did).map_err(identity_err)?;
    Ok(root_kel
        .delegated_devices()
        .into_iter()
        .any(|(d, _)| d == did))
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

    /// A trivial reversible "cipher" (XOR with a fixed keystream byte) —
    /// stands in for a real Android Keystore AES-GCM `Cipher` implementation
    /// just to exercise the encrypt/decrypt plumbing, not to test any real
    /// cryptography (`mini-ffi` never implements the cipher itself).
    #[derive(Debug)]
    struct XorCipher(u8);

    impl StorageCipher for XorCipher {
        fn encrypt(&self, plaintext: Vec<u8>) -> Result<Vec<u8>, StorageCipherError> {
            Ok(plaintext.into_iter().map(|b| b ^ self.0).collect())
        }
        fn decrypt(&self, ciphertext: Vec<u8>) -> Result<Vec<u8>, StorageCipherError> {
            Ok(ciphertext.into_iter().map(|b| b ^ self.0).collect())
        }
    }

    /// Always fails, simulating a revoked Keystore entry or a tag mismatch.
    #[derive(Debug)]
    struct FailingCipher;

    impl StorageCipher for FailingCipher {
        fn encrypt(&self, _plaintext: Vec<u8>) -> Result<Vec<u8>, StorageCipherError> {
            Err(StorageCipherError::Failed)
        }
        fn decrypt(&self, _ciphertext: Vec<u8>) -> Result<Vec<u8>, StorageCipherError> {
            Err(StorageCipherError::Failed)
        }
    }

    #[test]
    fn persisting_and_restoring_round_trips_root_and_devices() {
        let core = RootCore::new();
        let root_did = core.create_root().unwrap();
        let device_a = core.create_device().unwrap();
        let device_b = core.create_device().unwrap();

        let blob = core.persist_state(Box::new(XorCipher(0x5A))).unwrap();
        let restored = RootCore::restore(blob, Box::new(XorCipher(0x5A))).unwrap();

        assert!(restored.has_root());
        assert_eq!(restored.root_did(), Some(root_did));
        let mut devices = restored.delegated_devices();
        devices.sort();
        let mut expected = vec![device_a, device_b];
        expected.sort();
        assert_eq!(devices, expected);
    }

    #[test]
    fn a_restored_core_can_create_and_revoke_devices_normally() {
        let core = RootCore::new();
        core.create_root().unwrap();
        core.create_device().unwrap();

        let blob = core.persist_state(Box::new(XorCipher(0x11))).unwrap();
        let restored = RootCore::restore(blob, Box::new(XorCipher(0x11))).unwrap();

        // Not read-only: the restored core is fully functional.
        let new_device = restored.create_device().unwrap();
        assert_eq!(restored.delegated_devices().len(), 2);
        restored.revoke_device(new_device).unwrap();
        assert_eq!(restored.delegated_devices().len(), 1);
    }

    #[test]
    fn persisting_with_no_root_then_restoring_has_no_root() {
        let core = RootCore::new();
        let blob = core.persist_state(Box::new(XorCipher(0x42))).unwrap();
        let restored = RootCore::restore(blob, Box::new(XorCipher(0x42))).unwrap();
        assert!(!restored.has_root());
        assert!(restored.delegated_devices().is_empty());
    }

    #[test]
    fn a_failing_cipher_surfaces_as_a_storage_error() {
        let core = RootCore::new();
        core.create_root().unwrap();
        assert_eq!(
            core.persist_state(Box::new(FailingCipher)),
            Err(RootError::Storage)
        );
        assert_eq!(
            RootCore::restore(vec![1, 2, 3], Box::new(FailingCipher)).unwrap_err(),
            RootError::Storage
        );
    }

    #[test]
    fn restoring_with_the_wrong_cipher_key_fails_closed() {
        let core = RootCore::new();
        core.create_root().unwrap();
        let blob = core.persist_state(Box::new(XorCipher(0x5A))).unwrap();

        // A different key XOR-"decrypts" into garbage that is not the
        // magic/version header the real format starts with.
        assert_eq!(
            RootCore::restore(blob, Box::new(XorCipher(0x99))).unwrap_err(),
            RootError::CorruptState
        );
    }

    #[test]
    fn restoring_truncated_plaintext_fails_closed() {
        let core = RootCore::new();
        core.create_root().unwrap();
        let mut blob = core.persist_state(Box::new(XorCipher(0x00))).unwrap();
        blob.truncate(blob.len() / 2);
        assert_eq!(
            RootCore::restore(blob, Box::new(XorCipher(0x00))).unwrap_err(),
            RootError::CorruptState
        );
    }

    #[test]
    fn restoring_empty_bytes_fails_closed() {
        assert_eq!(
            RootCore::restore(Vec::new(), Box::new(XorCipher(0x00))).unwrap_err(),
            RootError::CorruptState
        );
    }

    // -- Issue #199: device enrollment/revocation multi-device flow. Two
    // separate `RootCore` instances stand in for two separate devices/
    // processes; only public bytes (KEL blobs) ever pass between them,
    // matching the acceptance test's "without device A transmitting root
    // key bytes to device B" requirement. Slice 4/5 (LAN/QR/BLE transport)
    // are out of scope -- these tests hand the bytes directly, simulating
    // an already-established channel.

    #[test]
    fn the_full_enrollment_and_revocation_acceptance_flow() {
        // 1. Root already exists on device A.
        let device_a = RootCore::new();
        let root_did = device_a.create_root().unwrap();

        // 2. Device B enrolls against the same root -- device A never
        // transmits root key bytes to device B; only the device's own
        // public KEL and the root's own public KEL cross the "channel".
        let device_b = RootCore::new();
        let request = device_b.begin_device_enrollment(root_did.clone()).unwrap();
        let approval = device_a.approve_device_enrollment(request).unwrap();
        let device_b_did = device_b.finish_device_enrollment(approval).unwrap();

        // Device A never held device B's secret controller, so the
        // enrollment is confirmed via the root's own public KEL -- not
        // device A's local secret-holding `delegated_devices()` list,
        // which correctly stays empty here (see
        // `revoke_delegated_device_does_not_require_holding_the_devices_secrets`).
        let root_kel_after_enroll = device_a.root_kel().unwrap();
        assert!(is_device_delegated(root_kel_after_enroll, device_b_did.clone()).unwrap());

        // 3. Device A revokes device B's delegation.
        device_a
            .revoke_delegated_device(device_b_did.clone())
            .unwrap();

        // 4. Device B can no longer act for the root after revocation, and
        // device A observes the revocation took effect via the same
        // portable KEL-bytes query.
        let root_kel_after_revoke = device_a.root_kel().unwrap();
        assert!(!is_device_delegated(root_kel_after_revoke, device_b_did).unwrap());
    }

    #[test]
    fn a_second_enrollment_cannot_begin_while_one_is_pending() {
        let some_root = did_mini::Controller::incept_single().unwrap();
        let root_did = some_root.did().as_str().to_string();
        let device_b = RootCore::new();
        device_b.begin_device_enrollment(root_did.clone()).unwrap();
        assert_eq!(
            device_b.begin_device_enrollment(root_did),
            Err(RootError::EnrollmentAlreadyPending)
        );
    }

    #[test]
    fn begin_device_enrollment_rejects_a_malformed_root_did() {
        let device_b = RootCore::new();
        assert!(matches!(
            device_b.begin_device_enrollment("not-a-did".to_string()),
            Err(RootError::Identity(_))
        ));
    }

    #[test]
    fn approve_device_enrollment_requires_a_root() {
        let device_a = RootCore::new();
        let device_b = RootCore::new();
        // No root on device_a yet; use device_a's own DID as a stand-in
        // target since begin_device_enrollment only needs a syntactically
        // valid DID string.
        let placeholder_root = did_mini::Controller::incept_single().unwrap();
        let request = device_b
            .begin_device_enrollment(placeholder_root.did().as_str().to_string())
            .unwrap();
        assert_eq!(
            device_a.approve_device_enrollment(request),
            Err(RootError::NoRoot)
        );
    }

    #[test]
    fn approve_device_enrollment_rejects_a_request_naming_a_different_root() {
        let device_a = RootCore::new();
        device_a.create_root().unwrap();

        let some_other_root = did_mini::Controller::incept_single().unwrap();
        let device_b = RootCore::new();
        let request = device_b
            .begin_device_enrollment(some_other_root.did().as_str().to_string())
            .unwrap();

        assert_eq!(
            device_a.approve_device_enrollment(request),
            Err(RootError::DelegatorMismatch)
        );
    }

    #[test]
    fn approve_device_enrollment_rejects_a_corrupt_request() {
        let device_a = RootCore::new();
        device_a.create_root().unwrap();
        assert!(matches!(
            device_a.approve_device_enrollment(vec![1, 2, 3]),
            Err(RootError::Identity(_))
        ));
    }

    #[test]
    fn finish_device_enrollment_fails_without_a_pending_enrollment() {
        let device_b = RootCore::new();
        assert_eq!(
            device_b.finish_device_enrollment(vec![1, 2, 3]),
            Err(RootError::NoPendingEnrollment)
        );
    }

    #[test]
    fn finish_device_enrollment_rejects_an_approval_that_never_happened() {
        let device_a = RootCore::new();
        let root_did = device_a.create_root().unwrap();

        let device_b = RootCore::new();
        device_b.begin_device_enrollment(root_did).unwrap();

        // Device A's root KEL, but the root never actually approved this
        // device -- verify_delegation must reject it, not silently
        // promote an unconfirmed identity.
        let unapproved_root_kel = device_a.root_kel().unwrap();
        assert!(matches!(
            device_b.finish_device_enrollment(unapproved_root_kel),
            Err(RootError::Identity(_))
        ));
        // The pending enrollment survives the rejection; a caller can
        // retry with a real approval afterward.
        assert!(device_b.finish_device_enrollment(vec![9, 9, 9]).is_err());
    }

    #[test]
    fn finish_device_enrollment_rejects_a_stale_root_kel_from_before_approval() {
        let device_a = RootCore::new();
        let root_did = device_a.create_root().unwrap();
        let stale_root_kel = device_a.root_kel().unwrap();

        let device_b = RootCore::new();
        let request = device_b.begin_device_enrollment(root_did).unwrap();
        device_a.approve_device_enrollment(request).unwrap();

        // A copy of the root KEL taken *before* the approval was appended
        // does not yet show the delegation -- must be rejected, not
        // silently accepted as "close enough".
        assert!(device_b.finish_device_enrollment(stale_root_kel).is_err());
    }

    #[test]
    fn revoke_delegated_device_does_not_require_holding_the_devices_secrets() {
        // The whole point of issue #199: device A never held device B's
        // secret controller (unlike the single-process `create_device`
        // convenience path), yet can still revoke it directly from its own
        // root KEL.
        let device_a = RootCore::new();
        let root_did = device_a.create_root().unwrap();
        let device_b = RootCore::new();
        let request = device_b.begin_device_enrollment(root_did).unwrap();
        let approval = device_a.approve_device_enrollment(request).unwrap();
        let device_b_did = device_b.finish_device_enrollment(approval).unwrap();

        // device_a.delegated_devices() (the local secret-holding list) has
        // never heard of device_b -- confirming revocation truly does not
        // depend on it.
        assert!(device_a.delegated_devices().is_empty());
        device_a
            .revoke_delegated_device(device_b_did.clone())
            .unwrap();
        assert!(!is_device_delegated(device_a.root_kel().unwrap(), device_b_did).unwrap());
    }

    #[test]
    fn revoke_delegated_device_requires_a_root() {
        let device_a = RootCore::new();
        let stranger = did_mini::Controller::incept_single().unwrap();
        assert_eq!(
            device_a.revoke_delegated_device(stranger.did().as_str().to_string()),
            Err(RootError::NoRoot)
        );
    }

    #[test]
    fn root_kel_requires_a_root() {
        let device_a = RootCore::new();
        assert_eq!(device_a.root_kel(), Err(RootError::NoRoot));
    }

    #[test]
    fn is_device_delegated_rejects_a_tampered_kel() {
        let device_a = RootCore::new();
        device_a.create_root().unwrap();
        let mut kel_bytes = device_a.root_kel().unwrap();
        let flip_at = kel_bytes.len() / 2;
        kel_bytes[flip_at] ^= 0xFF;
        assert!(matches!(
            is_device_delegated(kel_bytes, "did:mini:doesnotmatter".to_string()),
            Err(RootError::Identity(_))
        ));
    }

    #[test]
    fn is_device_delegated_is_false_for_an_unrelated_did() {
        let device_a = RootCore::new();
        device_a.create_root().unwrap();
        let root_kel = device_a.root_kel().unwrap();
        let stranger = did_mini::Controller::incept_single().unwrap();
        assert!(!is_device_delegated(root_kel, stranger.did().as_str().to_string()).unwrap());
    }
}
