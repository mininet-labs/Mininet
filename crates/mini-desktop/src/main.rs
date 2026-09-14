//! Windows-first Mininet reference client shell.
//!
//! This UI deliberately has no analytics, remote configuration, background
//! fetch, embedded browser, or update executor. Those are security properties
//! of the shell, not merely settings displayed to the user. Network and
//! protocol integration should be added behind explicit local interfaces.

#![forbid(unsafe_code)]

mod conversation_state;

use conversation_state::ConversationRecord;
use did_mini::{Capabilities, Controller, Did};
use eframe::egui;
use mini_bearer::{Bearer, Initiator, Responder, TcpBearer};
use mini_media::{assemble, publish_media, read_manifest};
use mini_messaging::{scan as scan_messages, send as send_message, MessageDraft};
use mini_objects::{ObjectType, OpaqueRoute};
use mini_selftest::{Outcome as CheckOutcome, Report as SelfTestReport};
use mini_social::{
    comments, community_members, community_posts, feed, followers, following, known_profiles,
    publish_comment, publish_community, publish_community_post, publish_media_post, publish_post,
    publish_profile, publish_profile_details, publish_wall, resolve_community, resolve_post,
    resolve_profile, set_follow, set_membership, set_reaction, CommunityPost, FeedFilter, FeedItem,
    LocalProfileAnnouncer, LocalProfileScanner, MembershipMode, NearbyProfile, PublicProfileDraft,
    PublicProfileField, ReactionKind, VisibilityPolicy, MAX_LOCATION_BYTES, MAX_PROFILE_FIELDS,
    MAX_PROFILE_FIELD_LABEL_BYTES, MAX_PROFILE_FIELD_VALUE_BYTES,
};
use mini_store::{Backend, FsBackend, Store};
use mini_sync::{
    kel_carrier, sync_bidirectional, sync_private_route_bidirectional, KelCache, SyncRole,
};
use mini_windows_setup::{InstallOptions, RecordingShell, Setup, SetupStatus, WindowsShell};
use mini_windows_vault::{load_existing, load_or_create, load_user_data, save_user_data, SeedPair};
use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

const PEER_IO_TIMEOUT: Duration = Duration::from_secs(10);

// --- Design system: spacing, color roles, relative time, avatars ---------
//
// A small shared scale so every view below draws from the same numbers
// instead of each screen picking its own. `MininetApp::apply_theme` still
// owns the base egui `Visuals`; these are the additional roles and helpers
// every redesigned view reaches for.

const SPACE_XS: f32 = 4.0;
const SPACE_SM: f32 = 8.0;
const SPACE_MD: f32 = 12.0;
const SPACE_LG: f32 = 18.0;
const SPACE_XL: f32 = 28.0;

/// The one accent color, reused wherever something should read as
/// "primary". A single vivid blue, deliberately close to the X-style
/// visual language this pass adopts (one saturated accent against a near-
/// black "Dim" panel, not a muted developer-tool blue) -- buttons, pills,
/// and cards all agree with the rest of the chrome through this one
/// constant, so `apply_theme`'s `Visuals::widgets.active.bg_fill` is set
/// from this too, rather than drifting from it.
const COLOR_ACCENT: egui::Color32 = egui::Color32::from_rgb(29, 155, 240);
const COLOR_TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(230, 236, 244);
const COLOR_TEXT_SECONDARY: egui::Color32 = egui::Color32::from_rgb(148, 162, 180);
const COLOR_SUCCESS: egui::Color32 = egui::Color32::from_rgb(122, 202, 154);
const COLOR_ERROR: egui::Color32 = egui::Color32::from_rgb(226, 128, 116);
const COLOR_CARD_BG: egui::Color32 = egui::Color32::from_rgb(19, 25, 36);
const COLOR_CARD_BORDER: egui::Color32 = egui::Color32::from_rgb(36, 47, 62);

/// A distinct fallback color per identity, so two people with different
/// names get different avatar badges instead of an identical gray circle.
const AVATAR_PALETTE: [egui::Color32; 8] = [
    egui::Color32::from_rgb(214, 108, 108),
    egui::Color32::from_rgb(214, 158, 92),
    egui::Color32::from_rgb(190, 182, 90),
    egui::Color32::from_rgb(120, 186, 120),
    egui::Color32::from_rgb(92, 176, 176),
    egui::Color32::from_rgb(100, 140, 214),
    egui::Color32::from_rgb(150, 110, 200),
    egui::Color32::from_rgb(200, 110, 170),
];

/// A short, readable stand-in for a display name when no profile is
/// published yet: the DID is the real identity anchor, but the full
/// `did:mini:<scid>` string is too long to read as a name in a card.
fn short_scid(did: &Did) -> String {
    let scid = did.scid();
    if scid.len() > 12 {
        format!("{}…", &scid[..12])
    } else {
        scid.to_string()
    }
}

/// Which color a status-bar notice reads in.
///
/// `self.notice` is a plain `String` set from ~60 call sites across this
/// file, not a typed `Result` -- turning every one of those into a proper
/// severity would be a much larger change than this pass's scope. This is
/// a keyword heuristic over the message text instead: honest about being
/// approximate, but enough to make "Could not..." visually read as a
/// problem instead of identical gray to everything else.
fn notice_color(notice: &str) -> egui::Color32 {
    const ERROR_MARKERS: [&str; 6] = [
        "Could not",
        "failed",
        "Failed",
        "refused",
        "unavailable",
        "does NOT match",
    ];
    if ERROR_MARKERS.iter().any(|marker| notice.contains(marker)) {
        COLOR_ERROR
    } else {
        COLOR_TEXT_SECONDARY
    }
}

fn avatar_color(seed: &str) -> egui::Color32 {
    let mut hash: u32 = 2_166_136_261; // FNV-1a offset basis
    for byte in seed.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    AVATAR_PALETTE[(hash as usize) % AVATAR_PALETTE.len()]
}

/// A raised card: the one container every redesigned view uses instead of
/// the plain default frame `ui.group()` draws, so cards look consistent
/// everywhere instead of each view styling its own.
fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(COLOR_CARD_BG)
        .stroke(egui::Stroke::new(1.0, COLOR_CARD_BORDER))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::same(SPACE_MD as i8))
        .show(ui, add_contents)
        .inner
}

/// A human relative-time label for `timestamp_ms`, as of `now_ms`. Falls
/// back to a real date once "N days ago" stops being a useful count.
fn relative_time(now_ms: u64, timestamp_ms: u64) -> String {
    let elapsed_secs = now_ms.saturating_sub(timestamp_ms) / 1000;
    if elapsed_secs < 5 {
        return "just now".to_string();
    }
    if elapsed_secs < 60 {
        return format!("{elapsed_secs}s");
    }
    let elapsed_mins = elapsed_secs / 60;
    if elapsed_mins < 60 {
        return format!("{elapsed_mins}m");
    }
    let elapsed_hours = elapsed_mins / 60;
    if elapsed_hours < 24 {
        return format!("{elapsed_hours}h");
    }
    let elapsed_days = elapsed_hours / 24;
    if elapsed_days < 7 {
        return format!("{elapsed_days}d");
    }
    format_date(timestamp_ms)
}

/// A `YYYY-MM-DD` UTC date for `timestamp_ms`, with no calendar dependency:
/// this client's dependency list stays readable, and this is the only place
/// a relative-time label ever falls back to an absolute one.
fn format_date(timestamp_ms: u64) -> String {
    let days_since_epoch = (timestamp_ms / 86_400_000) as i64;
    let (year, month, day) = civil_from_days(days_since_epoch);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Howard Hinnant's `civil_from_days`: proleptic-Gregorian days-since-epoch
/// to (year, month, day). Public-domain calendar arithmetic, not a security
/// primitive, valid for every timestamp this client will ever display.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Onboarding,
    Home,
    Inbox,
    /// People search, opt-in nearby discovery, and (Phase 6) vouch-based
    /// connection discovery — named "Discover" to match
    /// `docs/PLATFORM_PRODUCT_ARCHITECTURE.md`'s unified shell.
    Discover,
    Communities,
    Creator,
    Connections,
    System,
    Diagnostics,
    Updates,
    Privacy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdatePolicy {
    Ask,
    ManualOnly,
}

#[derive(Debug, Clone)]
enum SyncContext {
    FriendRequest { display_name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PrivacyState {
    /// No event collection is compiled into this reference shell.
    telemetry: bool,
    /// External URLs/sources require a deliberate user action.
    external_sources: bool,
    /// Optional relay use is off until explicitly enabled.
    relays: bool,
    /// LAN discovery is local-network only and independently switchable.
    lan_discovery: bool,
    /// Updates can never be installed silently.
    update_policy: UpdatePolicy,
}

impl Default for PrivacyState {
    fn default() -> Self {
        Self {
            telemetry: false,
            external_sources: false,
            relays: false,
            lan_discovery: false,
            update_policy: UpdatePolicy::ManualOnly,
        }
    }
}

struct MininetApp {
    workspace: Option<Workspace>,
    view: View,
    privacy: PrivacyState,
    composer: String,
    community_name: String,
    community_charter: String,
    /// Which community's inline post composer is currently open, if any --
    /// only one at a time, mirroring `reply_target`'s single-slot pattern.
    open_community_composer: Option<mini_objects::ObjectId>,
    community_post_text: String,
    profile_name: String,
    profile_bio: String,
    profile_photo_path: String,
    profile_avatar: Option<mini_objects::ObjectId>,
    profile_remove_photo: bool,
    profile_location: String,
    profile_share_location: bool,
    profile_age: String,
    profile_share_age: bool,
    profile_custom_fields: String,
    people_search: String,
    nearby_profiles: Vec<NearbyProfile>,
    discovery_rx: Option<Receiver<Result<Vec<NearbyProfile>, String>>>,
    visibility_rx: Option<Receiver<Result<String, String>>>,
    profile_textures: HashMap<String, egui::TextureHandle>,
    wall_name: String,
    wall_bio: String,
    wall_links: String,
    wall_unlisted: bool,
    media_path: String,
    media_content_type: String,
    media_caption: String,
    account_name: String,
    account_bio: String,
    signing_confirmation: bool,
    feed_filter: FeedFilter,
    reply_target: Option<mini_objects::ObjectId>,
    reply_text: String,
    export_path: String,
    import_path: String,
    peer_address: String,
    listen_port: String,
    follow_target: String,
    conversation_label: String,
    conversation_peer: String,
    conversation_invite: String,
    import_conversation_label: String,
    import_conversation_invite: String,
    message_text: String,
    selected_conversation: Option<usize>,
    sync_rx: Option<Receiver<Result<String, String>>>,
    sync_context: Option<SyncContext>,
    /// Results of a diagnostics run in progress, off the UI thread.
    selftest_rx: Option<Receiver<SelfTestReport>>,
    selftest_report: Option<SelfTestReport>,
    selftest_area: Option<&'static str>,
    install_notice: String,
    notice: String,
}

struct Workspace {
    store: Store<FsBackend>,
    identity: Option<DesktopIdentity>,
    human: Option<Did>,
    root: PathBuf,
    sequence: u64,
    conversations: Vec<ConversationRecord>,
}

struct DesktopIdentity {
    root: Controller,
    device: Controller,
}

fn desktop_identity_from_seeds(
    root_seeds: &SeedPair,
    device_seeds: &SeedPair,
) -> Result<DesktopIdentity, String> {
    let mut root = Controller::incept_single_from_seeds(&root_seeds.current, &root_seeds.next)
        .map_err(|error| error.to_string())?;
    let device = Controller::incept_device_single_from_seeds(
        &root.did(),
        &device_seeds.current,
        &device_seeds.next,
    )
    .map_err(|error| error.to_string())?;
    root.delegate_device(&device.did(), Capabilities::primary())
        .map_err(|error| error.to_string())?;
    Ok(DesktopIdentity { root, device })
}

fn load_desktop_identity(
    root: &std::path::Path,
    create_device: bool,
) -> Result<DesktopIdentity, String> {
    let root_seeds =
        load_existing(&root.join("identity.dpapi")).map_err(|error| error.to_string())?;
    let device_path = root.join("device.dpapi");
    let device_seeds = if create_device {
        load_or_create(&device_path)
    } else {
        load_existing(&device_path)
    }
    .map_err(|error| error.to_string())?;
    desktop_identity_from_seeds(&root_seeds, &device_seeds)
}

impl Workspace {
    fn open() -> Result<Self, String> {
        let root = data_root();
        let store = Store::new(FsBackend::open(&root).map_err(|error| error.to_string())?);
        let identity_path = root.join("identity.dpapi");
        let conversations = conversation_state::load(&root.join("conversations.dpapi"))?;
        let human = if identity_path.exists() {
            let seeds = load_existing(&identity_path).map_err(|error| error.to_string())?;
            Some(
                Controller::incept_single_from_seeds(&seeds.current, &seeds.next)
                    .map_err(|error| error.to_string())?
                    .did(),
            )
        } else {
            None
        };
        let sequence = next_object_sequence(&store, human.as_ref())?;
        Ok(Self {
            store,
            // Open every session read-only. The DPAPI-protected signing
            // material is reconstructed only after the user presses
            // "Unlock identity". A new root is never created implicitly.
            identity: None,
            human,
            root,
            sequence,
            conversations,
        })
    }

    fn is_unlocked(&self) -> bool {
        self.identity.is_some()
    }

    fn root_created(&self) -> bool {
        self.human.is_some()
    }

    fn has_public_account(&self) -> bool {
        self.current_profile().is_some()
    }

    fn human_did(&self) -> Result<&Did, String> {
        self.human
            .as_ref()
            .ok_or_else(|| "create a Mininet root first".to_string())
    }

    fn create_root(&mut self) -> Result<(), String> {
        if self.root_created() {
            return Ok(());
        }
        let root_seeds =
            load_or_create(&self.root.join("identity.dpapi")).map_err(|error| error.to_string())?;
        let device_seeds =
            load_or_create(&self.root.join("device.dpapi")).map_err(|error| error.to_string())?;
        let identity = desktop_identity_from_seeds(&root_seeds, &device_seeds)?;
        self.human = Some(identity.root.did());
        self.identity = Some(identity);
        Ok(())
    }

    fn lock(&mut self) {
        self.identity = None;
    }

    fn unlock(&mut self) -> Result<(), String> {
        // Explicit unlock is also the migration boundary for pre-device beta
        // accounts: it creates one independently protected delegated-device
        // vault while preserving the existing human-root DID.
        let identity = load_desktop_identity(&self.root, true)?;
        self.human = Some(identity.root.did());
        self.identity = Some(identity);
        Ok(())
    }

    fn publish_post(&mut self, text: &str) -> Result<(), String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| "identity is locked".to_string())?;
        let human = self.human_did()?.clone();
        publish_post(
            &mut self.store,
            &human,
            &identity.device,
            text,
            now_ms(),
            self.sequence,
        )
        .map_err(|error| error.to_string())?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(())
    }

    fn publish_profile(&mut self, name: &str, bio: &str) -> Result<(), String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| "identity is locked".to_string())?;
        let human = self.human_did()?.clone();
        publish_profile(
            &mut self.store,
            &human,
            &identity.device,
            name,
            bio,
            None,
            now_ms(),
            self.sequence,
        )
        .map_err(|error| error.to_string())?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn publish_custom_profile(
        &mut self,
        name: &str,
        bio: &str,
        photo_path: &str,
        existing_avatar: Option<&mini_objects::ObjectId>,
        location: Option<&str>,
        age: Option<u8>,
        fields: &[PublicProfileField],
    ) -> Result<Option<mini_objects::ObjectId>, String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| "identity is locked".to_string())?;
        let human = self.human_did()?.clone();
        let avatar = if photo_path.trim().is_empty() {
            existing_avatar.cloned()
        } else {
            let bytes = std::fs::read(photo_path)
                .map_err(|error| format!("could not read profile photo: {error}"))?;
            if bytes.len() > 8 * 1024 * 1024 {
                return Err("profile photo exceeds the 8 MiB beta limit".to_string());
            }
            let (_decoded, content_type) = decode_profile_image(&bytes)?;
            let manifest = publish_media(
                &mut self.store,
                &human,
                &identity.device,
                content_type,
                &bytes,
                now_ms(),
                self.sequence,
            )
            .map_err(|error| error.to_string())?;
            self.sequence = self
                .sequence
                .saturating_add(manifest.chunks.len() as u64 + 1);
            Some(manifest.id)
        };
        let draft = PublicProfileDraft {
            display_name: name,
            bio,
            avatar: avatar.as_ref(),
            location,
            age,
            fields,
        };
        publish_profile_details(
            &mut self.store,
            &human,
            &identity.device,
            &draft,
            now_ms(),
            self.sequence,
        )
        .map_err(|error| error.to_string())?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(avatar)
    }

    #[allow(clippy::too_many_arguments)]
    fn publish_custom_profile_confirmed(
        &mut self,
        name: &str,
        bio: &str,
        photo_path: &str,
        existing_avatar: Option<&mini_objects::ObjectId>,
        location: Option<&str>,
        age: Option<u8>,
        fields: &[PublicProfileField],
    ) -> Result<Option<mini_objects::ObjectId>, String> {
        let relock = !self.is_unlocked();
        if relock {
            self.unlock()?;
        }
        let result = self.publish_custom_profile(
            name,
            bio,
            photo_path,
            existing_avatar,
            location,
            age,
            fields,
        );
        if relock {
            self.lock();
        }
        result
    }

    fn known_profiles(&self) -> Vec<mini_social::Profile> {
        known_profiles(&self.store).unwrap_or_default()
    }

    fn current_profile(&self) -> Option<mini_social::Profile> {
        self.human
            .as_ref()
            .and_then(|human| resolve_profile(&self.store, human).ok().flatten())
    }

    /// The published profile for any identity root, not just this device's
    /// own -- how a feed card resolves a post's real author name and
    /// avatar instead of a placeholder.
    fn profile_for(&self, human: &Did) -> Option<mini_social::Profile> {
        resolve_profile(&self.store, human).ok().flatten()
    }

    fn profile_needs_device_upgrade(&self) -> bool {
        let Some(human) = self.human.as_ref() else {
            return false;
        };
        self.store
            .resolve_head(human, "profile")
            .ok()
            .flatten()
            .and_then(|id| self.store.get(&id).ok())
            .is_some_and(|profile| profile.author_device == *human)
    }

    fn upgrade_profile_for_sync(&mut self) -> Result<(), String> {
        let profile = self
            .current_profile()
            .ok_or_else(|| "publish a public profile first".to_string())?;
        self.publish_custom_profile_confirmed(
            &profile.display_name,
            &profile.bio,
            "",
            profile.avatar.as_ref(),
            profile.location.as_deref(),
            profile.age,
            &profile.fields,
        )?;
        Ok(())
    }

    fn follows(&self, target: &Did) -> bool {
        self.human
            .as_ref()
            .and_then(|human| following(&self.store, human).ok())
            .is_some_and(|people| people.iter().any(|person| person == target))
    }

    fn is_friend(&self, target: &Did) -> bool {
        self.follows(target)
            && self
                .human
                .as_ref()
                .and_then(|human| followers(&self.store, human).ok())
                .is_some_and(|people| people.iter().any(|person| person == target))
    }

    fn set_follow_target_confirmed(&mut self, target: &str, active: bool) -> Result<(), String> {
        let relock = !self.is_unlocked();
        if relock {
            self.unlock()?;
        }
        let result = self.set_follow_target(target, active);
        if relock {
            self.lock();
        }
        result
    }

    fn profile_image(&self, id: &mini_objects::ObjectId) -> Result<Vec<u8>, String> {
        let object = self.store.get(id).map_err(|error| error.to_string())?;
        let manifest = read_manifest(&object).map_err(|error| error.to_string())?;
        if !manifest.content_type.starts_with("image/") || manifest.total_len > 8 * 1024 * 1024 {
            return Err("profile photo manifest is not a bounded image".to_string());
        }
        assemble(&self.store, &manifest).map_err(|error| error.to_string())
    }

    fn publish_public_wall(
        &mut self,
        name: &str,
        bio: &str,
        links: &[&str],
        unlisted: bool,
    ) -> Result<(), String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| "identity is locked".to_string())?;
        let human = self.human_did()?.clone();
        publish_wall(
            &mut self.store,
            &human,
            &identity.device,
            name,
            bio,
            None,
            links,
            &[],
            if unlisted {
                VisibilityPolicy::Unlisted
            } else {
                VisibilityPolicy::Public
            },
            now_ms(),
            self.sequence,
        )
        .map_err(|error| error.to_string())?;
        self.sequence = self.sequence.saturating_add(2);
        Ok(())
    }

    fn publish_community(&mut self, name: &str, charter: &str) -> Result<(), String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| "identity is locked".to_string())?;
        let human = self.human_did()?.clone();
        publish_community(
            &mut self.store,
            &human,
            &identity.device,
            name,
            charter,
            MembershipMode::Open,
            now_ms(),
            self.sequence,
        )
        .map_err(|error| error.to_string())?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(())
    }

    fn publish_media_post(
        &mut self,
        path: &str,
        content_type: &str,
        caption: &str,
    ) -> Result<(), String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| "identity is locked".to_string())?;
        let bytes =
            std::fs::read(path).map_err(|error| format!("could not read media file: {error}"))?;
        let human = self.human_did()?.clone();
        let base_sequence = self.sequence;
        let manifest = publish_media(
            &mut self.store,
            &human,
            &identity.device,
            content_type,
            &bytes,
            now_ms(),
            base_sequence,
        )
        .map_err(|error| error.to_string())?;
        self.sequence = base_sequence.saturating_add(manifest.chunks.len() as u64 + 1);
        publish_media_post(
            &mut self.store,
            &human,
            &identity.device,
            manifest.id,
            caption,
            now_ms(),
            self.sequence,
        )
        .map_err(|error| error.to_string())?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(())
    }

    fn feed(&self, filter: FeedFilter) -> Result<Vec<FeedItem>, String> {
        feed(&self.store, self.human_did()?, filter, 50).map_err(|error| error.to_string())
    }

    fn publish_comment(
        &mut self,
        parent: &mini_objects::ObjectId,
        text: &str,
    ) -> Result<(), String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| "identity is locked".to_string())?;
        let human = self.human_did()?.clone();
        publish_comment(
            &mut self.store,
            &human,
            &identity.device,
            parent,
            text,
            now_ms(),
            self.sequence,
        )
        .map_err(|error| error.to_string())?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(())
    }

    fn react_like(&mut self, target: &mini_objects::ObjectId) -> Result<(), String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| "identity is locked".to_string())?;
        let human = self.human_did()?.clone();
        set_reaction(
            &mut self.store,
            &human,
            &identity.device,
            target,
            ReactionKind::Like,
            true,
            now_ms(),
            self.sequence,
        )
        .map_err(|error| error.to_string())?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(())
    }

    fn comment_count(&self, target: &mini_objects::ObjectId) -> usize {
        comments(&self.store, target)
            .map(|items| items.len())
            .unwrap_or(0)
    }

    /// Direct replies to `parent` (a post or another comment) -- the one
    /// level [`comment_thread`](MininetApp::comment_thread) recurses over
    /// to build a visible nested thread.
    fn comment_children(&self, parent: &mini_objects::ObjectId) -> Vec<mini_social::Comment> {
        comments(&self.store, parent).unwrap_or_default()
    }

    fn publish_community_post(
        &mut self,
        community: &mini_objects::ObjectId,
        text: &str,
    ) -> Result<(), String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| "identity is locked".to_string())?;
        let human = self.human_did()?.clone();
        publish_community_post(
            &mut self.store,
            &human,
            &identity.device,
            community.clone(),
            text,
            now_ms(),
            self.sequence,
        )
        .map_err(|error| error.to_string())?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(())
    }

    /// Every post scoped to `community`, newest first.
    fn community_feed(&self, community: &mini_objects::ObjectId) -> Vec<CommunityPost> {
        community_posts(&self.store, community).unwrap_or_default()
    }

    /// Cast a Reddit-style exclusive vote: choosing a side always explicitly
    /// clears the other, so a stale opposite vote never lingers active.
    /// There is no toggle-to-clear-both yet -- clicking Upvote/Downvote
    /// again simply re-asserts the same side, which is what the UI's two
    /// separate buttons naturally produce.
    fn react_vote(&mut self, target: &mini_objects::ObjectId, upvote: bool) -> Result<(), String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| "identity is locked".to_string())?;
        let human = self.human_did()?.clone();
        set_reaction(
            &mut self.store,
            &human,
            &identity.device,
            target,
            ReactionKind::Upvote,
            upvote,
            now_ms(),
            self.sequence,
        )
        .map_err(|error| error.to_string())?;
        self.sequence = self.sequence.saturating_add(1);
        set_reaction(
            &mut self.store,
            &human,
            &identity.device,
            target,
            ReactionKind::Downvote,
            !upvote,
            now_ms(),
            self.sequence,
        )
        .map_err(|error| error.to_string())?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(())
    }

    fn export_bundle(&self, path: &str) -> Result<usize, String> {
        const MAGIC: &[u8] = b"MINIBND1";
        const MAX_OBJECTS: usize = 10_000;
        const MAX_OBJECT_BYTES: usize = 16 * 1024 * 1024;
        let ids = self.store.all_ids().map_err(|error| error.to_string())?;
        if ids.len() > MAX_OBJECTS {
            return Err("local store exceeds export object limit".to_string());
        }
        let mut bundle = Vec::new();
        bundle.extend_from_slice(MAGIC);
        bundle.extend_from_slice(&(ids.len() as u32).to_be_bytes());
        for id in ids {
            let bytes = self
                .store
                .get(&id)
                .map_err(|error| error.to_string())?
                .to_bytes();
            if bytes.len() > MAX_OBJECT_BYTES {
                return Err("object exceeds export size limit".to_string());
            }
            bundle.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            bundle.extend_from_slice(&bytes);
        }
        atomic_write_file(PathBuf::from(path).as_path(), &bundle)?;
        Ok(bundle.len())
    }

    fn import_bundle(&mut self, path: &str) -> Result<usize, String> {
        const MAGIC: &[u8] = b"MINIBND1";
        const MAX_OBJECTS: usize = 10_000;
        const MAX_OBJECT_BYTES: usize = 16 * 1024 * 1024;
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        if bytes.len() < MAGIC.len() + 4 || &bytes[..MAGIC.len()] != MAGIC {
            return Err("invalid Mininet bundle header".to_string());
        }
        let mut offset = MAGIC.len();
        let count = read_u32(&bytes, &mut offset)? as usize;
        if count > MAX_OBJECTS {
            return Err("bundle exceeds object limit".to_string());
        }
        for _ in 0..count {
            let length = read_u32(&bytes, &mut offset)? as usize;
            if length > MAX_OBJECT_BYTES || offset.saturating_add(length) > bytes.len() {
                return Err("bundle object exceeds size limit".to_string());
            }
            let object = mini_objects::Object::from_bytes(&bytes[offset..offset + length])
                .map_err(|error| error.to_string())?;
            self.store
                .insert(&object)
                .map_err(|error| error.to_string())?;
            offset += length;
        }
        if offset != bytes.len() {
            return Err("bundle has trailing bytes".to_string());
        }
        Ok(count)
    }

    fn set_community_membership(
        &mut self,
        community: &mini_objects::ObjectId,
        joined: bool,
    ) -> Result<(), String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| "identity is locked".to_string())?;
        let human = self.human_did()?.clone();
        set_membership(
            &mut self.store,
            &human,
            &identity.device,
            community,
            joined,
            now_ms(),
            self.sequence,
        )
        .map_err(|error| error.to_string())?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(())
    }

    fn set_follow_target(&mut self, target: &str, follow: bool) -> Result<(), String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| "identity is locked".to_string())?;
        let human = self.human_did()?.clone();
        let target = Did::parse(target.trim()).map_err(|error| error.to_string())?;
        set_follow(
            &mut self.store,
            &human,
            &identity.device,
            &target,
            follow,
            now_ms(),
            self.sequence,
        )
        .map_err(|error| error.to_string())?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(())
    }

    fn following_count(&self) -> usize {
        self.human
            .as_ref()
            .and_then(|human| following(&self.store, human).ok())
            .map_or(0, |people| people.len())
    }

    fn follower_count(&self) -> usize {
        self.human
            .as_ref()
            .and_then(|human| followers(&self.store, human).ok())
            .map_or(0, |people| people.len())
    }

    fn mutual_follow_count(&self) -> usize {
        let Some(human) = self.human.as_ref() else {
            return 0;
        };
        let Ok(outgoing) = following(&self.store, human) else {
            return 0;
        };
        let Ok(incoming) = followers(&self.store, human) else {
            return 0;
        };
        outgoing
            .iter()
            .filter(|person| incoming.iter().any(|other| other == *person))
            .count()
    }

    fn post_text(&self, item: &FeedItem) -> String {
        // Canonical path only: `resolve_post` re-applies the same
        // structural validation `feed` already used to admit this item
        // (type, bound, UTF-8, link shape), rather than re-decoding the
        // raw payload here and risking a second, divergent decode rule.
        resolve_post(&self.store, &item.id)
            .map(|post| post.text)
            .unwrap_or_else(|_| "[unreadable or encrypted post]".to_string())
    }

    /// Same resolution as [`Self::post_text`], for a caller (like
    /// [`CommunityPost`]) that only has the post id, not a whole
    /// [`FeedItem`]. `None` only for a malformed/unreadable post -- callers
    /// decide how to render that, rather than this baking in feed's
    /// specific placeholder text.
    fn post_body(&self, id: &mini_objects::ObjectId) -> Option<String> {
        resolve_post(&self.store, id).map(|post| post.text).ok()
    }

    fn communities(&self) -> Vec<(mini_objects::ObjectId, String, String, usize, bool)> {
        self.store
            .by_type(&ObjectType::COMMUNITY)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| {
                let community = resolve_community(&self.store, &id).ok()?;
                let members = community_members(&self.store, &id).ok()?;
                let joined = members
                    .iter()
                    .any(|member| Some(member) == self.human.as_ref());
                Some((id, community.name, community.charter, members.len(), joined))
            })
            .collect()
    }

    fn create_beta_conversation(&mut self, label: &str, peer: &str) -> Result<String, String> {
        if !self.is_unlocked() {
            return Err("identity is locked".to_string());
        }
        let peer = Did::parse(peer.trim()).map_err(|error| error.to_string())?;
        let inviter = self.human_did()?.clone();
        let (record, invite) = ConversationRecord::create(label.trim().to_string(), peer, inviter)?;
        if self
            .conversations
            .iter()
            .any(|existing| existing.route() == record.route())
        {
            return Err("conversation route already exists".to_string());
        }
        self.conversations.push(record);
        if let Err(error) =
            conversation_state::save(&self.root.join("conversations.dpapi"), &self.conversations)
        {
            self.conversations.pop();
            return Err(error);
        }
        Ok(invite)
    }

    fn import_beta_conversation(&mut self, label: &str, invite: &str) -> Result<usize, String> {
        let record = ConversationRecord::import(label.trim().to_string(), invite)?;
        if self
            .conversations
            .iter()
            .any(|existing| existing.route() == record.route())
        {
            return Err("this conversation invite is already imported".to_string());
        }
        self.conversations.push(record);
        if let Err(error) =
            conversation_state::save(&self.root.join("conversations.dpapi"), &self.conversations)
        {
            self.conversations.pop();
            return Err(error);
        }
        Ok(self.conversations.len() - 1)
    }

    fn send_private_message(&mut self, index: usize, body: &str) -> Result<(), String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| "identity is locked".to_string())?;
        let human = self.human_did()?.clone();
        let conversation = self
            .conversations
            .get(index)
            .ok_or_else(|| "select a conversation".to_string())?;
        let secret = conversation.secret()?;
        send_message(
            &mut self.store,
            &secret,
            human,
            &identity.device,
            now_ms(),
            self.sequence,
            MessageDraft::text(body),
        )
        .map_err(|error| error.to_string())?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(())
    }

    fn private_messages(&self, index: usize) -> Result<mini_messaging::ConversationScan, String> {
        let conversation = self
            .conversations
            .get(index)
            .ok_or_else(|| "select a conversation".to_string())?;
        scan_messages(&self.store, &conversation.secret()?).map_err(|error| error.to_string())
    }
}

fn data_root() -> PathBuf {
    let home = std::env::var_os("MININET_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .map(|root| root.join("Mininet"))
        })
        .unwrap_or_else(|| PathBuf::from("Mininet"));
    home.join("objects")
}

fn settings_path() -> PathBuf {
    data_root().join("settings.dpapi")
}

fn encode_privacy_settings(settings: PrivacyState) -> [u8; 5] {
    [
        1,
        u8::from(settings.external_sources),
        u8::from(settings.relays),
        u8::from(settings.lan_discovery),
        match settings.update_policy {
            UpdatePolicy::ManualOnly => 1,
            UpdatePolicy::Ask => 2,
        },
    ]
}

fn decode_privacy_settings(bytes: &[u8]) -> Option<PrivacyState> {
    if bytes.len() != 5 || bytes[0] != 1 || bytes[1..4].iter().any(|byte| *byte > 1) {
        return None;
    }
    Some(PrivacyState {
        telemetry: false,
        external_sources: bytes[1] == 1,
        relays: bytes[2] == 1,
        lan_discovery: bytes[3] == 1,
        update_policy: match bytes[4] {
            1 => UpdatePolicy::ManualOnly,
            2 => UpdatePolicy::Ask,
            _ => return None,
        },
    })
}

fn load_privacy_settings() -> PrivacyState {
    load_user_data(&settings_path())
        .ok()
        .and_then(|bytes| decode_privacy_settings(&bytes))
        .unwrap_or_default()
}

fn save_privacy_settings(settings: PrivacyState) -> Result<(), String> {
    save_user_data(&settings_path(), &encode_privacy_settings(settings))
        .map_err(|error| error.to_string())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn next_object_sequence<B: Backend>(store: &Store<B>, author: Option<&Did>) -> Result<u64, String> {
    store
        .all_ids()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter_map(|id| store.get(&id).ok())
        .filter(|object| author.is_some_and(|author| &object.author_human == author))
        .map(|object| object.sequence)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| "local object sequence space is exhausted".to_string())
}

fn decode_profile_image(bytes: &[u8]) -> Result<(image::DynamicImage, &'static str), String> {
    const MAX_DIMENSION: u32 = 4096;
    const MAX_DECODE_ALLOCATION: u64 = 96 * 1024 * 1024;

    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| format!("could not inspect profile photo: {error}"))?;
    let content_type = match reader.format() {
        Some(image::ImageFormat::Png) => "image/png",
        Some(image::ImageFormat::Jpeg) => "image/jpeg",
        Some(image::ImageFormat::WebP) => "image/webp",
        Some(image::ImageFormat::Gif) => "image/gif",
        _ => return Err("profile photo must be PNG, JPEG, WebP, or GIF".to_string()),
    };
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOCATION);
    reader.limits(limits);
    let image = reader.decode().map_err(|error| {
        format!(
            "profile photo could not be decoded within the {MAX_DIMENSION}×{MAX_DIMENSION} safety limit: {error}"
        )
    })?;
    Ok((image, content_type))
}

fn parse_profile_fields(text: &str) -> Result<Vec<PublicProfileField>, String> {
    let mut fields = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let (label, value) = line
            .split_once(':')
            .ok_or_else(|| format!("custom field must use Label: Value — {line}"))?;
        let label = label.trim();
        let value = value.trim();
        if label.is_empty() || value.is_empty() {
            return Err("custom profile labels and values cannot be empty".to_string());
        }
        if label.len() > MAX_PROFILE_FIELD_LABEL_BYTES {
            return Err(format!(
                "custom profile label exceeds {MAX_PROFILE_FIELD_LABEL_BYTES} bytes: {label}"
            ));
        }
        if value.len() > MAX_PROFILE_FIELD_VALUE_BYTES {
            return Err(format!(
                "custom profile value for {label} exceeds {MAX_PROFILE_FIELD_VALUE_BYTES} bytes"
            ));
        }
        if fields
            .iter()
            .any(|field: &PublicProfileField| field.label.eq_ignore_ascii_case(label))
        {
            return Err(format!("duplicate custom profile field: {label}"));
        }
        fields.push(PublicProfileField {
            label: label.to_string(),
            value: value.to_string(),
        });
    }
    if fields.len() > MAX_PROFILE_FIELDS {
        return Err(format!(
            "at most {MAX_PROFILE_FIELDS} custom public profile fields are supported"
        ));
    }
    Ok(fields)
}

fn atomic_write_file(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut temporary = path.to_path_buf();
    temporary.set_extension("tmp");
    let result = (|| {
        let mut file = std::fs::File::create(&temporary).map_err(|error| error.to_string())?;
        use std::io::Write;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, path).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> Result<u32, String> {
    if offset.saturating_add(4) > bytes.len() {
        return Err("truncated bundle".to_string());
    }
    let value = u32::from_be_bytes([
        bytes[*offset],
        bytes[*offset + 1],
        bytes[*offset + 2],
        bytes[*offset + 3],
    ]);
    *offset += 4;
    Ok(value)
}

fn configure_peer_stream(stream: &TcpStream) -> Result<(), String> {
    stream
        .set_read_timeout(Some(PEER_IO_TIMEOUT))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(PEER_IO_TIMEOUT))
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn nearby_endpoint_for(profiles: &[NearbyProfile], did: &Did) -> Option<SocketAddr> {
    profiles
        .iter()
        .find(|profile| &profile.did == did)
        .map(|profile| profile.address)
}

fn open_sync_state(
    root: &std::path::Path,
    identity: &DesktopIdentity,
) -> Result<(Store<FsBackend>, KelCache), String> {
    let mut store = Store::new(FsBackend::open(root).map_err(|error| error.to_string())?);
    let human = identity.root.did();
    for kel in [identity.root.kel(), identity.device.kel()] {
        let carrier =
            kel_carrier(&kel, &human, &identity.device).map_err(|error| error.to_string())?;
        store.insert(&carrier).map_err(|error| error.to_string())?;
    }
    let mut cache = KelCache::new();
    cache.insert_verified(identity.root.kel());
    cache.insert_verified(identity.device.kel());
    cache
        .hydrate_from_store(&store)
        .map_err(|error| error.to_string())?;
    Ok((store, cache))
}

fn run_peer_sync(root: &std::path::Path, endpoint: &str, listener: bool) -> Result<String, String> {
    let identity = load_desktop_identity(root, false).map_err(|error| {
        format!(
            "identity/device vault unavailable; unlock the identity once before syncing: {error}"
        )
    })?;
    let (mut store, mut cache) = open_sync_state(root, &identity)?;

    if listener {
        let listener = std::net::TcpListener::bind(endpoint).map_err(|error| error.to_string())?;
        let (stream, _) = listener.accept().map_err(|error| error.to_string())?;
        configure_peer_stream(&stream)?;
        let mut bearer = TcpBearer::from_stream(stream).map_err(|error| error.to_string())?;
        let hello = bearer.recv().map_err(|error| error.to_string())?;
        let (mut channel, response) =
            Responder::respond(&hello).map_err(|error| error.to_string())?;
        bearer.send(&response).map_err(|error| error.to_string())?;
        let report = sync_bidirectional(
            &mut bearer,
            &mut channel,
            &mut store,
            &mut cache,
            SyncRole::Responder,
        )
        .map_err(|error| error.to_string())?;
        Ok(format!(
            "Peer sync complete: received {}, accepted {}, identity carriers {}, unknown authors {}, invalid {}.",
            report.received,
            report.accepted,
            report.carriers,
            report.unknown_author,
            report.invalid
        ))
    } else {
        let address = endpoint
            .to_socket_addrs()
            .map_err(|error| error.to_string())?
            .next()
            .ok_or_else(|| "peer address did not resolve".to_string())?;
        let stream = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(10))
            .map_err(|error| error.to_string())?;
        configure_peer_stream(&stream)?;
        let mut bearer = TcpBearer::from_stream(stream).map_err(|error| error.to_string())?;
        let (initiator, hello) = Initiator::start().map_err(|error| error.to_string())?;
        bearer.send(&hello).map_err(|error| error.to_string())?;
        let response = bearer.recv().map_err(|error| error.to_string())?;
        let mut channel = initiator
            .finish(&response)
            .map_err(|error| error.to_string())?;
        let report = sync_bidirectional(
            &mut bearer,
            &mut channel,
            &mut store,
            &mut cache,
            SyncRole::Initiator,
        )
        .map_err(|error| error.to_string())?;
        Ok(format!(
            "Peer sync complete: received {}, accepted {}, identity carriers {}, unknown authors {}, invalid {}.",
            report.received,
            report.accepted,
            report.carriers,
            report.unknown_author,
            report.invalid
        ))
    }
}

fn run_private_sync(
    root: &std::path::Path,
    endpoint: &str,
    listener: bool,
    route: OpaqueRoute,
) -> Result<String, String> {
    let mut store = Store::new(FsBackend::open(root).map_err(|error| error.to_string())?);
    let report = if listener {
        let listener = std::net::TcpListener::bind(endpoint).map_err(|error| error.to_string())?;
        let (stream, _) = listener.accept().map_err(|error| error.to_string())?;
        configure_peer_stream(&stream)?;
        let mut bearer = TcpBearer::from_stream(stream).map_err(|error| error.to_string())?;
        let hello = bearer.recv().map_err(|error| error.to_string())?;
        let (mut channel, response) =
            Responder::respond(&hello).map_err(|error| error.to_string())?;
        bearer.send(&response).map_err(|error| error.to_string())?;
        sync_private_route_bidirectional(
            &mut bearer,
            &mut channel,
            &mut store,
            route,
            SyncRole::Responder,
        )
        .map_err(|error| error.to_string())?
    } else {
        let address = endpoint
            .to_socket_addrs()
            .map_err(|error| error.to_string())?
            .next()
            .ok_or_else(|| "peer address did not resolve".to_string())?;
        let stream = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(10))
            .map_err(|error| error.to_string())?;
        configure_peer_stream(&stream)?;
        let mut bearer = TcpBearer::from_stream(stream).map_err(|error| error.to_string())?;
        let (initiator, hello) = Initiator::start().map_err(|error| error.to_string())?;
        bearer.send(&hello).map_err(|error| error.to_string())?;
        let response = bearer.recv().map_err(|error| error.to_string())?;
        let mut channel = initiator
            .finish(&response)
            .map_err(|error| error.to_string())?;
        sync_private_route_bidirectional(
            &mut bearer,
            &mut channel,
            &mut store,
            route,
            SyncRole::Initiator,
        )
        .map_err(|error| error.to_string())?
    };
    Ok(format!(
        "Private sync complete: received {}, accepted {}, invalid {}.",
        report.received, report.accepted, report.invalid
    ))
}

fn scan_nearby_profiles() -> Result<Vec<NearbyProfile>, String> {
    let scanner = LocalProfileScanner::bind().map_err(|error| error.to_string())?;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let mut profiles: Vec<NearbyProfile> = Vec::new();
    while let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) {
        let Some(profile) = scanner
            .recv_timeout(remaining.min(Duration::from_millis(500)))
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        if let Some(existing) = profiles
            .iter_mut()
            .find(|existing| existing.did == profile.did)
        {
            *existing = profile;
        } else {
            profiles.push(profile);
        }
    }
    profiles.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
            .then_with(|| left.did.as_str().cmp(right.did.as_str()))
    });
    Ok(profiles)
}

fn run_discoverable_profile_sync(
    root: &std::path::Path,
    port: u16,
    display_name: &str,
    visibility_duration: Duration,
    progress: &mpsc::Sender<Result<String, String>>,
) -> Result<String, String> {
    let identity = load_desktop_identity(root, false).map_err(|error| {
        format!(
            "identity/device vault unavailable; unlock the identity once before syncing: {error}"
        )
    })?;
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port))
        .map_err(|error| error.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let announcer = LocalProfileAnnouncer::bind(port, &identity.root.did(), display_name)
        .map_err(|error| error.to_string())?;
    let deadline = std::time::Instant::now() + visibility_duration;
    let mut completed = 0usize;
    loop {
        announcer.announce().map_err(|error| error.to_string())?;
        match listener.accept() {
            Ok((stream, peer)) => {
                let result = (|| {
                    stream
                        .set_nonblocking(false)
                        .map_err(|error| error.to_string())?;
                    configure_peer_stream(&stream)?;
                    let (mut store, mut cache) = open_sync_state(root, &identity)?;
                    let mut bearer =
                        TcpBearer::from_stream(stream).map_err(|error| error.to_string())?;
                    let hello = bearer.recv().map_err(|error| error.to_string())?;
                    let (mut channel, response) =
                        Responder::respond(&hello).map_err(|error| error.to_string())?;
                    bearer.send(&response).map_err(|error| error.to_string())?;
                    sync_bidirectional(
                        &mut bearer,
                        &mut channel,
                        &mut store,
                        &mut cache,
                        SyncRole::Responder,
                    )
                    .map_err(|error| error.to_string())
                })();
                match result {
                    Ok(report) => {
                        completed = completed.saturating_add(1);
                        let _ = progress.send(Ok(format!(
                            "Nearby sync #{completed} complete with {peer}: received {}, accepted {}, identity carriers {}, unknown authors {}, invalid {}. Still visible until the window ends.",
                            report.received,
                            report.accepted,
                            report.carriers,
                            report.unknown_author,
                            report.invalid
                        )));
                    }
                    Err(error) => {
                        let _ = progress.send(Err(format!(
                            "Rejected or incomplete nearby sync from {peer}: {error}. Visibility remains active."
                        )));
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.to_string()),
        }
        if std::time::Instant::now() >= deadline {
            return Ok(format!(
                "Nearby visibility window ended after {completed} completed sync connection(s)."
            ));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

impl Default for MininetApp {
    fn default() -> Self {
        let workspace = Workspace::open().ok();
        let existing_profile = workspace.as_ref().and_then(Workspace::current_profile);
        let view = if workspace
            .as_ref()
            .is_some_and(|workspace| workspace.root_created() && workspace.has_public_account())
        {
            View::Home
        } else {
            View::Onboarding
        };
        Self {
            workspace,
            view,
            privacy: load_privacy_settings(),
            composer: String::new(),
            community_name: String::new(),
            community_charter: String::new(),
            open_community_composer: None,
            community_post_text: String::new(),
            profile_name: existing_profile
                .as_ref()
                .map(|profile| profile.display_name.clone())
                .unwrap_or_default(),
            profile_bio: existing_profile
                .as_ref()
                .map(|profile| profile.bio.clone())
                .unwrap_or_default(),
            profile_photo_path: String::new(),
            profile_avatar: existing_profile
                .as_ref()
                .and_then(|profile| profile.avatar.clone()),
            profile_remove_photo: false,
            profile_location: existing_profile
                .as_ref()
                .and_then(|profile| profile.location.clone())
                .unwrap_or_default(),
            profile_share_location: existing_profile
                .as_ref()
                .is_some_and(|profile| profile.location.is_some()),
            profile_age: existing_profile
                .as_ref()
                .and_then(|profile| profile.age)
                .map(|age| age.to_string())
                .unwrap_or_default(),
            profile_share_age: existing_profile
                .as_ref()
                .is_some_and(|profile| profile.age.is_some()),
            profile_custom_fields: existing_profile
                .as_ref()
                .map(|profile| {
                    profile
                        .fields
                        .iter()
                        .map(|field| format!("{}: {}", field.label, field.value))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default(),
            people_search: String::new(),
            nearby_profiles: Vec::new(),
            discovery_rx: None,
            visibility_rx: None,
            profile_textures: HashMap::new(),
            wall_name: String::new(),
            wall_bio: String::new(),
            wall_links: String::new(),
            wall_unlisted: false,
            media_path: String::new(),
            media_content_type: "video/mp4".to_string(),
            media_caption: String::new(),
            account_name: String::new(),
            account_bio: String::new(),
            signing_confirmation: false,
            feed_filter: FeedFilter::Chronological,
            reply_target: None,
            reply_text: String::new(),
            export_path: data_root()
                .join("mininet-export.minibundle")
                .display()
                .to_string(),
            import_path: data_root()
                .join("mininet-import.minibundle")
                .display()
                .to_string(),
            peer_address: "127.0.0.1:46000".to_string(),
            listen_port: "46000".to_string(),
            follow_target: String::new(),
            conversation_label: String::new(),
            conversation_peer: String::new(),
            conversation_invite: String::new(),
            import_conversation_label: String::new(),
            import_conversation_invite: String::new(),
            message_text: String::new(),
            selected_conversation: None,
            sync_rx: None,
            sync_context: None,
            selftest_rx: None,
            selftest_report: None,
            selftest_area: None,
            install_notice: String::new(),
            notice:
                "Local object store ready. Identity is locked; no network activity has started."
                    .to_string(),
        }
    }
}

impl eframe::App for MininetApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_theme(ctx);
        if self.view == View::Creator {
            if let Some(path) = ctx.input(|input| {
                input
                    .raw
                    .dropped_files
                    .iter()
                    .find_map(|file| file.path.clone())
            }) {
                self.profile_photo_path = path.display().to_string();
                self.profile_remove_photo = false;
                self.notice = "Profile photo selected. It remains local until you review and publish the signed profile.".to_string();
            }
        }
        if let Some(result) = self
            .discovery_rx
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok())
        {
            self.discovery_rx = None;
            match result {
                Ok(profiles) => {
                    let count = profiles.len();
                    self.nearby_profiles = profiles;
                    self.notice = format!("Nearby scan complete: {count} opt-in profile(s) found.");
                }
                Err(error) => self.notice = format!("Nearby scan failed: {error}"),
            }
        }
        if let Some(result) = self
            .sync_rx
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok())
        {
            self.sync_rx = None;
            self.workspace = Workspace::open().ok();
            self.notice = match (self.sync_context.take(), result) {
                (Some(SyncContext::FriendRequest { display_name }), Ok(summary)) => format!(
                    "Friend request delivered to {display_name}. They can add you back after syncing. {summary}"
                ),
                (Some(SyncContext::FriendRequest { display_name }), Err(error)) => format!(
                    "Friend request for {display_name} is saved locally, but automatic delivery failed: {error}. Find them nearby and retry sync."
                ),
                (None, Ok(summary)) => summary,
                (None, Err(error)) => format!("Peer sync failed: {error}"),
            };
        }
        if let Some(receiver) = self.selftest_rx.as_ref() {
            match receiver.try_recv() {
                Ok(report) => {
                    self.selftest_rx = None;
                    self.notice = format!("Diagnostics finished: {}", report.summary());
                    self.selftest_report = Some(report);
                }
                // The worker went away without sending: a check panicked.
                // Discarding this state with `.ok()` left the receiver in
                // place forever, so the page kept spinning with every button
                // disabled and no way to retry short of restarting.
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.selftest_rx = None;
                    self.notice =
                        "Diagnostics stopped unexpectedly. Nothing here has been verified."
                            .to_string();
                    self.selftest_report = Some(SelfTestReport {
                        checks: vec![mini_selftest::Check {
                            area: "diagnostics",
                            name: "the diagnostics stopped before reporting",
                            negative: false,
                            outcome: CheckOutcome::Failed {
                                detail: "a check ended the run without producing a result. \
                                         Nothing has been verified; press a button to run again."
                                    .to_string(),
                            },
                        }],
                        elapsed_ms: 0,
                    });
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let mut visibility_results = Vec::new();
        let mut visibility_finished = false;
        if let Some(receiver) = self.visibility_rx.as_ref() {
            loop {
                match receiver.try_recv() {
                    Ok(result) => visibility_results.push(result),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        visibility_finished = true;
                        break;
                    }
                }
            }
        }
        if visibility_finished {
            self.visibility_rx = None;
        }
        for result in visibility_results {
            self.workspace = Workspace::open().ok();
            self.notice = match result {
                Ok(summary) => summary,
                Err(error) => error,
            };
        }
        if self.view == View::Onboarding {
            self.onboarding(ctx);
            return;
        }
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.set_min_height(54.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("MININET").strong().size(20.0));
                ui.label(egui::RichText::new("local-first social network").color(egui::Color32::GRAY));
                ui.separator();
                ui.colored_label(egui::Color32::from_rgb(100, 210, 160), "LOCAL ONLY");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Privacy center").clicked() {
                        self.view = View::Privacy;
                    }
                    if let Some(workspace) = self.workspace.as_mut() {
                        if workspace.is_unlocked() {
                            if ui.button("Lock identity").clicked() {
                                workspace.lock();
                                self.signing_confirmation = false;
                                self.notice = "Identity locked. Reading remains available; signing is disabled.".to_string();
                            }
                        } else if ui.button("Unlock identity").clicked() {
                            match workspace.unlock() {
                                Ok(()) => self.notice = "Identity unlocked. Review and confirm before signing.".to_string(),
                                Err(error) => self.notice = format!("Unlock failed: {error}"),
                            }
                        }
                    }
                });
            });
        });

        egui::SidePanel::left("navigation")
            .resizable(false)
            .default_width(232.0)
            .show(ctx, |ui| {
                ui.add_space(SPACE_MD);
                ui.horizontal(|ui| {
                    ui.add_space(SPACE_SM);
                    ui.label(
                        egui::RichText::new("MININET")
                            .strong()
                            .size(18.0)
                            .color(COLOR_TEXT_PRIMARY),
                    );
                });
                ui.add_space(SPACE_LG);
                self.nav_button(ui, View::Home, "★", "Home");
                self.nav_button(ui, View::Discover, "◎", "Discover");
                self.nav_button(ui, View::Communities, "♦", "Communities");
                self.nav_button(ui, View::Inbox, "✉", "Inbox (beta)");
                self.nav_button(ui, View::Creator, "☆", "Creator studio");
                self.nav_button(ui, View::Connections, "↔", "Connections");
                self.nav_button(ui, View::System, "⚙", "System & storage");
                ui.add_space(SPACE_MD);
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Post")
                                .strong()
                                .color(COLOR_TEXT_PRIMARY),
                        )
                        .fill(COLOR_ACCENT)
                        .corner_radius(20.0)
                        .min_size(egui::vec2(ui.available_width(), 40.0)),
                    )
                    .clicked()
                {
                    self.view = View::Home;
                }
                ui.add_space(SPACE_LG);
                ui.separator();
                ui.add_space(SPACE_SM);
                ui.label(
                    egui::RichText::new("CONTROL PLANE")
                        .small()
                        .strong()
                        .color(COLOR_TEXT_SECONDARY),
                );
                ui.add_space(SPACE_XS);
                self.nav_button(ui, View::Privacy, "◕", "Privacy & safety");
                self.nav_button(ui, View::Diagnostics, "✱", "Diagnostics");
                self.nav_button(ui, View::Updates, "↻", "Version & install");
                ui.separator();
                ui.label(
                    egui::RichText::new("No analytics\nNo ad SDKs\nNo embedded web view")
                        .small()
                        .color(COLOR_TEXT_SECONDARY),
                );
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(18.0);
                    self.header(ui);
                    ui.add_space(14.0);
                    match self.view {
                        View::Onboarding => {
                            unreachable!("onboarding returns before the main shell")
                        }
                        View::Home => self.home(ui),
                        View::Inbox => self.inbox(ui),
                        View::Discover => self.discover(ui),
                        View::Communities => self.communities(ui),
                        View::Creator => self.creator(ui),
                        View::Connections => self.connections(ui),
                        View::System => self.system(ui),
                        View::Diagnostics => self.diagnostics(ui),
                        View::Updates => self.updates(ui),
                        View::Privacy => self.privacy(ui),
                    }
                    ui.add_space(18.0);
                });
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(&self.notice)
                        .small()
                        .color(notice_color(&self.notice)),
                );
                ui.separator();
                ui.label(egui::RichText::new("Updates: manual approval").small());
                ui.label(egui::RichText::new("No background sync").small());
            });
        });
    }
}

impl MininetApp {
    fn start_nearby_scan(&mut self) {
        if !self.privacy.lan_discovery {
            self.notice = "Enable local-network discovery in Privacy & safety first.".to_string();
            return;
        }
        if self.discovery_rx.is_some() {
            self.notice = "A nearby profile scan is already running.".to_string();
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.discovery_rx = Some(receiver);
        self.notice = "Scanning the local network for 3 seconds…".to_string();
        std::thread::spawn(move || {
            let _ = sender.send(scan_nearby_profiles());
        });
    }

    fn start_profile_visibility(&mut self) {
        if !self.privacy.lan_discovery {
            self.notice = "Enable local-network discovery in Privacy & safety first.".to_string();
            return;
        }
        if self.sync_rx.is_some() || self.visibility_rx.is_some() {
            self.notice = "A peer operation is already running.".to_string();
            return;
        }
        if self
            .workspace
            .as_ref()
            .is_some_and(Workspace::profile_needs_device_upgrade)
        {
            self.notice = "Upgrade this beta profile for verified peer sync first. Your human DID and public details will stay the same.".to_string();
            return;
        }
        let Some(name) = self
            .workspace
            .as_ref()
            .and_then(Workspace::current_profile)
            .map(|profile| profile.display_name)
        else {
            self.notice = "Publish a public profile before becoming discoverable.".to_string();
            return;
        };
        let port = match self.listen_port.trim().parse::<u16>() {
            Ok(port) if port != 0 => port,
            _ => {
                self.notice = "Enter a valid non-zero listen port.".to_string();
                return;
            }
        };
        let (sender, receiver) = mpsc::channel();
        self.visibility_rx = Some(receiver);
        self.notice = format!(
            "Visible as {name} on the local network for 60 seconds; ready for multiple bounded syncs."
        );
        let root = data_root();
        std::thread::spawn(move || {
            let result =
                run_discoverable_profile_sync(&root, port, &name, Duration::from_secs(60), &sender);
            let _ = sender.send(result);
        });
    }

    fn start_peer_sync(&mut self, listener: bool, context: Option<SyncContext>) -> bool {
        if self.sync_rx.is_some() || self.visibility_rx.is_some() {
            self.notice = "A peer sync is already running.".to_string();
            return false;
        }
        let endpoint = if listener {
            format!("0.0.0.0:{}", self.listen_port.trim())
        } else {
            self.peer_address.trim().to_string()
        };
        let (sender, receiver) = mpsc::channel();
        self.sync_rx = Some(receiver);
        self.sync_context = context;
        self.notice = if listener {
            format!("Listening once on {endpoint}; no other network activity is enabled.")
        } else {
            format!("Connecting once to {endpoint}; the UI remains responsive.")
        };
        let root = data_root();
        std::thread::spawn(move || {
            let result = run_peer_sync(&root, &endpoint, listener);
            let _ = sender.send(result);
        });
        true
    }

    fn start_private_sync(&mut self, listener: bool) {
        if self.sync_rx.is_some() || self.visibility_rx.is_some() {
            self.notice = "A peer sync is already running.".to_string();
            return;
        }
        let Some(index) = self.selected_conversation else {
            self.notice = "Select a conversation before private sync.".to_string();
            return;
        };
        let Some(route) = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.conversations.get(index))
            .map(ConversationRecord::route)
        else {
            self.notice = "Selected conversation is unavailable.".to_string();
            return;
        };
        let endpoint = if listener {
            format!("0.0.0.0:{}", self.listen_port.trim())
        } else {
            self.peer_address.trim().to_string()
        };
        let (sender, receiver) = mpsc::channel();
        self.sync_rx = Some(receiver);
        self.notice = if listener {
            format!("Listening once for the selected conversation on {endpoint}.")
        } else {
            format!("Connecting once for the selected conversation to {endpoint}.")
        };
        let root = data_root();
        std::thread::spawn(move || {
            let result = run_private_sync(&root, &endpoint, listener, route);
            let _ = sender.send(result);
        });
    }

    fn add_friend(&mut self, profile: &mini_social::Profile) {
        if self.sync_rx.is_some() || self.visibility_rx.is_some() {
            self.notice = format!(
                "Finish the active peer operation before adding {}. No request was signed yet.",
                profile.display_name
            );
            return;
        }
        let result = self
            .workspace
            .as_mut()
            .ok_or_else(|| "Local workspace unavailable.".to_string())
            .and_then(|workspace| {
                workspace.set_follow_target_confirmed(profile.human.as_str(), true)
            });
        if let Err(error) = result {
            self.notice = format!("Could not add friend: {error}");
            return;
        }

        let Some(endpoint) = nearby_endpoint_for(&self.nearby_profiles, &profile.human) else {
            self.notice = format!(
                "Friend request for {} is signed locally. Find them nearby again to deliver it.",
                profile.display_name
            );
            return;
        };
        self.peer_address = endpoint.to_string();
        let context = SyncContext::FriendRequest {
            display_name: profile.display_name.clone(),
        };
        if !self.start_peer_sync(false, Some(context)) {
            self.notice = format!(
                "Friend request for {} is signed locally. Finish the active peer operation, then sync to deliver it.",
                profile.display_name
            );
        }
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(15, 20, 29);
        visuals.window_fill = egui::Color32::from_rgb(10, 14, 21);
        visuals.faint_bg_color = egui::Color32::from_rgb(28, 37, 52);
        visuals.extreme_bg_color = egui::Color32::from_rgb(8, 11, 17);
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(20, 27, 38);
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(27, 37, 52);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(42, 61, 82);
        visuals.widgets.active.bg_fill = COLOR_ACCENT.gamma_multiply(0.55);
        visuals.selection.bg_fill = COLOR_ACCENT.gamma_multiply(0.45);
        let mut style = (*ctx.style()).clone();
        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(12.0, 10.0);
        style.spacing.button_padding = egui::vec2(14.0, 9.0);
        style.spacing.window_margin = egui::Margin::same(18);
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(28.0));
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        ctx.set_style(style);
    }

    /// A left-rail nav row: an icon glyph plus a label in a pill that fills
    /// with the accent color when selected — the X-style left nav, not the
    /// plain `SelectableLabel` rectangle this used to be.
    fn nav_button(&mut self, ui: &mut egui::Ui, view: View, icon: &str, label: &str) {
        let selected = self.view == view;
        let desired_size = egui::vec2(ui.available_width(), 40.0);
        let (rect, response) = ui.allocate_exact_size(
            desired_size,
            egui::Sense::click().union(egui::Sense::hover()),
        );
        if response.clicked() {
            self.view = view;
        }
        let bg = if selected {
            COLOR_ACCENT.gamma_multiply(0.22)
        } else if response.hovered() {
            COLOR_CARD_BG
        } else {
            egui::Color32::TRANSPARENT
        };
        let painter = ui.painter();
        painter.rect_filled(rect, 20.0, bg);
        let text_color = if selected {
            COLOR_TEXT_PRIMARY
        } else {
            COLOR_TEXT_SECONDARY
        };
        painter.text(
            rect.left_center() + egui::vec2(16.0, 0.0),
            egui::Align2::LEFT_CENTER,
            icon,
            egui::FontId::proportional(16.0),
            if selected {
                COLOR_ACCENT
            } else {
                COLOR_TEXT_SECONDARY
            },
        );
        painter.text(
            rect.left_center() + egui::vec2(42.0, 0.0),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(14.0),
            text_color,
        );
    }

    fn header(&self, ui: &mut egui::Ui) {
        let (title, subtitle) = match self.view {
            View::Onboarding => (
                "Welcome to Mininet",
                "Create your local root, then publish the public profile you choose to share.",
            ),
            View::Home => (
                "Your feed",
                "A local view of objects your device has received.",
            ),
            View::Inbox => (
                "Inbox beta",
                "Encrypted route-scoped messages with manual trusted invitation and sync.",
            ),
            View::Discover => (
                "Discover",
                "Search signed profiles already on your device, opt-in nearby peers, and (soon) people you may know through mutual vouches.",
            ),
            View::Communities => (
                "Communities",
                "Portable spaces for discussion, not platform-owned silos.",
            ),
            View::Diagnostics => (
                "Diagnostics",
                "Run the real protocol code on this device and read what it actually did.",
            ),
            View::Updates => (
                "Version & install",
                "What is installed, whether it still matches its manifest, and how to go back.",
            ),
            View::Creator => (
                "Creator studio",
                "Publish text, images, clips, and long-form media from one identity.",
            ),
            View::Connections => (
                "Connections",
                "Direct peers, local mesh, and optional relays.",
            ),
            View::System => (
                "Mininet system",
                "Inspect the local object graph and the protocol foundations available to this client.",
            ),
            View::Privacy => (
                "Privacy center",
                "See exactly what this client can and cannot do.",
            ),
        };
        ui.heading(egui::RichText::new(title).color(COLOR_TEXT_PRIMARY));
        ui.label(egui::RichText::new(subtitle).color(COLOR_TEXT_SECONDARY));
    }

    fn onboarding(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(54.0);
                ui.heading(egui::RichText::new("MININET").color(COLOR_TEXT_PRIMARY));
                ui.label(
                    egui::RichText::new("Your identity. Your objects. Your transport choices.")
                        .color(COLOR_TEXT_SECONDARY),
                );
                ui.add_space(SPACE_XL);
                ui.allocate_ui_with_layout(
                    [620.0, ui.available_height()].into(),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let root_created = self
                            .workspace
                            .as_ref()
                            .is_some_and(Workspace::root_created);
                        let has_public_account = self
                            .workspace
                            .as_ref()
                            .is_some_and(Workspace::has_public_account);
                        let is_unlocked = self
                            .workspace
                            .as_ref()
                            .is_some_and(Workspace::is_unlocked);
                        if self.workspace.is_some() {
                            if !root_created {
                                card(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new("1. Create your Mininet root")
                                            .strong()
                                            .size(18.0)
                                            .color(COLOR_TEXT_PRIMARY),
                                    );
                                    ui.add_space(SPACE_XS);
                                    ui.label(egui::RichText::new("This creates a new local signing root protected by the Windows user vault. It never uploads a seed or contacts a server.").color(COLOR_TEXT_SECONDARY));
                                    ui.label(egui::RichText::new("You will be able to export recovery material only through a separate, deliberate backup flow.").color(COLOR_TEXT_SECONDARY));
                                    ui.add_space(SPACE_SM);
                                    if ui
                                        .add(
                                            egui::Button::new("Create local root")
                                                .fill(COLOR_ACCENT.gamma_multiply(0.35)),
                                        )
                                        .clicked()
                                    {
                                        self.notice = match self
                                            .workspace
                                            .as_mut()
                                            .expect("workspace checked above")
                                            .create_root()
                                        {
                                            Ok(()) => "Root created locally. Publish your public account to continue.".to_string(),
                                            Err(error) => format!("Root creation failed: {error}"),
                                        };
                                    }
                                });
                            } else if !has_public_account {
                                card(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new("2. Create your public account")
                                            .strong()
                                            .size(18.0)
                                            .color(COLOR_TEXT_PRIMARY),
                                    );
                                    ui.add_space(SPACE_XS);
                                    ui.label(egui::RichText::new("Start with a display name and optional bio. Next, you can choose a photo, location, age, and any custom public details before becoming visible to anyone.").color(COLOR_TEXT_SECONDARY));
                                    ui.label(egui::RichText::new("Your cryptographic identity remains the DID shown in Privacy & safety.").color(COLOR_TEXT_SECONDARY));
                                    ui.add_space(SPACE_MD);
                                    ui.horizontal(|ui| {
                                        let preview_name = if self.account_name.trim().is_empty()
                                        {
                                            "?".to_string()
                                        } else {
                                            self.account_name.trim().to_string()
                                        };
                                        self.avatar(ui, &preview_name, None, 48.0);
                                        ui.vertical(|ui| {
                                            ui.label(
                                                egui::RichText::new("Display name")
                                                    .small()
                                                    .color(COLOR_TEXT_SECONDARY),
                                            );
                                            ui.text_edit_singleline(&mut self.account_name);
                                        });
                                    });
                                    ui.add_space(SPACE_SM);
                                    ui.label(
                                        egui::RichText::new("Bio")
                                            .small()
                                            .color(COLOR_TEXT_SECONDARY),
                                    );
                                    ui.add_sized(
                                        [ui.available_width(), 90.0],
                                        egui::TextEdit::multiline(&mut self.account_bio),
                                    );
                                    ui.add_space(SPACE_SM);
                                    if !is_unlocked {
                                        ui.label(
                                            egui::RichText::new("This setup action will unlock the local root only long enough to sign the profile, then lock it again.")
                                                .small()
                                                .color(COLOR_TEXT_SECONDARY),
                                        );
                                        ui.add_space(SPACE_XS);
                                    }
                                    if ui
                                        .add(
                                            egui::Button::new("Publish public account")
                                                .fill(COLOR_ACCENT.gamma_multiply(0.35)),
                                        )
                                        .clicked()
                                    {
                                        self.notice = if self.account_name.trim().is_empty() {
                                            "Choose a display name first.".to_string()
                                        } else if let Some(workspace) = self.workspace.as_mut() {
                                            let result = if workspace.is_unlocked() {
                                                workspace.publish_profile(
                                                    self.account_name.trim(),
                                                    self.account_bio.trim(),
                                                )
                                            } else {
                                                workspace.unlock().and_then(|()| {
                                                    workspace.publish_profile(
                                                        self.account_name.trim(),
                                                        self.account_bio.trim(),
                                                    )
                                                })
                                            };
                                            workspace.lock();
                                            match result {
                                                Ok(()) => {
                                                    self.profile_name = self.account_name.trim().to_string();
                                                    self.profile_bio = self.account_bio.trim().to_string();
                                                    self.view = View::Creator;
                                                    "Public account created locally and identity locked again. Add any optional public details below, or open Discover when you are ready.".to_string()
                                                }
                                                Err(error) => format!("Could not create public account: {error}"),
                                            }
                                        } else {
                                            "Local workspace unavailable.".to_string()
                                        };
                                    }
                                });
                            }
                        } else {
                            card(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("The local workspace could not be opened.")
                                        .strong()
                                        .color(COLOR_ERROR),
                                );
                                ui.label(egui::RichText::new(&self.notice).color(COLOR_TEXT_SECONDARY));
                            });
                        }
                        ui.add_space(SPACE_MD);
                        ui.label(
                            egui::RichText::new(&self.notice)
                                .small()
                                .color(notice_color(&self.notice)),
                        );
                    },
                );
            });
        });
    }

    fn home(&mut self, ui: &mut egui::Ui) {
        let is_unlocked = self.workspace.as_ref().is_some_and(Workspace::is_unlocked);
        let own_display_name = self
            .workspace
            .as_ref()
            .and_then(Workspace::current_profile)
            .map(|profile| profile.display_name)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "You".to_string());
        card(ui, |ui| {
            ui.horizontal(|ui| {
                self.avatar(ui, &own_display_name, None, 40.0);
                ui.vertical(|ui| {
                    ui.add_sized(
                        [ui.available_width(), 64.0],
                        egui::TextEdit::multiline(&mut self.composer)
                            .hint_text("Write a post… (saved locally before sync)")
                            .frame(false),
                    );
                });
            });
            ui.add_space(SPACE_SM);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        is_unlocked,
                        egui::Button::new("Publish").fill(COLOR_ACCENT.gamma_multiply(0.35)),
                    )
                    .clicked()
                {
                    self.notice = if self.composer.trim().is_empty() {
                        "Nothing published: write something first.".to_string()
                    } else if let Some(workspace) = self.workspace.as_mut() {
                        match workspace.publish_post(self.composer.trim()) {
                            Ok(()) => {
                                self.composer.clear();
                                "Post written to the local object store. No network used."
                                    .to_string()
                            }
                            Err(error) => format!("Could not publish locally: {error}"),
                        }
                    } else {
                        "Local workspace unavailable; no content was published.".to_string()
                    };
                }
                if ui.button("Attach media").clicked() {
                    self.view = View::Creator;
                }
                if ui.button("Add community").clicked() {
                    self.view = View::Communities;
                }
                if !is_unlocked {
                    ui.label(
                        egui::RichText::new("Unlock your identity to publish")
                            .small()
                            .color(COLOR_TEXT_SECONDARY),
                    );
                }
            });
        });
        ui.add_space(SPACE_LG);
        ui.horizontal(|ui| {
            ui.label("Feed order:");
            egui::ComboBox::from_id_salt("feed_filter")
                .selected_text(match self.feed_filter {
                    FeedFilter::Chronological => "Chronological",
                    FeedFilter::MostSupported => "Most supported",
                    _ => "Custom",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.feed_filter,
                        FeedFilter::Chronological,
                        "Chronological",
                    );
                    ui.selectable_value(
                        &mut self.feed_filter,
                        FeedFilter::MostSupported,
                        "Most supported",
                    );
                });
            ui.label("Ranking is local and user-selected.");
        });
        let items = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.feed(self.feed_filter).ok())
            .unwrap_or_default();
        if items.is_empty() {
            card(ui, |ui| {
                ui.label(egui::RichText::new("No local posts yet.").color(COLOR_TEXT_PRIMARY));
                ui.label(
                    egui::RichText::new(
                        "Your first post stays on this device until you choose a connection path.",
                    )
                    .small()
                    .color(COLOR_TEXT_SECONDARY),
                );
            });
        }
        let cards: Vec<_> = self
            .workspace
            .as_ref()
            .map(|workspace| {
                items
                    .iter()
                    .map(|item| {
                        (
                            item.id.clone(),
                            item.author.clone(),
                            workspace.post_text(item),
                            item.timestamp_ms,
                            match item.reason {
                                mini_social::FeedReason::Own => "Own",
                                mini_social::FeedReason::Followed => "Followed",
                            },
                            item.support_count,
                            workspace.comment_count(&item.id),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (id, author, body, timestamp_ms, reason, support_count, comment_count) in cards {
            self.post_card(
                ui,
                &id,
                &author,
                &body,
                timestamp_ms,
                reason,
                support_count,
                comment_count,
            );
        }
        if let Some(target) = self.reply_target.clone() {
            card(ui, |ui| {
                ui.label(
                    egui::RichText::new("Reply to selected post")
                        .strong()
                        .color(COLOR_TEXT_PRIMARY),
                );
                ui.text_edit_multiline(&mut self.reply_text);
                ui.add_space(SPACE_XS);
                ui.horizontal(|ui| {
                    let is_unlocked = self.workspace.as_ref().is_some_and(Workspace::is_unlocked);
                    if ui
                        .add_enabled(
                            is_unlocked,
                            egui::Button::new("Publish reply")
                                .fill(COLOR_ACCENT.gamma_multiply(0.35)),
                        )
                        .clicked()
                    {
                        self.notice = if self.reply_text.trim().is_empty() {
                            "Write a reply first.".to_string()
                        } else if let Some(workspace) = self.workspace.as_mut() {
                            match workspace.publish_comment(&target, self.reply_text.trim()) {
                                Ok(()) => {
                                    self.reply_text.clear();
                                    self.reply_target = None;
                                    "Reply written locally. No network used.".to_string()
                                }
                                Err(error) => format!("Could not publish reply: {error}"),
                            }
                        } else {
                            "Local workspace unavailable.".to_string()
                        };
                    }
                    if ui.button("Cancel").clicked() {
                        self.reply_target = None;
                        self.reply_text.clear();
                    }
                });
            });
        }
    }

    fn inbox(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Beta security boundary").strong());
            ui.label("Messages are signed and encrypted at rest, and private sync is limited to the selected opaque conversation route.");
            ui.colored_label(
                egui::Color32::YELLOW,
                "Invitation codes contain the conversation key. Anyone who obtains one can read this beta conversation. Transfer it through a trusted channel.",
            );
            ui.label("This beta does not yet provide prekeys, a ratchet, post-compromise recovery, mailbox delivery, or authenticated endpoint discovery.");
        });
        ui.add_space(12.0);

        let conversation_cards: Vec<(usize, String, String)> = self
            .workspace
            .as_ref()
            .map(|workspace| {
                workspace
                    .conversations
                    .iter()
                    .enumerate()
                    .map(|(index, conversation)| {
                        (
                            index,
                            conversation.label.clone(),
                            conversation.peer.as_str().to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        ui.group(|ui| {
            ui.label(egui::RichText::new("Conversations").strong());
            if conversation_cards.is_empty() {
                ui.label("No private conversations are stored in this Windows profile.");
            }
            for (index, label, peer) in &conversation_cards {
                let selected = self.selected_conversation == Some(*index);
                if ui
                    .selectable_label(selected, format!("{label}  ·  {peer}"))
                    .clicked()
                {
                    self.selected_conversation = Some(*index);
                }
            }
        });
        ui.add_space(12.0);

        ui.columns(2, |columns| {
            columns[0].group(|ui| {
                ui.label(egui::RichText::new("Create an invitation").strong());
                ui.label("Local label");
                ui.text_edit_singleline(&mut self.conversation_label);
                ui.label("Intended peer DID");
                ui.text_edit_singleline(&mut self.conversation_peer);
                let valid_peer = Did::parse(self.conversation_peer.trim()).is_ok();
                if !self.conversation_peer.trim().is_empty() && !valid_peer {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "Enter a complete did:mini identifier.",
                    );
                }
                if ui
                    .add_enabled(
                        valid_peer && !self.conversation_label.trim().is_empty(),
                        egui::Button::new("Create sensitive invite"),
                    )
                    .clicked()
                {
                    let label = self.conversation_label.trim().to_string();
                    let peer = self.conversation_peer.trim().to_string();
                    self.notice = if let Some(workspace) = self.workspace.as_mut() {
                        match workspace.create_beta_conversation(&label, &peer) {
                            Ok(invite) => {
                                self.selected_conversation =
                                    Some(workspace.conversations.len().saturating_sub(1));
                                self.conversation_invite = invite;
                                self.conversation_label.clear();
                                self.conversation_peer.clear();
                                "Conversation stored through DPAPI. Transfer the invite securely."
                                    .to_string()
                            }
                            Err(error) => format!("Could not create conversation: {error}"),
                        }
                    } else {
                        "Local workspace unavailable.".to_string()
                    };
                }
            });
            columns[1].group(|ui| {
                ui.label(egui::RichText::new("Import an invitation").strong());
                ui.label("Local label");
                ui.text_edit_singleline(&mut self.import_conversation_label);
                ui.label("Sensitive invitation code");
                ui.add_sized(
                    [ui.available_width(), 72.0],
                    egui::TextEdit::multiline(&mut self.import_conversation_invite)
                        .hint_text("mini-invite-v1.…"),
                );
                if ui.button("Import into protected vault").clicked() {
                    let label = self.import_conversation_label.trim().to_string();
                    let invite = self.import_conversation_invite.trim().to_string();
                    self.notice = if label.is_empty() || invite.is_empty() {
                        "A local label and invitation code are required.".to_string()
                    } else if let Some(workspace) = self.workspace.as_mut() {
                        match workspace.import_beta_conversation(&label, &invite) {
                            Ok(index) => {
                                self.selected_conversation = Some(index);
                                self.import_conversation_label.clear();
                                self.import_conversation_invite.clear();
                                "Conversation capability imported into DPAPI-protected storage."
                                    .to_string()
                            }
                            Err(error) => format!("Could not import conversation: {error}"),
                        }
                    } else {
                        "Local workspace unavailable.".to_string()
                    };
                }
            });
        });

        if !self.conversation_invite.is_empty() {
            ui.add_space(10.0);
            ui.group(|ui| {
                ui.label(egui::RichText::new("Sensitive invite — grants message access").strong());
                ui.add_sized(
                    [ui.available_width(), 72.0],
                    egui::TextEdit::multiline(&mut self.conversation_invite),
                );
                if ui.button("Copy sensitive invite").clicked() {
                    ui.ctx().copy_text(self.conversation_invite.clone());
                    self.notice = "Sensitive invite copied. Clipboard-reading software may access it; clear the clipboard after transfer.".to_string();
                }
            });
        }

        let Some(selected) = self.selected_conversation else {
            return;
        };
        let selected_card = conversation_cards
            .iter()
            .find(|(index, _, _)| *index == selected)
            .cloned();
        let Some((_, label, peer)) = selected_card else {
            self.selected_conversation = None;
            return;
        };

        ui.add_space(12.0);
        ui.group(|ui| {
            ui.heading(&label);
            ui.label(format!("Claimed peer: {peer}"));
            ui.label(egui::RichText::new("Message signatures are retained, but this beta view does not yet prove current device delegation/provenance.").small());
            let scan = self
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.private_messages(selected).ok());
            if let Some(scan) = scan {
                if scan.messages.is_empty() {
                    ui.label("No messages on this device yet.");
                }
                let own_did = self
                    .workspace
                    .as_ref()
                    .and_then(|workspace| workspace.human.as_ref());
                for message in scan.messages {
                    ui.group(|ui| {
                        let sender = if own_did == Some(&message.author_human) {
                            "You".to_string()
                        } else if message.author_human.as_str() == peer {
                            label.clone()
                        } else {
                            message.author_human.as_str().to_string()
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new(sender).strong());
                            ui.label(
                                egui::RichText::new(format!(
                                    "sequence {} · {}",
                                    message.sequence, message.timestamp_ms
                                ))
                                .small()
                                .color(egui::Color32::GRAY),
                            );
                        });
                        ui.label(message.body);
                    });
                }
                if !scan.rejected.is_empty() {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!(
                            "{} envelope(s) could not be decrypted or validated.",
                            scan.rejected.len()
                        ),
                    );
                }
            } else {
                ui.colored_label(egui::Color32::YELLOW, "Conversation could not be decrypted.");
            }
            ui.add_sized(
                [ui.available_width(), 64.0],
                egui::TextEdit::multiline(&mut self.message_text).hint_text("Write a private message"),
            );
            ui.checkbox(
                &mut self.signing_confirmation,
                "I confirm this creates a signed encrypted message",
            );
            if ui.button("Send to local outbox").clicked() {
                let body = self.message_text.trim().to_string();
                self.notice = if body.is_empty() {
                    "Write a message first.".to_string()
                } else if !self.signing_confirmation {
                    "Confirm signing before sending.".to_string()
                } else if let Some(workspace) = self.workspace.as_mut() {
                    match workspace.send_private_message(selected, &body) {
                        Ok(()) => {
                            self.message_text.clear();
                            self.signing_confirmation = false;
                            "Encrypted message stored locally. Sync the conversation to deliver it."
                                .to_string()
                        }
                        Err(error) => format!("Could not send message: {error}"),
                    }
                } else {
                    "Local workspace unavailable.".to_string()
                };
            }
        });

        ui.add_space(12.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("Deliver selected conversation").strong());
            ui.label("Both peers must select the same imported conversation. The route check completes before message IDs are exchanged.");
            ui.horizontal(|ui| {
                ui.label("Peer address");
                ui.text_edit_singleline(&mut self.peer_address);
            });
            ui.horizontal(|ui| {
                ui.label("Listen port");
                ui.text_edit_singleline(&mut self.listen_port);
            });
            ui.horizontal(|ui| {
                if ui.button("Connect and sync conversation").clicked() {
                    self.start_private_sync(false);
                }
                if ui.button("Listen once for conversation").clicked() {
                    self.start_private_sync(true);
                }
            });
            ui.label("Foreground only: no background mailbox, retry loop, push service, or always-on listener.");
        });
    }

    fn profile_texture(
        &mut self,
        ctx: &egui::Context,
        avatar: &mini_objects::ObjectId,
    ) -> Option<egui::TextureHandle> {
        if let Some(texture) = self.profile_textures.get(avatar.as_str()) {
            return Some(texture.clone());
        }
        let bytes = self.workspace.as_ref()?.profile_image(avatar).ok()?;
        let image = decode_profile_image(&bytes).ok()?.0.thumbnail(160, 160);
        let rgba = image.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        let texture = ctx.load_texture(
            format!("profile:{}", avatar.as_str()),
            color,
            egui::TextureOptions::LINEAR,
        );
        self.profile_textures
            .insert(avatar.as_str().to_string(), texture.clone());
        Some(texture)
    }

    /// Draw an avatar: the real cached profile photo for `avatar` when one
    /// is available, otherwise a colored badge with `display_name`'s first
    /// letter. Every card that names someone gets an identity anchor this
    /// way instead of a bare dot or nothing at all.
    fn avatar(
        &mut self,
        ui: &mut egui::Ui,
        display_name: &str,
        avatar: Option<&mini_objects::ObjectId>,
        diameter: f32,
    ) {
        let texture = avatar.and_then(|id| self.profile_texture(ui.ctx(), id));
        if let Some(texture) = texture {
            ui.add(
                egui::Image::new((texture.id(), egui::vec2(diameter, diameter)))
                    .corner_radius(diameter / 2.0),
            );
        } else {
            let letter = display_name
                .trim()
                .chars()
                .next()
                .map(|ch| ch.to_ascii_uppercase())
                .unwrap_or('?');
            let (rect, _response) =
                ui.allocate_exact_size(egui::vec2(diameter, diameter), egui::Sense::hover());
            let painter = ui.painter();
            painter.circle_filled(rect.center(), diameter / 2.0, avatar_color(display_name));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                letter,
                egui::FontId::proportional(diameter * 0.45),
                COLOR_TEXT_PRIMARY,
            );
        }
    }

    fn discover(&mut self, ui: &mut egui::Ui) {
        let profile_needs_upgrade = self
            .workspace
            .as_ref()
            .is_some_and(Workspace::profile_needs_device_upgrade);
        if profile_needs_upgrade {
            card(ui, |ui| {
                ui.label(
                    egui::RichText::new("One-time verified-sync upgrade")
                        .strong()
                        .color(COLOR_ERROR),
                );
                ui.label(egui::RichText::new("This account was created by an earlier desktop beta that signed directly with the human root. Peers correctly reject those objects. Re-sign the same public profile with a scoped delegated device; your DID and published details stay unchanged.").color(COLOR_TEXT_SECONDARY));
                ui.add_space(SPACE_SM);
                if ui
                    .add(
                        egui::Button::new("Upgrade public profile for peer sync")
                            .fill(COLOR_ACCENT.gamma_multiply(0.35)),
                    )
                    .clicked()
                {
                    self.notice = match self.workspace.as_mut() {
                        Some(workspace) => match workspace.upgrade_profile_for_sync() {
                            Ok(()) => "Public profile upgraded with a delegated-device signature. Nearby verified sync is ready.".to_string(),
                            Err(error) => format!("Could not upgrade public profile: {error}"),
                        },
                        None => "Local workspace unavailable.".to_string(),
                    };
                }
            });
            ui.add_space(SPACE_MD);
        }
        card(ui, |ui| {
            ui.label(
                egui::RichText::new("Find people")
                    .strong()
                    .color(COLOR_TEXT_PRIMARY),
            );
            ui.add_sized(
                [ui.available_width(), 34.0],
                egui::TextEdit::singleline(&mut self.people_search)
                    .hint_text("Search locally by display name or did:mini identifier"),
            );
            ui.label(
                egui::RichText::new(
                    "Names are searchable labels and are not unique. The DID remains the identity anchor.",
                )
                .small()
                .color(COLOR_TEXT_SECONDARY),
            );
            if ui
                .checkbox(
                    &mut self.privacy.lan_discovery,
                    "Allow opt-in nearby discovery on this local network",
                )
                .changed()
            {
                self.notice = match save_privacy_settings(self.privacy) {
                    Ok(()) => {
                        "Nearby discovery preference saved in the Windows user vault.".to_string()
                    }
                    Err(error) => format!("Could not save discovery preference: {error}"),
                };
            }
            ui.add_space(SPACE_XS);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Find nearby for 3 seconds").clicked() {
                    self.start_nearby_scan();
                }
                if ui.button("Be visible nearby for 60 seconds").clicked() {
                    self.start_profile_visibility();
                }
            });
            ui.label(
                egui::RichText::new(
                    "Nearby visibility reveals your chosen display name and DID to the local network only during this window.",
                )
                .small()
                .color(COLOR_TEXT_SECONDARY),
            );
        });

        let query = self.people_search.trim().to_lowercase();
        let own_did = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.human.clone());
        let nearby: Vec<NearbyProfile> = self
            .nearby_profiles
            .iter()
            .filter(|profile| {
                own_did.as_ref() != Some(&profile.did)
                    && (query.is_empty()
                        || profile.display_name.to_lowercase().contains(&query)
                        || profile.did.as_str().to_lowercase().contains(&query))
            })
            .cloned()
            .collect();
        if !nearby.is_empty() {
            ui.add_space(SPACE_MD);
            ui.label(
                egui::RichText::new("Nearby — not yet verified")
                    .strong()
                    .color(COLOR_TEXT_PRIMARY),
            );
            for profile in nearby {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        self.avatar(ui, &profile.display_name, None, 48.0);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(&profile.display_name)
                                    .strong()
                                    .color(COLOR_TEXT_PRIMARY),
                            );
                            ui.label(
                                egui::RichText::new(profile.did.as_str())
                                    .small()
                                    .color(COLOR_TEXT_SECONDARY),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new("Sync signed profile")
                                        .fill(COLOR_ACCENT.gamma_multiply(0.35)),
                                )
                                .clicked()
                            {
                                self.peer_address = profile.address.to_string();
                                self.start_peer_sync(false, None);
                            }
                        });
                    });
                    ui.add_space(SPACE_XS);
                    ui.label(
                        egui::RichText::new(
                            "This LAN announcement can be spoofed. Sync and verify the signed profile before trusting its name or details.",
                        )
                        .small()
                        .color(COLOR_TEXT_SECONDARY),
                    );
                });
                ui.add_space(SPACE_SM);
            }
        }

        ui.add_space(SPACE_MD);
        ui.label(
            egui::RichText::new("Signed profiles on this device")
                .strong()
                .color(COLOR_TEXT_PRIMARY),
        );
        let profiles: Vec<mini_social::Profile> = self
            .workspace
            .as_ref()
            .map(Workspace::known_profiles)
            .unwrap_or_default()
            .into_iter()
            .filter(|profile| {
                query.is_empty()
                    || profile.display_name.to_lowercase().contains(&query)
                    || profile.human.as_str().to_lowercase().contains(&query)
            })
            .collect();
        if profiles.is_empty() {
            card(ui, |ui| {
                ui.label(egui::RichText::new("No matching signed profiles are present yet. Ask the other instance to become visible, then sync its profile.").color(COLOR_TEXT_SECONDARY));
            });
        }
        for profile in profiles {
            let is_own = own_did.as_ref() == Some(&profile.human);
            let follows = self
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.follows(&profile.human));
            let friend = self
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.is_friend(&profile.human));
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    self.avatar(ui, &profile.display_name, profile.avatar.as_ref(), 56.0);
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(&profile.display_name)
                                .strong()
                                .size(16.0)
                                .color(COLOR_TEXT_PRIMARY),
                        );
                        ui.label(
                            egui::RichText::new(profile.human.as_str())
                                .small()
                                .color(COLOR_TEXT_SECONDARY),
                        );
                        if !profile.bio.trim().is_empty() {
                            ui.label(egui::RichText::new(&profile.bio).color(COLOR_TEXT_PRIMARY));
                        }
                        ui.horizontal_wrapped(|ui| {
                            if let Some(location) = &profile.location {
                                ui.label(
                                    egui::RichText::new(format!("Location: {location}"))
                                        .small()
                                        .color(COLOR_TEXT_SECONDARY),
                                );
                            }
                            if let Some(age) = profile.age {
                                ui.label(
                                    egui::RichText::new(format!("Age: {age}"))
                                        .small()
                                        .color(COLOR_TEXT_SECONDARY),
                                );
                            }
                        });
                    });
                });
                if !profile.fields.is_empty() {
                    ui.add_space(SPACE_XS);
                    for field in &profile.fields {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{}:", field.label))
                                    .strong()
                                    .small()
                                    .color(COLOR_TEXT_SECONDARY),
                            );
                            ui.label(egui::RichText::new(&field.value).color(COLOR_TEXT_PRIMARY));
                        });
                    }
                }
                ui.add_space(SPACE_SM);
                ui.horizontal_wrapped(|ui| {
                    if is_own {
                        ui.label(
                            egui::RichText::new("This is your public profile.")
                                .color(COLOR_TEXT_SECONDARY),
                        );
                    } else if friend {
                        ui.label(
                            egui::RichText::new("Friends")
                                .strong()
                                .color(COLOR_SUCCESS),
                        );
                        if ui.button("Remove friend").clicked() {
                            self.notice = if let Some(workspace) = self.workspace.as_mut() {
                                match workspace
                                    .set_follow_target_confirmed(profile.human.as_str(), false)
                                {
                                    Ok(()) => "Friend/follow edge removed locally; sync to share the change.".to_string(),
                                    Err(error) => format!("Could not remove friend: {error}"),
                                }
                            } else {
                                "Local workspace unavailable.".to_string()
                            };
                        }
                    } else if follows {
                        ui.label(
                            egui::RichText::new("Friend request/follow sent")
                                .color(COLOR_TEXT_SECONDARY),
                        );
                        if ui.button("Cancel").clicked() {
                            self.notice = if let Some(workspace) = self.workspace.as_mut() {
                                match workspace
                                    .set_follow_target_confirmed(profile.human.as_str(), false)
                                {
                                    Ok(()) => "Friend request/follow removed locally.".to_string(),
                                    Err(error) => format!("Could not remove follow: {error}"),
                                }
                            } else {
                                "Local workspace unavailable.".to_string()
                            };
                        }
                    } else if ui
                        .add(
                            egui::Button::new("Add friend")
                                .fill(COLOR_ACCENT.gamma_multiply(0.35)),
                        )
                        .clicked()
                    {
                        self.add_friend(&profile);
                    }
                    if ui.button("Copy DID").clicked() {
                        ui.ctx().copy_text(profile.human.as_str().to_string());
                    }
                });
            });
            ui.add_space(SPACE_SM);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn post_card(
        &mut self,
        ui: &mut egui::Ui,
        id: &mini_objects::ObjectId,
        author: &Did,
        body: &str,
        timestamp_ms: u64,
        reason: &str,
        support_count: usize,
        comment_count: usize,
    ) {
        let profile = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.profile_for(author));
        let display_name = profile
            .as_ref()
            .map(|profile| profile.display_name.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| short_scid(author));
        let avatar_id = profile.as_ref().and_then(|profile| profile.avatar.clone());
        let is_unlocked = self.workspace.as_ref().is_some_and(Workspace::is_unlocked);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                self.avatar(ui, &display_name, avatar_id.as_ref(), 40.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&display_name)
                                .strong()
                                .color(COLOR_TEXT_PRIMARY),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "· {}",
                                relative_time(now_ms(), timestamp_ms)
                            ))
                            .small()
                            .color(COLOR_TEXT_SECONDARY),
                        );
                    });
                    ui.label(egui::RichText::new(body).color(COLOR_TEXT_PRIMARY));
                });
            });
            ui.add_space(SPACE_SM);
            ui.label(
                egui::RichText::new(format!(
                    "{support_count} like{} · {comment_count} repl{} · why here: {reason}",
                    if support_count == 1 { "" } else { "s" },
                    if comment_count == 1 { "y" } else { "ies" },
                ))
                .small()
                .color(COLOR_TEXT_SECONDARY),
            );
            ui.add_space(SPACE_XS);
            ui.horizontal(|ui| {
                if ui.button("Reply").clicked() {
                    self.reply_target = Some(id.clone());
                }
                if ui
                    .add_enabled(
                        is_unlocked,
                        egui::Button::new("Like").fill(COLOR_ACCENT.gamma_multiply(0.25)),
                    )
                    .clicked()
                {
                    self.notice = if let Some(workspace) = self.workspace.as_mut() {
                        match workspace.react_like(id) {
                            Ok(()) => "Like written locally. No network used.".to_string(),
                            Err(error) => format!("Could not react: {error}"),
                        }
                    } else {
                        "Local workspace unavailable.".to_string()
                    };
                }
            });
        });
        ui.add_space(SPACE_SM);
    }

    /// A Reddit-style vote/reply/author row for one community post, plus
    /// its nested reply thread below it.
    fn community_post_card(&mut self, ui: &mut egui::Ui, post: &CommunityPost) {
        let profile = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.profile_for(&post.author));
        let display_name = profile
            .as_ref()
            .map(|profile| profile.display_name.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| short_scid(&post.author));
        let avatar_id = profile.as_ref().and_then(|profile| profile.avatar.clone());
        let is_unlocked = self.workspace.as_ref().is_some_and(Workspace::is_unlocked);
        let body = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.post_body(&post.id));
        card(ui, |ui| {
            ui.horizontal(|ui| {
                self.avatar(ui, &display_name, avatar_id.as_ref(), 36.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&display_name)
                                .strong()
                                .color(COLOR_TEXT_PRIMARY),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "· {}",
                                relative_time(now_ms(), post.timestamp_ms)
                            ))
                            .small()
                            .color(COLOR_TEXT_SECONDARY),
                        );
                    });
                    if let Some(body) = &body {
                        ui.label(egui::RichText::new(body).color(COLOR_TEXT_PRIMARY));
                    }
                });
            });
            ui.add_space(SPACE_SM);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(is_unlocked, egui::Button::new("Upvote"))
                    .clicked()
                {
                    self.notice = if let Some(workspace) = self.workspace.as_mut() {
                        match workspace.react_vote(&post.id, true) {
                            Ok(()) => "Upvote written locally.".to_string(),
                            Err(error) => format!("Could not vote: {error}"),
                        }
                    } else {
                        "Local workspace unavailable.".to_string()
                    };
                }
                ui.label(
                    egui::RichText::new(post.vote_score.to_string())
                        .strong()
                        .color(COLOR_TEXT_PRIMARY),
                );
                if ui
                    .add_enabled(is_unlocked, egui::Button::new("Downvote"))
                    .clicked()
                {
                    self.notice = if let Some(workspace) = self.workspace.as_mut() {
                        match workspace.react_vote(&post.id, false) {
                            Ok(()) => "Downvote written locally.".to_string(),
                            Err(error) => format!("Could not vote: {error}"),
                        }
                    } else {
                        "Local workspace unavailable.".to_string()
                    };
                }
                if ui.button("Reply").clicked() {
                    self.reply_target = Some(post.id.clone());
                }
            });
            ui.add_space(SPACE_SM);
            self.comment_thread(ui, &post.id.clone(), 0);
        });
        ui.add_space(SPACE_SM);
    }

    /// Recursively render every reply to `parent`, indented one step per
    /// depth level. Bounded to `MAX_THREAD_DEPTH` so a pathological or
    /// adversarial reply chain cannot make this recurse unboundedly deep.
    fn comment_thread(&mut self, ui: &mut egui::Ui, parent: &mini_objects::ObjectId, depth: u8) {
        const MAX_THREAD_DEPTH: u8 = 6;
        if depth >= MAX_THREAD_DEPTH {
            return;
        }
        let children = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.comment_children(parent))
            .unwrap_or_default();
        for comment in children {
            let profile = self
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.profile_for(&comment.author));
            let display_name = profile
                .as_ref()
                .map(|profile| profile.display_name.clone())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| short_scid(&comment.author));
            let avatar_id = profile.as_ref().and_then(|profile| profile.avatar.clone());
            ui.horizontal(|ui| {
                ui.add_space(SPACE_LG * f32::from(depth));
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        self.avatar(ui, &display_name, avatar_id.as_ref(), 24.0);
                        ui.label(
                            egui::RichText::new(&display_name)
                                .strong()
                                .small()
                                .color(COLOR_TEXT_PRIMARY),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "· {}",
                                relative_time(now_ms(), comment.timestamp_ms)
                            ))
                            .small()
                            .color(COLOR_TEXT_SECONDARY),
                        );
                    });
                    ui.label(egui::RichText::new(&comment.text).color(COLOR_TEXT_PRIMARY));
                    if ui.small_button("Reply").clicked() {
                        self.reply_target = Some(comment.id.clone());
                    }
                    self.comment_thread(ui, &comment.id.clone(), depth + 1);
                });
            });
            ui.add_space(SPACE_XS);
        }
    }

    fn communities(&mut self, ui: &mut egui::Ui) {
        let is_unlocked = self.workspace.as_ref().is_some_and(Workspace::is_unlocked);
        card(ui, |ui| {
            ui.label(
                egui::RichText::new("Create a local community")
                    .strong()
                    .color(COLOR_TEXT_PRIMARY),
            );
            ui.text_edit_singleline(&mut self.community_name);
            ui.add_sized(
                [ui.available_width(), 48.0],
                egui::TextEdit::multiline(&mut self.community_charter)
                    .hint_text("Charter and norms"),
            );
            ui.add_space(SPACE_XS);
            if ui
                .add_enabled(
                    is_unlocked,
                    egui::Button::new("Publish community").fill(COLOR_ACCENT.gamma_multiply(0.35)),
                )
                .clicked()
            {
                self.notice = if self.community_name.trim().is_empty() {
                    "A community name is required.".to_string()
                } else if let Some(workspace) = self.workspace.as_mut() {
                    match workspace.publish_community(
                        self.community_name.trim(),
                        self.community_charter.trim(),
                    ) {
                        Ok(()) => {
                            self.community_name.clear();
                            self.community_charter.clear();
                            "Community card written locally. No directory was contacted."
                                .to_string()
                        }
                        Err(error) => format!("Could not create community: {error}"),
                    }
                } else {
                    "Local workspace unavailable.".to_string()
                };
            }
        });
        ui.add_space(SPACE_MD);
        let cards = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.communities())
            .unwrap_or_default();
        if cards.is_empty() {
            ui.label(
                egui::RichText::new("No community cards are present locally yet.")
                    .color(COLOR_TEXT_SECONDARY),
            );
        }
        for (id, name, charter, member_count, joined) in cards {
            let posts = self
                .workspace
                .as_ref()
                .map(|workspace| workspace.community_feed(&id))
                .unwrap_or_default();
            card(ui, |ui| {
                ui.label(egui::RichText::new(name).strong().color(COLOR_TEXT_PRIMARY));
                ui.label(egui::RichText::new(charter).color(COLOR_TEXT_PRIMARY));
                ui.label(
                    egui::RichText::new(format!("{member_count} locally known members"))
                        .small()
                        .color(COLOR_TEXT_SECONDARY),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            is_unlocked,
                            egui::Button::new(if joined {
                                "Leave community"
                            } else {
                                "Join community"
                            }),
                        )
                        .clicked()
                    {
                        self.notice = if let Some(workspace) = self.workspace.as_mut() {
                            match workspace.set_community_membership(&id, !joined) {
                                Ok(()) => {
                                    if joined {
                                        "Leave object written locally.".to_string()
                                    } else {
                                        "Join object written locally.".to_string()
                                    }
                                }
                                Err(error) => format!("Could not change membership: {error}"),
                            }
                        } else {
                            "Local workspace unavailable.".to_string()
                        };
                    }
                    let composing = self.open_community_composer.as_ref() == Some(&id);
                    if ui
                        .add_enabled(is_unlocked, egui::Button::new("Post"))
                        .clicked()
                    {
                        self.open_community_composer =
                            if composing { None } else { Some(id.clone()) };
                        self.community_post_text.clear();
                    }
                });
                if self.open_community_composer.as_ref() == Some(&id) {
                    ui.add_space(SPACE_XS);
                    ui.add_sized(
                        [ui.available_width(), 56.0],
                        egui::TextEdit::multiline(&mut self.community_post_text)
                            .hint_text("Write a post for this community…"),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new("Publish")
                                    .fill(COLOR_ACCENT.gamma_multiply(0.35)),
                            )
                            .clicked()
                        {
                            self.notice = if self.community_post_text.trim().is_empty() {
                                "Write something first.".to_string()
                            } else if let Some(workspace) = self.workspace.as_mut() {
                                match workspace
                                    .publish_community_post(&id, self.community_post_text.trim())
                                {
                                    Ok(()) => {
                                        self.community_post_text.clear();
                                        self.open_community_composer = None;
                                        "Community post written locally.".to_string()
                                    }
                                    Err(error) => format!("Could not publish: {error}"),
                                }
                            } else {
                                "Local workspace unavailable.".to_string()
                            };
                        }
                        if ui.button("Cancel").clicked() {
                            self.open_community_composer = None;
                            self.community_post_text.clear();
                        }
                    });
                }
                if !posts.is_empty() {
                    ui.add_space(SPACE_SM);
                    ui.separator();
                    ui.add_space(SPACE_XS);
                    ui.label(
                        egui::RichText::new(format!("{} post(s) in this community", posts.len()))
                            .small()
                            .color(COLOR_TEXT_SECONDARY),
                    );
                }
            });
            for post in &posts {
                self.community_post_card(ui, post);
            }
            ui.add_space(SPACE_SM);
        }
        ui.add_space(SPACE_MD);
        ui.label(egui::RichText::new("Community content remains fetchable by object id. Labels and local filters can change your view; they do not erase the author's copy.").italics().color(COLOR_TEXT_SECONDARY));
    }

    fn creator(&mut self, ui: &mut egui::Ui) {
        let is_unlocked = self.workspace.as_ref().is_some_and(Workspace::is_unlocked);
        card(ui, |ui| {
            ui.label(
                egui::RichText::new("Your public profile")
                    .strong()
                    .color(COLOR_TEXT_PRIMARY),
            );
            ui.label(egui::RichText::new("You choose every optional detail below. Only the display name is required; blank or disabled fields are not published.").small().color(COLOR_TEXT_SECONDARY));
            ui.add_space(SPACE_SM);
            ui.label("Display name");
            ui.add_sized(
                [ui.available_width(), 32.0],
                egui::TextEdit::singleline(&mut self.profile_name)
                    .hint_text("The name people can search for"),
            );
            ui.label("Bio (optional)");
            ui.add_sized(
                [ui.available_width(), 64.0],
                egui::TextEdit::multiline(&mut self.profile_bio)
                    .hint_text("A short introduction, interests, or what you make"),
            );
            ui.separator();
            ui.label(
                egui::RichText::new("Profile photo (optional)")
                    .strong()
                    .color(COLOR_TEXT_PRIMARY),
            );
            if self.profile_avatar.is_some() && !self.profile_remove_photo {
                ui.horizontal(|ui| {
                    self.avatar(
                        ui,
                        &self.profile_name.clone(),
                        self.profile_avatar.clone().as_ref(),
                        48.0,
                    );
                    ui.label(
                        egui::RichText::new("A profile photo is currently published.")
                            .color(COLOR_TEXT_SECONDARY),
                    );
                });
            }
            ui.horizontal_wrapped(|ui| {
                ui.label("Drop an image onto this window or paste its local path:");
                ui.text_edit_singleline(&mut self.profile_photo_path);
            });
            ui.label(egui::RichText::new("PNG, JPEG, WebP, or GIF; maximum 8 MiB. The image is stored as signed Mininet media, not uploaded to a third party.").small().color(COLOR_TEXT_SECONDARY));
            if self.profile_avatar.is_some() {
                ui.checkbox(
                    &mut self.profile_remove_photo,
                    "Remove my currently published photo",
                );
            }
            ui.separator();
            ui.checkbox(
                &mut self.profile_share_location,
                "Publish a location I choose",
            );
            if self.profile_share_location {
                ui.add_sized(
                    [ui.available_width(), 32.0],
                    egui::TextEdit::singleline(&mut self.profile_location)
                        .hint_text("For example: Manchester, UK (avoid a precise address)"),
                );
                ui.label(
                    egui::RichText::new(
                        "Tip: a city or region is usually safer than a home or live location.",
                    )
                    .small()
                    .color(COLOR_TEXT_SECONDARY),
                );
            }
            ui.checkbox(&mut self.profile_share_age, "Publish my age");
            if self.profile_share_age {
                ui.add_sized(
                    [140.0, 32.0],
                    egui::TextEdit::singleline(&mut self.profile_age).hint_text("Age"),
                );
            }
            ui.separator();
            ui.label(
                egui::RichText::new("Custom public details (optional)")
                    .strong()
                    .color(COLOR_TEXT_PRIMARY),
            );
            ui.label(egui::RichText::new("Add one Label: Value pair per line, such as Pronouns, Website, Languages, Interests, or Availability.").small().color(COLOR_TEXT_SECONDARY));
            ui.add_sized(
                [ui.available_width(), 96.0],
                egui::TextEdit::multiline(&mut self.profile_custom_fields)
                    .hint_text("Pronouns: they/them\nWebsite: https://example.org\nLanguages: English, Slovene"),
            );
            ui.add_space(SPACE_XS);
            if ui
                .add_enabled(
                    is_unlocked,
                    egui::Button::new("Save public profile")
                        .fill(COLOR_ACCENT.gamma_multiply(0.35)),
                )
                .clicked()
            {
                let fields = parse_profile_fields(&self.profile_custom_fields);
                let age = if self.profile_share_age {
                    self.profile_age
                        .trim()
                        .parse::<u8>()
                        .map(Some)
                        .map_err(|_| "age must be a whole number from 1 to 255".to_string())
                        .and_then(|age| {
                            if age == Some(0) {
                                Err("age must be a whole number from 1 to 255".to_string())
                            } else {
                                Ok(age)
                            }
                        })
                } else {
                    Ok(None)
                };
                let location = self.profile_location.trim();
                let validation = fields.and_then(|fields| {
                    if self.profile_name.trim().is_empty() {
                        Err("A display name is required.".to_string())
                    } else if self.profile_share_location && location.is_empty() {
                        Err("Enter a location or turn off location sharing.".to_string())
                    } else if location.len() > MAX_LOCATION_BYTES {
                        Err(format!("location exceeds {MAX_LOCATION_BYTES} bytes"))
                    } else {
                        age.map(|age| (fields, age))
                    }
                });
                self.notice = match validation {
                    Err(error) => error,
                    Ok((fields, age)) => {
                        let retained_avatar = if self.profile_remove_photo {
                            None
                        } else {
                            self.profile_avatar.as_ref()
                        };
                        if let Some(workspace) = self.workspace.as_mut() {
                            match workspace.publish_custom_profile_confirmed(
                                self.profile_name.trim(),
                                self.profile_bio.trim(),
                                self.profile_photo_path.trim(),
                                retained_avatar,
                                self.profile_share_location.then_some(location),
                                age,
                                &fields,
                            ) {
                                Ok(avatar) => {
                                    self.profile_avatar = avatar;
                                    self.profile_photo_path.clear();
                                    self.profile_remove_photo = false;
                                    self.profile_textures.clear();
                                    "Public profile saved locally. Use Discover to become visible nearby or sync it to another peer.".to_string()
                                }
                                Err(error) => format!("Could not publish profile: {error}"),
                            }
                        } else {
                            "Local workspace unavailable.".to_string()
                        }
                    }
                };
            }
        });
        ui.add_space(SPACE_LG);
        card(ui, |ui| {
            ui.label(
                egui::RichText::new("Public wall")
                    .strong()
                    .color(COLOR_TEXT_PRIMARY),
            );
            ui.label(egui::RichText::new("A voluntary public-facing surface separate from your profile. It does not reveal another root unless you explicitly publish a linkage object.").small().color(COLOR_TEXT_SECONDARY));
            ui.add_space(SPACE_SM);
            ui.label("Wall name");
            ui.text_edit_singleline(&mut self.wall_name);
            ui.label("Wall bio");
            ui.add_sized(
                [ui.available_width(), 64.0],
                egui::TextEdit::multiline(&mut self.wall_bio),
            );
            ui.label("Public links (one per line, optional)");
            ui.text_edit_multiline(&mut self.wall_links);
            ui.checkbox(
                &mut self.wall_unlisted,
                "Make this wall unlisted (resolvable only by direct identifier)",
            );
            ui.add_space(SPACE_XS);
            if ui
                .add_enabled(
                    is_unlocked,
                    egui::Button::new("Publish wall").fill(COLOR_ACCENT.gamma_multiply(0.35)),
                )
                .clicked()
            {
                let link_values: Vec<String> = self
                    .wall_links
                    .lines()
                    .map(str::trim)
                    .filter(|link| !link.is_empty())
                    .map(str::to_string)
                    .collect();
                let link_refs: Vec<&str> = link_values.iter().map(String::as_str).collect();
                self.notice = if self.wall_name.trim().is_empty() {
                    "A wall name is required.".to_string()
                } else if let Some(workspace) = self.workspace.as_mut() {
                    match workspace.publish_public_wall(
                        self.wall_name.trim(),
                        self.wall_bio.trim(),
                        &link_refs,
                        self.wall_unlisted,
                    ) {
                        Ok(()) => {
                            "Public wall written locally. No directory was contacted.".to_string()
                        }
                        Err(error) => format!("Could not publish public wall: {error}"),
                    }
                } else {
                    "Local workspace unavailable.".to_string()
                };
            }
        });
        ui.add_space(SPACE_LG);
        let target_valid =
            self.follow_target.trim().is_empty() || Did::parse(self.follow_target.trim()).is_ok();
        card(ui, |ui| {
            ui.label(
                egui::RichText::new("People and follows")
                    .strong()
                    .color(COLOR_TEXT_PRIMARY),
            );
            ui.label(egui::RichText::new("Exchange the full did:mini identifier through a trusted channel. Usernames are not unique contact identifiers.").small().color(COLOR_TEXT_SECONDARY));
            ui.add_space(SPACE_SM);
            if let Some(workspace) = self.workspace.as_ref() {
                if let Some(human) = workspace.human.as_ref() {
                    ui.horizontal(|ui| {
                        ui.label("Your DID");
                        let mut did_text = human.as_str().to_string();
                        ui.add_sized(
                            [ui.available_width() - 86.0, 30.0],
                            egui::TextEdit::singleline(&mut did_text).interactive(false),
                        );
                        if ui.button("Copy DID").clicked() {
                            ui.ctx().copy_text(did_text);
                        }
                    });
                }
            }
            ui.horizontal(|ui| {
                ui.label("Friend's DID");
                ui.add_sized(
                    [ui.available_width(), 30.0],
                    egui::TextEdit::singleline(&mut self.follow_target).hint_text("did:mini:..."),
                );
            });
            if !target_valid {
                ui.colored_label(
                    COLOR_ERROR,
                    "Enter a complete did:mini identifier, not a display name.",
                );
            }
            ui.add_space(SPACE_XS);
            ui.horizontal(|ui| {
                let can_act = is_unlocked && target_valid && !self.follow_target.trim().is_empty();
                if ui
                    .add_enabled(
                        can_act,
                        egui::Button::new("Follow").fill(COLOR_ACCENT.gamma_multiply(0.35)),
                    )
                    .clicked()
                {
                    self.notice = if self.follow_target.trim().is_empty() {
                        "Enter a did:mini target first.".to_string()
                    } else if let Some(workspace) = self.workspace.as_mut() {
                        match workspace.set_follow_target(&self.follow_target, true) {
                            Ok(()) => "Follow object written locally.".to_string(),
                            Err(error) => format!("Could not follow target: {error}"),
                        }
                    } else {
                        "Local workspace unavailable.".to_string()
                    };
                }
                if ui
                    .add_enabled(can_act, egui::Button::new("Unfollow"))
                    .clicked()
                {
                    self.notice = if self.follow_target.trim().is_empty() {
                        "Enter a did:mini target first.".to_string()
                    } else if let Some(workspace) = self.workspace.as_mut() {
                        match workspace.set_follow_target(&self.follow_target, false) {
                            Ok(()) => "Unfollow object written locally.".to_string(),
                            Err(error) => format!("Could not unfollow target: {error}"),
                        }
                    } else {
                        "Local workspace unavailable.".to_string()
                    };
                }
            });
            if let Some(workspace) = self.workspace.as_ref() {
                ui.add_space(SPACE_XS);
                ui.label(
                    egui::RichText::new(format!(
                        "Following {} · {} follower(s) · {} mutual friend(s) known locally",
                        workspace.following_count(),
                        workspace.follower_count(),
                        workspace.mutual_follow_count()
                    ))
                    .small()
                    .color(COLOR_TEXT_SECONDARY),
                );
            }
        });
        ui.add_space(SPACE_LG);
        card(ui, |ui| {
            ui.label(
                egui::RichText::new("Your creator page")
                    .strong()
                    .color(COLOR_TEXT_PRIMARY),
            );
            ui.label(
                egui::RichText::new("Profile + pinned collections + progressive media")
                    .small()
                    .color(COLOR_TEXT_SECONDARY),
            );
            ui.separator();
            ui.label(
                egui::RichText::new("Publish local media")
                    .strong()
                    .color(COLOR_TEXT_PRIMARY),
            );
            ui.label("Local file path");
            ui.add(
                egui::TextEdit::singleline(&mut self.media_path)
                    .hint_text("C:\\path\\to\\file.png"),
            );
            ui.label("Content type");
            ui.add(egui::TextEdit::singleline(&mut self.media_content_type).hint_text("image/png"));
            ui.label("Caption (optional)");
            ui.add(
                egui::TextEdit::singleline(&mut self.media_caption)
                    .hint_text("Say something about it"),
            );
            ui.add_space(SPACE_XS);
            if ui
                .add_enabled(
                    is_unlocked,
                    egui::Button::new("Publish media").fill(COLOR_ACCENT.gamma_multiply(0.35)),
                )
                .clicked()
            {
                self.notice = if self.media_path.trim().is_empty() {
                    "Choose a local file path first.".to_string()
                } else if let Some(workspace) = self.workspace.as_mut() {
                    match workspace.publish_media_post(
                        self.media_path.trim(),
                        self.media_content_type.trim(),
                        self.media_caption.trim(),
                    ) {
                        Ok(()) => {
                            "Media chunks and linked post written locally. No upload occurred."
                                .to_string()
                        }
                        Err(error) => format!("Could not publish media: {error}"),
                    }
                } else {
                    "Local workspace unavailable.".to_string()
                };
            }
            ui.label(egui::RichText::new("The path is read locally; no file picker, browser, uploader, or remote preview is used.").small().color(COLOR_TEXT_SECONDARY));
            ui.separator();
            ui.label(egui::RichText::new("Collections and analytics are derived from the same local objects; no third-party dashboard is required.").small().color(COLOR_TEXT_SECONDARY));
        });
        ui.add_space(SPACE_LG);
        ui.label(egui::RichText::new("Media playback is designed to work from local chunks first. External catalog adapters are opt-in and never become update or identity authorities.").small().color(COLOR_TEXT_SECONDARY));
    }

    fn connections(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Transport order is user-controlled").strong());
        ui.add_space(6.0);
        self.transport_row(ui, "Offline store", "Always available", true);
        self.transport_row(
            ui,
            "Local Wi-Fi / hotspot",
            "Direct nearby transfer",
            self.privacy.lan_discovery,
        );
        self.transport_row(
            ui,
            "Self-hosted relay",
            "Optional encrypted transport",
            self.privacy.relays,
        );
        ui.add_space(12.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("Add a friend or contact").strong());
            ui.label("Open Discover to search signed profiles by name or DID, discover an opt-in nearby profile, and add a friend with one button.");
            if let Some(workspace) = self.workspace.as_ref() {
                if let Some(human) = workspace.human.as_ref() {
                    ui.label(format!("Your DID: {human}"));
                }
            }
            if ui.button("Open friend manager").clicked() {
                self.view = View::Discover;
            }
            ui.label("A friend is shown as mutual only after both signed follow objects arrive through sync.");
        });
        ui.add_space(12.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("One-shot direct peer sync").strong());
            ui.label("Encrypted TCP bearer + verified MINI/SYNC1 ingest. Nothing runs until you press a button.");
            ui.label("Use this with a peer you trust; the address is not authenticated by discovery.");
            ui.horizontal(|ui| {
                ui.label("Peer address");
                ui.text_edit_singleline(&mut self.peer_address);
            });
            ui.horizontal(|ui| {
                ui.label("Listen port");
                ui.text_edit_singleline(&mut self.listen_port);
            });
            ui.horizontal(|ui| {
                if ui.button("Connect once").clicked() {
                    self.start_peer_sync(false, None);
                }
                if ui.button("Listen once").clicked() {
                    self.start_peer_sync(true, None);
                }
            });
            if self.sync_rx.is_some() || self.visibility_rx.is_some() {
                ui.label("Peer operation active… close the peer or wait for the bounded protocol to finish.");
            }
            ui.label("Direct TCP works on LAN or over the internet when the endpoint is reachable. Port forwarding, firewall rules, NAT traversal, and relay deployment remain the operator's responsibility.");
        });
        ui.add_space(12.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("Offline transfer").strong());
            ui.label("Move signed objects by USB, a trusted shared folder, or a peer handoff. Bundles do not contain the DPAPI identity vault.");
            ui.text_edit_singleline(&mut self.export_path);
            if ui.button("Export local objects").clicked() {
                self.notice = if let Some(workspace) = self.workspace.as_ref() {
                    match workspace.export_bundle(self.export_path.trim()) {
                        Ok(bytes) => format!("Exported {bytes} bytes of signed objects. The bundle is portable, not encrypted."),
                        Err(error) => format!("Export failed: {error}"),
                    }
                } else {
                    "Local workspace unavailable.".to_string()
                };
            }
            ui.separator();
            ui.text_edit_singleline(&mut self.import_path);
            if ui.button("Import local objects").clicked() {
                self.notice = if let Some(workspace) = self.workspace.as_mut() {
                    match workspace.import_bundle(self.import_path.trim()) {
                        Ok(count) => format!("Imported {count} verified object(s). No network used."),
                        Err(error) => format!("Import failed: {error}"),
                    }
                } else {
                    "Local workspace unavailable.".to_string()
                };
            }
            ui.label("For sensitive material, protect the destination with BitLocker or another user-selected encrypted container.");
        });
        ui.add_space(12.0);
        ui.label("A blocked domain or unavailable relay does not delete local data. Export, peer transfer, and alternate relays remain separate paths.");
    }

    fn system(&mut self, ui: &mut egui::Ui) {
        let Some(workspace) = self.workspace.as_ref() else {
            ui.colored_label(egui::Color32::YELLOW, "Local workspace unavailable.");
            return;
        };
        let count = |object_type: &ObjectType| {
            workspace
                .store
                .by_type(object_type)
                .map_or(0, |ids| ids.len())
        };
        let total = workspace.store.all_ids().map_or(0, |ids| ids.len());
        ui.group(|ui| {
            ui.label(egui::RichText::new("Local state").strong());
            ui.label(format!(
                "{total} signed/content-addressed object(s) stored locally."
            ));
            ui.label(if workspace.root_created() {
                "Root: created in the Windows user vault"
            } else {
                "Root: not created yet"
            });
            ui.label(if workspace.is_unlocked() {
                "Signing: unlocked for this session"
            } else {
                "Signing: locked; reading remains available"
            });
        });
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            for (label, value) in [
                ("Posts", count(&ObjectType::POST)),
                ("Profiles", count(&ObjectType::PROFILE)),
                ("Comments", count(&ObjectType::COMMENT)),
                ("Reactions", count(&ObjectType::REACTION)),
                ("Communities", count(&ObjectType::COMMUNITY)),
                ("Public walls", count(&ObjectType::WALL)),
                ("Media manifests", count(&ObjectType::MEDIA_MANIFEST)),
                ("Forge commits", count(&ObjectType::COMMIT)),
                ("Releases", count(&ObjectType::RELEASE)),
            ] {
                ui.group(|ui| {
                    ui.heading(value.to_string());
                    ui.label(label);
                });
            }
        });
        ui.add_space(10.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("Available client surfaces").strong());
            ui.horizontal_wrapped(|ui| {
                if ui.button("Open feed").clicked() {
                    self.view = View::Home;
                }
                if ui.button("Open communities").clicked() {
                    self.view = View::Communities;
                }
                if ui.button("Open creator studio").clicked() {
                    self.view = View::Creator;
                }
                if ui.button("Open connections").clicked() {
                    self.view = View::Connections;
                }
                if ui.button("Open privacy center").clicked() {
                    self.view = View::Privacy;
                }
                if ui.button("Run diagnostics").clicked() {
                    self.view = View::Diagnostics;
                }
                if ui.button("Version & install").clicked() {
                    self.view = View::Updates;
                }
            });
        });
        ui.add_space(10.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("Protocol coverage").strong());
            ui.label("Integrated here: signed social objects, local feed assembly, communities, threaded replies, reactions, chunked media, DPAPI identity/conversation storage, Inbox beta, offline bundles, encrypted one-shot TCP sync, Windows install inspection with re-verification and rollback, and a diagnostics suite that executes the real identity, storage, social, media, messaging, sync, governance, erasure-coding, and storage-proof code.");
            ui.label("Available in the repository but not yet a finished desktop workflow: production chat sessions/mailboxes, forge repository/PR administration, presence/keystone encounters, reward accounting, privacy-cost routing, and release adoption decisions. Diagnostics *exercises* the governed-review path end to end, which is not the same as offering a desktop workflow for running it.");
            ui.label("Those foundations are deliberately shown as boundaries rather than unsafe pretend buttons. Public object types remain inspectable and syncable when another Mininet tool creates them; private messages currently require an explicit one-shot conversation sync.");
        });
        ui.add_space(10.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("Production readiness").strong());
            for (feature, status, owner) in [
                ("Local social, profiles, follows, walls, communities", "Integrated / test-covered", "Desktop"),
                ("Offline bundles and manual encrypted TCP sync", "Integrated / operator-configured", "Desktop + networking"),
                ("Windows packaging, install, verify, rollback, uninstall", "Integrated / test-covered; not code-signed; MSI ships for per-user managed deployment, no per-machine install", "Setup"),
                ("Runnable diagnostics over the real protocol code", "Integrated / test-covered, including refusal checks", "Diagnostics"),
                ("Internet relay and NAT traversal", "Partial: self-hosted relay foundation exists", "Networking"),
                ("Private messaging", "Manual Inbox beta integrated; prekeys, ratchet, mailbox, provenance UI and multi-device delivery missing", "Messaging + desktop"),
                ("Voice and video calls", "Not implemented end to end", "Realtime media"),
                ("Forge repositories, pull requests, releases", "Protocol foundation, exercised by diagnostics; no desktop administration workflow", "Forge UI"),
                ("Presence / keystone / reward encounter", "Protocol demo; production hardware path missing", "Identity + device"),
                ("Notifications, moderation labels, block/mute", "Not integrated in desktop", "Social UI"),
                ("Search, public web intake, external catalog adapters", "Partial or not started", "Search / adapters"),
                ("Production security and cryptographic review", "Launch-blocking external gate", "Security program"),
            ] {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(feature).strong());
                    ui.label(status);
                    ui.label(egui::RichText::new(format!("Owner: {owner}")).small());
                });
            }
            ui.label("This matrix is intentionally conservative: a tested prototype is not treated as production-ready until its external gates and real deployment path exist.");
        });
    }

    fn transport_row(&self, ui: &mut egui::Ui, name: &str, detail: &str, enabled: bool) {
        ui.horizontal(|ui| {
            ui.label(if enabled { "●" } else { "○" });
            ui.label(egui::RichText::new(name).strong());
            ui.label(detail);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(if enabled { "enabled" } else { "off" });
            });
        });
        ui.separator();
    }

    /// Start a diagnostics run off the UI thread.
    ///
    /// The suite opens real sockets and writes real files, so it takes long
    /// enough that running it on the UI thread would freeze the window --- the
    /// one place in this client where a background thread is worth the extra
    /// state. Scratch state goes in a throwaway directory under the OS temp
    /// directory, never in the user's own `MININET_HOME`, so a diagnostics run
    /// can never touch identities or posts.
    fn start_selftest(&mut self, area: Option<&'static str>) {
        if self.selftest_rx.is_some() {
            self.notice = "Diagnostics are already running.".to_string();
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.selftest_rx = Some(receiver);
        self.selftest_area = area;
        self.selftest_report = None;
        self.notice = match area {
            Some(area) => format!("Running the {area} checks..."),
            None => "Running every check...".to_string(),
        };
        std::thread::spawn(move || {
            let scratch = mini_selftest::default_scratch();
            let report = match std::fs::create_dir_all(&scratch) {
                Ok(()) => match area {
                    Some(area) => mini_selftest::run_area(&scratch, area),
                    None => mini_selftest::run_all(&scratch),
                },
                // An unwritable or full temp directory used to produce an
                // empty report, which `is_clean()` reads as "nothing failed"
                // --- a green result over a run that never happened, which is
                // the exact failure this whole view exists to prevent.
                Err(error) => SelfTestReport {
                    checks: vec![mini_selftest::Check {
                        area: "diagnostics",
                        name: "the diagnostics could not start",
                        negative: false,
                        outcome: CheckOutcome::Failed {
                            detail: format!(
                                "could not create a scratch directory at {}: {error}. No check \
                                 ran, so nothing here has been verified.",
                                scratch.display()
                            ),
                        },
                    }],
                    elapsed_ms: 0,
                },
            };
            let _ = std::fs::remove_dir_all(&scratch);
            let _ = sender.send(report);
        });
    }

    /// Diagnostics: run the real protocol stack and show what happened.
    ///
    /// This exists because a protocol whose guarantees can only be confirmed
    /// by reading its test suite is a protocol its users cannot confirm at
    /// all. Every line here is the shipped library code executing, not a
    /// description of it.
    fn diagnostics(&mut self, ui: &mut egui::Ui) {
        ui.heading("Diagnostics");
        ui.label(
            "Runs the real identity, storage, social, media, messaging, sync, governance, \
             erasure-coding, storage-proof, and install code and reports what happened. \
             Nothing here touches your identities or posts: every check builds its own \
             throwaway state and deletes it afterwards.",
        );
        ui.add_space(8.0);
        // Includes the value-layer checks a real run appends after spawning
        // `mininet-value-selftest`: without them this summary undercounts
        // what "Run every check" actually does whenever that binary ships
        // alongside the client, which is every packaged build.
        let total =
            mini_selftest::all_checks().len() + mini_selftest::value::ADVERTISED_CHECKS.len();
        let refusals = mini_selftest::all_checks()
            .iter()
            .filter(|(_, _, negative, _)| *negative)
            .count()
            + mini_selftest::value::ADVERTISED_CHECKS
                .iter()
                .filter(|(_, _, negative)| *negative)
                .count();
        ui.label(format!(
            "{total} checks across {} areas. {refusals} of them check that something is \
             *refused* rather than that it works, which is what shows the guarantees are \
             load-bearing.",
            mini_selftest::AREAS.len()
        ));
        ui.add_space(10.0);
        let running = self.selftest_rx.is_some();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!running, egui::Button::new("Run every check"))
                .clicked()
            {
                self.start_selftest(None);
            }
            for area in mini_selftest::AREAS {
                if ui.add_enabled(!running, egui::Button::new(*area)).clicked() {
                    self.start_selftest(Some(area));
                }
            }
        });
        if running {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Running. The sync check opens a real loopback socket.");
            });
        }
        ui.add_space(12.0);
        // What a green result does *not* cover, shown next to the button that
        // produces it. A suite that passes says nothing about code it never
        // touched, and leaving that out is how a partial check gets read as a
        // whole-system guarantee.
        egui::CollapsingHeader::new("What these checks do and do not cover")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(mini_selftest::coverage::summary());
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .id_salt("coverage")
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for (name, coverage) in mini_selftest::COVERAGE {
                            let (mark, colour, detail) = match coverage {
                                mini_selftest::Coverage::Exercised { area } => (
                                    "run",
                                    egui::Color32::from_rgb(90, 170, 110),
                                    (*area).to_string(),
                                ),
                                mini_selftest::Coverage::SeparateBinary { binary, .. } => (
                                    "run",
                                    egui::Color32::from_rgb(90, 170, 110),
                                    format!("via {binary}"),
                                ),
                                mini_selftest::Coverage::Transitive { via } => {
                                    ("dep", egui::Color32::GRAY, (*via).to_string())
                                }
                                mini_selftest::Coverage::Gap { reason } => (
                                    "not run",
                                    egui::Color32::from_rgb(200, 150, 70),
                                    (*reason).to_string(),
                                ),
                            };
                            ui.horizontal_wrapped(|ui| {
                                ui.colored_label(colour, mark);
                                ui.label(egui::RichText::new(*name).strong());
                                ui.label(egui::RichText::new(detail).small());
                            });
                        }
                    });
            });
        ui.add_space(10.0);
        let Some(report) = self.selftest_report.clone() else {
            ui.label(
                egui::RichText::new("No run yet. Nothing is claimed until you press a button.")
                    .italics(),
            );
            return;
        };
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(report.summary()).strong());
                if report.is_clean() {
                    ui.colored_label(egui::Color32::from_rgb(90, 170, 110), "nothing failed");
                } else {
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 120, 60),
                        "at least one check failed; this build should not be trusted",
                    );
                }
            });
            if let Some(area) = self.selftest_area {
                ui.label(egui::RichText::new(format!("Only the {area} area ran.")).small());
            }
        });
        ui.add_space(10.0);
        let mut current_area = "";
        for check in &report.checks {
            if check.area != current_area {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(check.area).strong());
                current_area = check.area;
            }
            let (mark, colour) = match &check.outcome {
                CheckOutcome::Passed { .. } => ("pass", egui::Color32::from_rgb(90, 170, 110)),
                CheckOutcome::Failed { .. } => ("FAIL", egui::Color32::from_rgb(220, 120, 60)),
                CheckOutcome::Skipped { .. } => ("skip", egui::Color32::GRAY),
            };
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(colour, mark);
                ui.label(check.name);
                if check.negative {
                    ui.label(egui::RichText::new("refusal").small());
                }
            });
            ui.label(egui::RichText::new(format!("      {}", check.outcome.detail())).small());
        }
    }

    /// Version & install: what is installed, whether it still matches its
    /// manifest, and how to go back.
    ///
    /// Reads the same `mini-windows-setup` state `mininet-setup.exe` writes,
    /// so the client and the installer never disagree about what is
    /// installed. Deliberately read-mostly: this view can re-verify and it can
    /// roll back to a version already on disk, but it cannot install,
    /// download, or update anything. Nothing in this client fetches a release
    /// or applies one on a timer (`docs/INVARIANTS.md` U1); installing is
    /// always something a person started, in the setup program.
    fn updates(&mut self, ui: &mut egui::Ui) {
        ui.heading("Version & install");
        // Derived from this executable's own location, not the default root:
        // a client installed somewhere custom would otherwise inspect
        // %LOCALAPPDATA%\\Programs\\Mininet, find nothing, and report itself
        // unmanaged with verification and rollback disabled on an
        // installation that has both.
        let setup = match std::env::current_exe() {
            Ok(exe) => Setup::containing(&exe),
            Err(_) => Setup::for_current_user(),
        }
        .with_user_data_root(data_root());
        let status = setup.status();
        ui.add_space(6.0);
        match &status {
            Ok(status) => self.install_summary(ui, status),
            Err(error) => {
                ui.colored_label(
                    egui::Color32::from_rgb(220, 120, 60),
                    format!("Could not read the installation: {error}"),
                );
            }
        }
        ui.add_space(12.0);
        if let Ok(status) = &status {
            let installed = status.active.clone();
            let can_roll_back = status.previous.is_some();
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        installed.is_some(),
                        egui::Button::new("Re-check installed files"),
                    )
                    .clicked()
                {
                    self.install_notice = match installed
                        .as_ref()
                        .map(|record| setup.verify_installed(&record.version_text))
                    {
                        Some(Ok(report)) if report.is_intact() => format!(
                            "Installed {} matches its manifest: {} file(s), {} bytes re-hashed.",
                            report.version_text, report.files_checked, report.bytes_checked
                        ),
                        Some(Ok(report)) => {
                            let problems: Vec<String> = report
                                .problems
                                .iter()
                                .map(mini_windows_setup::report::describe_problem)
                                .collect();
                            format!(
                                "Installed {} does NOT match its manifest: {}. Reinstall from a \
                                 package you trust before running it again.",
                                report.version_text,
                                problems.join(", ")
                            )
                        }
                        Some(Err(error)) => format!("Could not check the files: {error}"),
                        None => "Nothing is installed to check.".to_string(),
                    };
                }
                if ui
                    .add_enabled(can_roll_back, egui::Button::new("Roll back one version"))
                    .clicked()
                {
                    // Shell integration is Windows-only; elsewhere the file
                    // half still happens and the recorded actions are reported
                    // rather than silently claimed.
                    //
                    // The options here must match what is actually on this
                    // machine, not a fresh install's defaults: a
                    // `Desktop shortcut` installed just an update ago would
                    // otherwise be left pointing at the newer version while
                    // rollback silently "fixes" only the Start Menu entry,
                    // and an install made with `--no-start-menu` would gain
                    // an unwanted Start Menu entry. The currently active
                    // record remembers the real choice, since it is exactly
                    // what the last install or upgrade actually applied.
                    let options = match &installed {
                        Some(record) => InstallOptions {
                            start_menu_shortcut: record.start_menu_shortcut,
                            desktop_shortcut: record.desktop_shortcut,
                            register_uninstall: record.register_uninstall,
                            ..InstallOptions::default()
                        },
                        None => InstallOptions::default(),
                    };
                    let mut windows_shell = WindowsShell::default();
                    let mut recording_shell = RecordingShell::default();
                    let shell: &mut dyn mini_windows_setup::ShellIntegration = if cfg!(windows) {
                        &mut windows_shell
                    } else {
                        &mut recording_shell
                    };
                    self.install_notice = match setup.rollback(&options, shell, now_ms()) {
                        Ok(record) => format!(
                            "Rolled back to {}. Close and reopen the client to run it.",
                            record.version_text
                        ),
                        Err(error) => format!("Rollback refused: {error}"),
                    };
                }
                if ui.button("Show where things live").clicked() {
                    self.install_notice = format!(
                        "Program files: {}\nYour data: {}",
                        status.install_root.display(),
                        status.user_data_root.display()
                    );
                }
            });
        }
        if !self.install_notice.is_empty() {
            ui.add_space(10.0);
            ui.group(|ui| {
                for line in self.install_notice.lines() {
                    ui.label(line);
                }
            });
        }
        ui.add_space(12.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("What this client will not do").strong());
            ui.label(
                "It does not check for updates, download a release, or install one. There is no \
                 background task and no timer. Updating means running the setup program \
                 yourself, with a package you obtained however you chose.",
            );
            ui.label(
                "It cannot be forced or remotely disabled. A newer release cannot replace this \
                 one without someone on this device approving that exact package by its digest.",
            );
            ui.label(
                "Rolling back only ever moves to a version already on this disk, and only after \
                 re-hashing every one of its files first.",
            );
        });
        ui.add_space(10.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("Verify this yourself").strong());
            ui.label(
                "The manifest beside each package records every file's length, BLAKE3, and \
                 SHA-256. Get-FileHash checks the SHA-256 column without running anything we \
                 shipped.",
            );
            ui.label(
                "A matching digest proves the file is the one the manifest describes. Who wrote \
                 the manifest is a separate question, answered by the release attestations in \
                 mini-forge, not by this view.",
            );
            ui.label(
                egui::RichText::new(
                    "These builds are not code-signed, so Windows SmartScreen warns on first \
                     run. That warning is accurate.",
                )
                .italics(),
            );
        });
    }

    fn install_summary(&self, ui: &mut egui::Ui, status: &SetupStatus) {
        ui.group(|ui| {
            match &status.active {
                Some(record) => {
                    ui.label(
                        egui::RichText::new(format!("Installed version {}", record.version_text))
                            .strong(),
                    );
                    ui.monospace(format!("package digest {}", record.package_digest));
                }
                None => {
                    ui.label(egui::RichText::new("No managed installation here").strong());
                    ui.label(
                        "This copy is running from a build directory or an unmanaged folder. \
                         That works, but there is no manifest to check it against and no \
                         rollback target.",
                    );
                }
            }
            match &status.previous {
                Some(previous) => ui.label(format!("Can roll back to {}", previous.version_text)),
                None => ui.label("No earlier version to roll back to"),
            };
            if !status.installed_versions.is_empty() {
                ui.label(format!(
                    "Versions on disk: {}",
                    status.installed_versions.join(", ")
                ));
            }
            ui.label(format!("Program files: {}", status.install_root.display()));
            ui.label(format!(
                "Your identities, posts, and settings: {} ({})",
                status.user_data_root.display(),
                if status.user_data_present {
                    "present"
                } else {
                    "not created yet"
                }
            ));
            ui.label(
                egui::RichText::new(
                    "Those two directories are separate on purpose: removing the program cannot \
                     delete an identity you cannot recreate.",
                )
                .small(),
            );
        });
    }

    fn privacy(&mut self, ui: &mut egui::Ui) {
        ui.colored_label(egui::Color32::from_rgb(100, 210, 160), "HARDENED DEFAULTS");
        ui.add_space(8.0);
        let mut settings_changed = ui
            .checkbox(
                &mut self.privacy.external_sources,
                "Allow external source adapters",
            )
            .changed();
        settings_changed |= ui
            .checkbox(
                &mut self.privacy.lan_discovery,
                "Allow local-network discovery",
            )
            .changed();
        settings_changed |= ui
            .checkbox(
                &mut self.privacy.relays,
                "Allow user-selected encrypted relays",
            )
            .changed();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Update adoption:");
            settings_changed |= ui
                .selectable_value(
                    &mut self.privacy.update_policy,
                    UpdatePolicy::ManualOnly,
                    "Manual only",
                )
                .changed();
            settings_changed |= ui
                .selectable_value(
                    &mut self.privacy.update_policy,
                    UpdatePolicy::Ask,
                    "Ask before adoption",
                )
                .changed();
        });
        if settings_changed {
            self.notice = match save_privacy_settings(self.privacy) {
                Ok(()) => {
                    "Privacy settings saved through the Windows protection boundary.".to_string()
                }
                Err(error) => format!("Privacy settings were not saved: {error}"),
            };
        }
        ui.separator();
        ui.label(egui::RichText::new("Telemetry: permanently disabled in this shell").strong());
        ui.label("There is no analytics client, ad SDK, embedded browser, remote configuration, silent update executor, or background network loop in this UI crate.");
        if let Some(workspace) = self.workspace.as_ref() {
            if let Some(human) = workspace.human.as_ref() {
                ui.label(format!("Current session identity: {}", human.as_str()));
            } else {
                ui.label("No Mininet root exists yet. Complete onboarding to create one.");
            }
            ui.label("The identity seed envelope is protected by Windows DPAPI for the current user. This does not defend against malware or an administrator running as that user.");
        }
        ui.add_space(10.0);
        ui.colored_label(egui::Color32::YELLOW, "Windows boundary");
        ui.label("This reduces Mininet's own tracking and censorship dependencies. It cannot stop a compromised Windows kernel, a malicious administrator, malware, accessibility abuse, screen capture, or a hardware/driver keylogger. Sensitive entry should use a trusted OS/device and Mininet should keep secrets out of logs and URLs.");
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Mininet")
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Mininet",
        options,
        Box::new(|_cc| Ok(Box::new(MininetApp::default()))),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        decode_privacy_settings, encode_privacy_settings, nearby_endpoint_for,
        next_object_sequence, parse_profile_fields, PrivacyState, UpdatePolicy,
    };
    use did_mini::Controller;
    use mini_objects::{ObjectBuilder, ObjectType, Payload};
    use mini_social::NearbyProfile;
    use mini_store::{MemoryBackend, Store};
    use std::net::SocketAddr;

    #[test]
    fn privacy_defaults_are_local_and_manual() {
        let state = PrivacyState::default();
        assert!(!state.telemetry);
        assert!(!state.external_sources);
        assert!(!state.relays);
        assert!(!state.lan_discovery);
        assert_eq!(state.update_policy, UpdatePolicy::ManualOnly);
    }

    #[test]
    fn privacy_settings_round_trip_without_telemetry_state() {
        let state = PrivacyState {
            telemetry: true,
            external_sources: true,
            relays: false,
            lan_discovery: true,
            update_policy: UpdatePolicy::Ask,
        };
        let restored = decode_privacy_settings(&encode_privacy_settings(state)).unwrap();
        assert!(!restored.telemetry);
        assert!(restored.external_sources);
        assert!(!restored.relays);
        assert!(restored.lan_discovery);
        assert_eq!(restored.update_policy, UpdatePolicy::Ask);
    }

    #[test]
    fn custom_profile_fields_are_parsed_without_fixed_platform_schema() {
        let fields = parse_profile_fields(
            "Pronouns: they/them\nWebsite: https://example.org/profile?q=one:two",
        )
        .unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].label, "Pronouns");
        assert_eq!(fields[0].value, "they/them");
        assert_eq!(fields[1].label, "Website");
        assert_eq!(fields[1].value, "https://example.org/profile?q=one:two");
    }

    #[test]
    fn duplicate_or_malformed_custom_profile_fields_are_rejected() {
        assert!(parse_profile_fields("Website: one\nwebsite: two").is_err());
        assert!(parse_profile_fields("missing separator").is_err());
    }

    #[test]
    fn signing_sequence_continues_after_existing_objects() {
        let identity = Controller::incept_single_from_seeds(&[7; 32], &[8; 32]).unwrap();
        let other = Controller::incept_single_from_seeds(&[9; 32], &[10; 32]).unwrap();
        let object = ObjectBuilder::new(ObjectType::POST)
            .sequence(41)
            .payload(Payload::Public(b"existing".to_vec()))
            .sign(&identity.did(), &identity)
            .unwrap();
        let mut store = Store::new(MemoryBackend::new());
        store.insert(&object).unwrap();
        let foreign = ObjectBuilder::new(ObjectType::POST)
            .sequence(u64::MAX)
            .payload(Payload::Public(b"foreign".to_vec()))
            .sign(&other.did(), &other)
            .unwrap();
        store.insert(&foreign).unwrap();
        assert_eq!(
            next_object_sequence(&store, Some(&identity.did())).unwrap(),
            42
        );
    }

    #[test]
    fn automatic_delivery_uses_only_the_exact_verified_did() {
        let alice = Controller::incept_single_from_seeds(&[11; 32], &[12; 32]).unwrap();
        let bob = Controller::incept_single_from_seeds(&[13; 32], &[14; 32]).unwrap();
        let misleading_name = NearbyProfile {
            address: "127.0.0.1:46001".parse::<SocketAddr>().unwrap(),
            did: alice.did(),
            display_name: "Bob".to_string(),
        };
        let exact_did = NearbyProfile {
            address: "127.0.0.1:46002".parse::<SocketAddr>().unwrap(),
            did: bob.did(),
            display_name: "Anything".to_string(),
        };

        assert_eq!(
            nearby_endpoint_for(&[misleading_name, exact_did], &bob.did()),
            Some("127.0.0.1:46002".parse().unwrap())
        );
    }

    #[cfg(windows)]
    #[test]
    fn legacy_root_signed_profile_upgrades_without_changing_human_did() {
        use super::{load_or_create, now_ms, publish_profile, Workspace};
        use mini_store::FsBackend;

        let test_root = std::env::temp_dir().join(format!(
            "mininet-desktop-profile-upgrade-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let root_seeds = load_or_create(&test_root.join("identity.dpapi")).unwrap();
        let root_identity =
            Controller::incept_single_from_seeds(&root_seeds.current, &root_seeds.next).unwrap();
        let human = root_identity.did();
        let mut store = Store::new(FsBackend::open(&test_root).unwrap());
        publish_profile(
            &mut store,
            &human,
            &root_identity,
            "Legacy Alice",
            "kept unchanged",
            None,
            1,
            0,
        )
        .unwrap();
        let mut workspace = Workspace {
            store,
            identity: None,
            human: Some(human.clone()),
            root: test_root.clone(),
            sequence: 1,
            conversations: Vec::new(),
        };

        assert!(workspace.profile_needs_device_upgrade());
        workspace.upgrade_profile_for_sync().unwrap();
        assert!(!workspace.profile_needs_device_upgrade());
        assert_eq!(workspace.human.as_ref(), Some(&human));
        assert!(!workspace.is_unlocked());
        let profile = workspace.current_profile().unwrap();
        assert_eq!(profile.display_name, "Legacy Alice");
        assert_eq!(profile.bio, "kept unchanged");

        std::fs::remove_dir_all(test_root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn one_visibility_window_verifies_profile_then_receives_follow() {
        use super::{
            followers, known_profiles, load_desktop_identity, load_or_create, publish_profile,
            run_discoverable_profile_sync, run_peer_sync, set_follow,
        };
        use mini_store::FsBackend;
        use std::sync::mpsc;
        use std::time::Duration;

        fn profile_root(root: &std::path::Path, name: &str) -> did_mini::Did {
            load_or_create(&root.join("identity.dpapi")).unwrap();
            let identity = load_desktop_identity(root, true).unwrap();
            let human = identity.root.did();
            let mut store = Store::new(FsBackend::open(root).unwrap());
            publish_profile(
                &mut store,
                &human,
                &identity.device,
                name,
                "two-peer visibility test",
                None,
                1,
                0,
            )
            .unwrap();
            human
        }

        let test_root = std::env::temp_dir().join(format!(
            "mininet-desktop-visible-{}-{}",
            std::process::id(),
            super::now_ms()
        ));
        let bob_root = test_root.join("bob");
        let alice_root = test_root.join("alice");
        let bob_did = profile_root(&bob_root, "Bob");
        let alice_did = profile_root(&alice_root, "Alice");

        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let (sender, receiver) = mpsc::channel();
        let server_root = bob_root.clone();
        let server = std::thread::spawn(move || {
            run_discoverable_profile_sync(
                &server_root,
                port,
                "Bob",
                Duration::from_secs(4),
                &sender,
            )
        });
        std::thread::sleep(Duration::from_millis(150));

        let endpoint = format!("127.0.0.1:{port}");
        run_peer_sync(&alice_root, &endpoint, false).unwrap();
        let alice_identity = load_desktop_identity(&alice_root, false).unwrap();
        let mut alice_store = Store::new(FsBackend::open(&alice_root).unwrap());
        set_follow(
            &mut alice_store,
            &alice_did,
            &alice_identity.device,
            &bob_did,
            true,
            2,
            1,
        )
        .unwrap();
        run_peer_sync(&alice_root, &endpoint, false).unwrap();
        let summary = server.join().unwrap().unwrap();
        assert!(summary.contains("2 completed sync connection(s)"));
        let events: Vec<Result<String, String>> = receiver.try_iter().collect();
        assert_eq!(events.iter().filter(|event| event.is_ok()).count(), 2);

        let bob_store = Store::new(FsBackend::open(&bob_root).unwrap());
        let names: Vec<String> = known_profiles(&bob_store)
            .unwrap()
            .into_iter()
            .map(|profile| profile.display_name)
            .collect();
        assert!(names.iter().any(|name| name == "Alice"));
        assert!(names.iter().any(|name| name == "Bob"));
        assert_eq!(followers(&bob_store, &bob_did).unwrap(), vec![alice_did]);

        std::fs::remove_dir_all(test_root).unwrap();
    }
}
