//! Windows-first Mininet reference client shell.
//!
//! No analytics, remote configuration, embedded browser, or update executor.
//! Networking runs only inside an owner-started connection session or an
//! owner-started hosting window; both can be re-enabled on launch only by a
//! persisted, default-off choice the owner makes in Connections.

#![forbid(unsafe_code)]

mod connectivity;
mod conversation_state;
mod mute_list;
mod network_session;
mod peer_link;
mod theme;
mod timeline;

use conversation_state::ConversationRecord;
use did_mini::{Capabilities, Controller, Did};
use eframe::egui;
use mini_media::{assemble, publish_media, read_manifest};
use mini_messaging::{scan as scan_messages, send as send_message, MessageDraft};
use mini_objects::{ObjectType, OpaqueRoute};
use mini_selftest::{Outcome as CheckOutcome, Report as SelfTestReport};
use mini_social::{
    community_members, followers, following, known_profiles, publish_comment, publish_community,
    publish_media_post, publish_post, publish_profile, publish_profile_details, publish_wall,
    resolve_community, resolve_profile, set_follow, set_membership, set_reaction, FeedFilter,
    LocalProfileAnnouncer, LocalProfileScanner, MembershipMode, NearbyProfile, PublicProfileDraft,
    PublicProfileField, ReactionKind, VisibilityPolicy, MAX_LOCATION_BYTES, MAX_PROFILE_FIELDS,
    MAX_PROFILE_FIELD_LABEL_BYTES, MAX_PROFILE_FIELD_VALUE_BYTES,
};
use mini_store::{Backend, FsBackend, Store};
use mini_sync::{kel_carrier, KelCache};
use mini_windows_setup::{InstallOptions, RecordingShell, Setup, SetupStatus, WindowsShell};
use mini_windows_vault::{load_existing, load_or_create, load_user_data, save_user_data, SeedPair};
use std::collections::HashMap;
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

const PEER_IO_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Onboarding,
    Home,
    Discover,
    Media,
    Inbox,
    People,
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

/// An owner-started hosting window: the accepting socket lives on a worker
/// thread and stops when this is dropped or the owner presses Stop.
struct HostState {
    stop: Arc<AtomicBool>,
    rx: Receiver<peer_link::HostEvent>,
    port: u16,
    started: Instant,
    listening: bool,
    served: usize,
    failed: usize,
    last: String,
}

impl Drop for HostState {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
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
    theme_applied: bool,
    /// The launch policy (session/host on launch) runs exactly once.
    launched: bool,
    connections: connectivity::ConnectionSettings,
    network_session: Option<network_session::NetworkSession>,
    /// One session exchange in flight: (peer index, outcome).
    session_rx: Option<Receiver<(usize, Result<String, String>)>>,
    host: Option<HostState>,
    /// Most recent network events, newest last, bounded.
    activity: Vec<String>,
    timeline_cards: Vec<timeline::Card>,
    timeline_rx: Option<Receiver<Result<Vec<timeline::Card>, String>>>,
    timeline_loaded: (FeedFilter, timeline::Scope),
    timeline_scope: timeline::Scope,
    timeline_refresh: Instant,
    timeline_error: Option<String>,
    timeline_query: String,
    /// Decoded post images keyed by manifest id; `None` records a manifest
    /// that could not be shown so it is not re-decoded every frame.
    media_textures: HashMap<String, Option<egui::TextureHandle>>,
    new_peer_label: String,
    new_peer_endpoint: String,
    card_input: String,
    /// Host part the owner wants on their connection card.
    card_host: String,
    /// Post ids the owner has not had on screen yet; cleared when Home is
    /// shown. Counted from timeline snapshots, so it needs no server.
    unseen_posts: Vec<String>,
    /// Device-local mute list; hides posts, suggestions and directory rows.
    muted: mute_list::MuteList,
    composer: String,
    community_name: String,
    community_charter: String,
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

    /// Re-read on-disk state after another thread (a sync worker or the
    /// host) inserted objects, without dropping an unlocked identity: the
    /// owner should not have to unlock again after every exchange.
    fn refresh(&mut self) -> Result<(), String> {
        let store = Store::new(FsBackend::open(&self.root).map_err(|error| error.to_string())?);
        let conversations = conversation_state::load(&self.root.join("conversations.dpapi"))?;
        let sequence = next_object_sequence(&store, self.human.as_ref())?;
        self.store = store;
        self.conversations = conversations;
        self.sequence = self.sequence.max(sequence);
        Ok(())
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
        let relock = !self.is_unlocked();
        if relock {
            self.unlock()?;
        }
        let result = self.react_like_unlocked(target);
        if relock {
            self.lock();
        }
        result
    }

    fn react_like_unlocked(&mut self, target: &mini_objects::ObjectId) -> Result<(), String> {
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

fn accept_once(endpoint: &str) -> Result<TcpStream, String> {
    let listener = std::net::TcpListener::bind(endpoint).map_err(|error| error.to_string())?;
    let (stream, _) = listener.accept().map_err(|error| error.to_string())?;
    Ok(stream)
}

fn run_peer_sync(root: &std::path::Path, endpoint: &str, listener: bool) -> Result<String, String> {
    if listener {
        let stream = accept_once(endpoint)?;
        peer_link::serve(root, stream, &[]).map(|summary| format!("Peer sync complete: {summary}."))
    } else {
        peer_link::dial_public(root, endpoint)
    }
}

fn run_private_sync(
    root: &std::path::Path,
    endpoint: &str,
    listener: bool,
    route: OpaqueRoute,
) -> Result<String, String> {
    if listener {
        let stream = accept_once(endpoint)?;
        peer_link::serve(root, stream, &[route])
            .map(|summary| format!("Private sync complete: {summary}."))
    } else {
        match peer_link::dial_private(root, endpoint, route)? {
            peer_link::PrivateOutcome::Synced { received, accepted } => Ok(format!(
                "Private sync complete: received {received}, accepted {accepted}."
            )),
            peer_link::PrivateOutcome::NotOnThisPeer => {
                Err("the peer does not hold this conversation".into())
            }
        }
    }
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
                let result = peer_link::serve(root, stream, &[]);
                match result {
                    Ok(summary) => {
                        completed = completed.saturating_add(1);
                        let _ = progress.send(Ok(format!(
                            "Nearby sync #{completed} complete with {peer}: {summary}. Still visible until the window ends."
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
        let connections = connectivity::load(&data_root());
        let listen_port = connections.listen_port.to_string();
        let (muted, mute_error) = mute_list::load(&data_root());
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
            theme_applied: false,
            launched: false,
            connections,
            network_session: None,
            session_rx: None,
            host: None,
            activity: Vec::new(),
            timeline_cards: Vec::new(),
            timeline_rx: None,
            timeline_loaded: (FeedFilter::Chronological, timeline::Scope::Following),
            timeline_scope: timeline::Scope::Following,
            timeline_refresh: Instant::now(),
            timeline_error: None,
            timeline_query: String::new(),
            media_textures: HashMap::new(),
            new_peer_label: String::new(),
            new_peer_endpoint: String::new(),
            card_input: String::new(),
            card_host: String::new(),
            unseen_posts: Vec::new(),
            muted,
            composer: String::new(),
            community_name: String::new(),
            community_charter: String::new(),
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
            peer_address: String::new(),
            listen_port,
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
            notice: match mute_error {
                Some(error) => {
                    format!("Mute list could not be read and is treated as empty: {error}")
                }
                None => {
                    "Local object store ready. Identity is locked; no network activity has started."
                        .to_string()
                }
            },
        }
    }
}

impl eframe::App for MininetApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.theme_applied {
            theme::apply(ctx);
            self.theme_applied = true;
        }
        if !self.launched {
            self.launched = true;
            self.apply_launch_policy();
        }
        self.poll_timeline(ctx);
        self.poll_host();
        self.poll_session();
        self.poll_dropped_files(ctx);
        self.poll_discovery();
        self.poll_one_shot_sync();
        self.poll_selftest();
        self.poll_visibility();
        self.schedule_session();
        if self.network_session.is_some() || self.host.is_some() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        if self.sync_rx.is_some()
            || self.session_rx.is_some()
            || self.discovery_rx.is_some()
            || self.visibility_rx.is_some()
            || self.selftest_rx.is_some()
            || self.timeline_rx.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.view == View::Onboarding {
            self.onboarding(ctx);
            return;
        }
        self.top_bar(ctx);
        self.navigation_rail(ctx);
        if ctx.screen_rect().width() >= 1100.0 {
            self.discovery_column(ctx);
        }
        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .stroke(egui::Stroke::new(1.0, theme::BORDER))
                    .inner_margin(egui::Margin::symmetric(16, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let summary = self.connection_summary();
                    let reserve = 360.0_f32.min(ui.available_width() * 0.45);
                    ui.add_sized(
                        [ui.available_width() - reserve, 18.0],
                        egui::Label::new(
                            egui::RichText::new(&self.notice)
                                .small()
                                .color(theme::TEXT_SECONDARY),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&self.notice);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        theme::muted(ui, &summary);
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::symmetric(24, 18)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.header(ui);
                        ui.add_space(12.0);
                        match self.view {
                            View::Onboarding => {
                                unreachable!("onboarding returns before the main shell")
                            }
                            View::Home => self.home(ui),
                            View::Discover => self.discover(ui),
                            View::Media => self.media_timeline(ui),
                            View::Inbox => self.inbox(ui),
                            View::People => self.people(ui),
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
    }
}

impl MininetApp {
    // ----- background workers and polling -------------------------------

    fn log_activity(&mut self, line: String) {
        const MAX_ACTIVITY: usize = 60;
        self.activity.push(line);
        if self.activity.len() > MAX_ACTIVITY {
            let excess = self.activity.len() - MAX_ACTIVITY;
            self.activity.drain(..excess);
        }
    }

    /// Re-read store-backed state after a worker inserted objects. Keeps
    /// the identity unlocked; falls back to a fresh open if refresh fails.
    fn reload_workspace(&mut self) {
        match self.workspace.as_mut() {
            Some(workspace) => {
                if workspace.refresh().is_err() {
                    self.workspace = Workspace::open().ok();
                }
            }
            None => self.workspace = Workspace::open().ok(),
        }
        self.timeline_refresh = Instant::now();
    }

    fn save_connections(&mut self) {
        if let Err(error) = connectivity::save(&data_root(), &self.connections) {
            self.notice = format!("Connection settings were not saved: {error}");
        }
    }

    /// The persisted, default-off launch choices. Nothing else starts a
    /// socket because the application was opened.
    fn apply_launch_policy(&mut self) {
        let ready = self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.root_created() && workspace.has_public_account());
        if !ready {
            return;
        }
        if self.connections.host_on_launch {
            self.start_host();
        }
        if self.connections.session_on_launch && !self.connections.peers.is_empty() {
            self.start_session();
        }
    }

    fn start_host(&mut self) {
        if self.host.is_some() {
            self.notice = "Already accepting connections.".into();
            return;
        }
        if self
            .workspace
            .as_ref()
            .is_some_and(Workspace::profile_needs_device_upgrade)
        {
            self.notice = "Upgrade this beta profile for verified peer sync first (People).".into();
            return;
        }
        let port = self.connections.listen_port;
        let routes: Vec<OpaqueRoute> = if self.connections.include_private {
            self.workspace
                .as_ref()
                .map(|workspace| {
                    workspace
                        .conversations
                        .iter()
                        .map(ConversationRecord::route)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        let root = data_root();
        let worker_stop = Arc::clone(&stop);
        std::thread::spawn(move || peer_link::run_host(root, port, routes, worker_stop, sender));
        self.host = Some(HostState {
            stop,
            rx: receiver,
            port,
            started: Instant::now(),
            listening: false,
            served: 0,
            failed: 0,
            last: String::new(),
        });
        self.log_activity(format!("Hosting requested on port {port}."));
    }

    fn stop_host(&mut self) {
        if self.host.take().is_some() {
            self.notice =
                "Stopped accepting connections. Exchanges already in progress may finish.".into();
            self.log_activity("Hosting stopped.".into());
        }
    }

    fn poll_host(&mut self) {
        let mut events = Vec::new();
        let mut ended = None;
        if let Some(host) = self.host.as_ref() {
            loop {
                match host.rx.try_recv() {
                    Ok(peer_link::HostEvent::Ended(result)) => {
                        ended = Some(result);
                        break;
                    }
                    Ok(event) => events.push(event),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        ended = Some(Err("the hosting worker stopped unexpectedly".into()));
                        break;
                    }
                }
            }
        }
        let mut changed = false;
        for event in events {
            match event {
                peer_link::HostEvent::Listening { port } => {
                    if let Some(host) = self.host.as_mut() {
                        host.listening = true;
                    }
                    self.log_activity(format!("Accepting connections on port {port}."));
                }
                peer_link::HostEvent::Connection { peer, result } => {
                    changed |= result.is_ok();
                    let line = match &result {
                        Ok(summary) => format!("{peer}: {summary}"),
                        Err(error) => format!("{peer}: {error}"),
                    };
                    if let Some(host) = self.host.as_mut() {
                        if result.is_ok() {
                            host.served = host.served.saturating_add(1);
                        } else {
                            host.failed = host.failed.saturating_add(1);
                        }
                        host.last = line.clone();
                    }
                    self.log_activity(line);
                }
                peer_link::HostEvent::Ended(_) => {}
            }
        }
        if let Some(result) = ended {
            self.host = None;
            match result {
                Ok(summary) => self.log_activity(summary),
                Err(error) => {
                    self.notice = format!("Hosting stopped: {error}");
                    self.log_activity(format!("Hosting stopped: {error}"));
                }
            }
        }
        if changed {
            self.reload_workspace();
        }
    }

    fn start_session(&mut self) {
        if self.network_session.is_some() {
            self.notice = "A connection session is already running.".into();
            return;
        }
        if self
            .workspace
            .as_ref()
            .is_some_and(Workspace::profile_needs_device_upgrade)
        {
            self.notice = "Upgrade this beta profile for verified peer sync first (People).".into();
            return;
        }
        match network_session::NetworkSession::start(
            &self.connections.endpoints(),
            Instant::now(),
            self.connections.session_length,
            self.connections.include_private,
        ) {
            Ok(session) => {
                let count = session.peers().len();
                self.network_session = Some(session);
                self.log_activity(format!(
                    "Session started with {count} saved peer(s), {}.",
                    self.connections.session_length.label().to_lowercase()
                ));
            }
            Err(error) => self.notice = error,
        }
    }

    fn stop_session(&mut self) {
        if self.network_session.take().is_some() {
            self.notice = "Session stopped. An exchange already in progress may finish.".into();
            self.log_activity("Session stopped.".into());
        }
    }

    fn schedule_session(&mut self) {
        let now = Instant::now();
        if self
            .network_session
            .as_ref()
            .is_some_and(|session| session.expired(now))
        {
            self.network_session = None;
            self.notice = "Connection session ended. Start another to keep syncing.".into();
            self.log_activity("Session ended (time limit reached).".into());
            return;
        }
        if self.session_rx.is_some() || self.sync_rx.is_some() {
            return;
        }
        let Some((index, endpoint, include_private)) =
            self.network_session.as_ref().and_then(|session| {
                session.next_due(now).and_then(|index| {
                    session
                        .endpoint(index)
                        .map(|endpoint| (index, endpoint.to_owned(), session.include_private()))
                })
            })
        else {
            return;
        };
        let routes: Vec<(String, OpaqueRoute)> = if include_private {
            self.workspace
                .as_ref()
                .map(|workspace| {
                    workspace
                        .conversations
                        .iter()
                        .map(|conversation| (conversation.label.clone(), conversation.route()))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let (sender, receiver) = mpsc::channel();
        self.session_rx = Some(receiver);
        let root = data_root();
        std::thread::spawn(move || {
            let result = exchange_with_peer(&root, &endpoint, &routes);
            let _ = sender.send((index, result));
        });
    }

    fn poll_session(&mut self) {
        let outcome = self
            .session_rx
            .as_ref()
            .and_then(|receiver| match receiver.try_recv() {
                Ok(outcome) => Some(outcome),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some((
                    usize::MAX,
                    Err("the exchange worker stopped unexpectedly".into()),
                )),
            });
        let Some((index, result)) = outcome else {
            return;
        };
        self.session_rx = None;
        let now = Instant::now();
        let endpoint = self
            .network_session
            .as_ref()
            .and_then(|session| session.endpoint(index))
            .unwrap_or("peer")
            .to_owned();
        let line = match &result {
            Ok(summary) => format!("{endpoint}: {summary}"),
            Err(error) => format!("{endpoint}: {error}"),
        };
        if let Some(session) = self.network_session.as_mut() {
            session.completed(index, result.is_ok(), line.clone(), now);
        }
        if result.is_ok() {
            self.reload_workspace();
        }
        self.log_activity(line);
    }

    fn poll_dropped_files(&mut self, ctx: &egui::Context) {
        if self.view != View::Creator {
            return;
        }
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

    fn poll_discovery(&mut self) {
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
    }

    fn poll_one_shot_sync(&mut self) {
        let sync_result = self
            .sync_rx
            .as_ref()
            .and_then(|receiver| match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                    "Sync worker stopped; delivery was not confirmed.".into(),
                )),
            });
        let Some(result) = sync_result else {
            return;
        };
        self.sync_rx = None;
        self.reload_workspace();
        let line = match &result {
            Ok(summary) => summary.clone(),
            Err(error) => format!("One-shot sync failed: {error}"),
        };
        self.log_activity(line);
        self.notice = match (self.sync_context.take(), result) {
            (Some(SyncContext::FriendRequest { display_name }), Ok(summary)) => format!(
                "Friend request delivered to {display_name}. They can add you back after syncing. {summary}"
            ),
            (Some(SyncContext::FriendRequest { display_name }), Err(error)) => format!(
                "Friend request for {display_name} is saved locally, but automatic delivery failed: {error}. It will go out on the next successful exchange."
            ),
            (None, Ok(summary)) => summary,
            (None, Err(error)) => format!("Peer sync failed: {error}"),
        };
    }

    fn poll_selftest(&mut self) {
        let Some(receiver) = self.selftest_rx.as_ref() else {
            return;
        };
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
                    "Diagnostics stopped unexpectedly. Nothing here has been verified.".to_string();
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

    fn poll_visibility(&mut self) {
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
            self.reload_workspace();
            let line = match result {
                Ok(summary) => summary,
                Err(error) => error,
            };
            self.log_activity(line.clone());
            self.notice = line;
        }
    }

    // ----- connection state summaries -----------------------------------

    /// Short, honest state for the top bar: what is actually running.
    fn connection_state(&self) -> (&'static str, egui::Color32) {
        let now = Instant::now();
        let hosting = self.host.as_ref().is_some_and(|host| host.listening);
        match (&self.network_session, hosting) {
            (Some(_), _) if self.session_rx.is_some() => ("Syncing", theme::ACCENT),
            (Some(session), _) if session.any_success() => ("Connected", theme::ONLINE_GREEN),
            (Some(session), _) if session.all_failed() => ("Peers unreachable", theme::WARN_AMBER),
            (Some(session), _) if session.elapsed(now) < Duration::from_secs(1) => {
                ("Connecting", theme::ACCENT)
            }
            (Some(_), _) => ("Connecting", theme::ACCENT),
            (None, true) => ("Hosting", theme::ONLINE_GREEN),
            (None, false) if self.host.is_some() => ("Starting host", theme::ACCENT),
            (None, false) => ("Offline", theme::TEXT_SECONDARY),
        }
    }

    fn connection_summary(&self) -> String {
        let now = Instant::now();
        let mut parts = Vec::new();
        if let Some(host) = self.host.as_ref() {
            parts.push(format!("Hosting on {} · {} served", host.port, host.served));
        }
        if let Some(session) = self.network_session.as_ref() {
            let remaining = match session.remaining(now) {
                Some(left) => format!("{}m left", left.as_secs() / 60),
                None => "open-ended".to_string(),
            };
            parts.push(format!(
                "Session · {} peer(s) · {remaining}",
                session.peers().len()
            ));
        }
        if parts.is_empty() {
            "No network activity · saved content available".to_string()
        } else {
            parts.join(" · ")
        }
    }

    // ----- shell chrome ---------------------------------------------------

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .stroke(egui::Stroke::new(1.0, theme::BORDER))
                    .inner_margin(egui::Margin::symmetric(20, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    theme::avatar(ui, "Mininet", "mininet", 30.0);
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Mininet")
                            .strong()
                            .size(19.0)
                            .color(theme::TEXT_PRIMARY),
                    );
                    ui.add_space(4.0);
                    theme::muted(ui, "Your people. Your world. Your internet.");
                    ui.add_space(10.0);
                    let (state, color) = self.connection_state();
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(format!("🌐  {state}"))
                                    .small()
                                    .strong()
                                    .color(color),
                            )
                            .fill(color.linear_multiply(0.14))
                            .stroke(egui::Stroke::new(1.0, color))
                            .corner_radius(egui::CornerRadius::same(255)),
                        )
                        .on_hover_text(self.connection_summary())
                        .clicked()
                    {
                        self.view = View::Connections;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(theme::secondary_button("Privacy")).clicked() {
                            self.view = View::Privacy;
                        }
                        ui.add_space(6.0);
                        if let Some(workspace) = self.workspace.as_mut() {
                            if workspace.is_unlocked() {
                                if ui.add(theme::secondary_button("🔒  Lock identity")).clicked() {
                                    workspace.lock();
                                    self.signing_confirmation = false;
                                    self.notice = "Identity locked. Reading remains available; signing is disabled.".to_string();
                                }
                            } else if ui.add(theme::primary_button("🔓  Unlock identity")).clicked() {
                                match workspace.unlock() {
                                    Ok(()) => self.notice = "Identity unlocked. Review and confirm before signing.".to_string(),
                                    Err(error) => self.notice = format!("Unlock failed: {error}"),
                                }
                            }
                        }
                    });
                });
            });
    }

    fn navigation_rail(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("navigation")
            .resizable(false)
            .default_width(236.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::symmetric(12, 16)),
            )
            .show(ctx, |ui| {
                let home_label = if self.unseen_posts.is_empty() {
                    "Home".to_string()
                } else {
                    format!("Home  ({} new)", self.unseen_posts.len())
                };
                self.nav_button(ui, View::Home, "🏠", &home_label);
                self.nav_button(ui, View::Discover, "🔍", "Explore");
                self.nav_button(ui, View::Media, "🎬", "Media");
                self.nav_button(ui, View::Inbox, "✉", "Messages");
                self.nav_button(ui, View::People, "👥", "People");
                self.nav_button(ui, View::Communities, "🏢", "Communities");
                self.nav_button(ui, View::Creator, "✏", "Creator studio");
                self.nav_button(ui, View::Connections, "🔗", "Connections");
                self.nav_button(ui, View::System, "🖥", "System & storage");
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);
                self.nav_button(ui, View::Privacy, "🔒", "Privacy & safety");
                self.nav_button(ui, View::Diagnostics, "ℹ", "Diagnostics");
                self.nav_button(ui, View::Updates, "⬆", "Version & install");
                ui.add_space(14.0);
                if ui
                    .add_sized([ui.available_width(), 40.0], theme::primary_button("Post"))
                    .clicked()
                {
                    self.view = View::Home;
                }
                ui.add_space(14.0);
                if let Some(name) = self
                    .workspace
                    .as_ref()
                    .and_then(Workspace::current_profile)
                    .map(|profile| profile.display_name)
                {
                    let did = self
                        .workspace
                        .as_ref()
                        .and_then(|workspace| workspace.human.as_ref())
                        .map(|did| did.as_str().to_owned())
                        .unwrap_or_default();
                    ui.horizontal(|ui| {
                        theme::avatar(ui, &name, &did, 34.0);
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(&name).strong());
                            theme::muted(ui, &short_did(&did));
                        });
                    });
                }
                ui.add_space(8.0);
                theme::muted(
                    ui,
                    "Your keys. Your choices.
No tracking. No forced updates.",
                );
            });
    }

    fn discovery_column(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        egui::SidePanel::right("discovery_sidebar")
            .resizable(false)
            .default_width(300.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::symmetric(16, 16)),
            )
            .show(ctx, |ui| {
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.timeline_query)
                        .hint_text("🔍  Search Mininet")
                        .desired_width(f32::INFINITY),
                );
                if search.changed() && !self.timeline_query.trim().is_empty() {
                    self.view = View::Discover;
                }
                ui.add_space(14.0);

                theme::card_frame().show(ui, |ui| {
                    theme::section_title(ui, "Your network");
                    ui.add_space(4.0);
                    match self.host.as_ref() {
                        Some(host) if host.listening => {
                            ui.horizontal(|ui| {
                                theme::pill_badge(ui, "HOSTING", theme::ONLINE_GREEN);
                                theme::muted(
                                    ui,
                                    &format!("port {} · {} served", host.port, host.served),
                                );
                            });
                        }
                        Some(_) => theme::muted(ui, "Starting host…"),
                        None => theme::muted(ui, "Not accepting connections."),
                    }
                    match self.network_session.as_ref() {
                        Some(session) => {
                            for slot in session.peers() {
                                ui.horizontal(|ui| {
                                    let (glyph, color) = match slot.last_ok() {
                                        Some(true) => ("✔", theme::ONLINE_GREEN),
                                        Some(false) => ("✖", theme::WARN_AMBER),
                                        None => ("•", theme::TEXT_SECONDARY),
                                    };
                                    ui.label(egui::RichText::new(glyph).color(color));
                                    ui.label(
                                        egui::RichText::new(self.peer_label(slot.endpoint()))
                                            .small(),
                                    );
                                    theme::muted(ui, &format!("{}s", slot.due_in(now)));
                                });
                            }
                            ui.horizontal(|ui| {
                                if ui.add(theme::secondary_button("Sync now")).clicked() {
                                    if let Some(session) = self.network_session.as_mut() {
                                        session.sync_now(now);
                                    }
                                }
                                if ui.add(theme::secondary_button("Stop")).clicked() {
                                    self.stop_session();
                                }
                            });
                        }
                        None if self.connections.peers.is_empty() => {
                            theme::muted(ui, "No saved peers yet.");
                            if ui.add(theme::primary_button("Add a peer")).clicked() {
                                self.view = View::Connections;
                            }
                        }
                        None => {
                            theme::muted(
                                ui,
                                &format!("{} saved peer(s), no session.", self.connections.peers.len()),
                            );
                            if ui.add(theme::primary_button("Start session")).clicked() {
                                self.start_session();
                            }
                        }
                    }
                });
                ui.add_space(14.0);

                let suggestions = self.follow_suggestions(3);
                if !suggestions.is_empty() {
                    theme::card_frame().show(ui, |ui| {
                        theme::section_title(ui, "Who to follow");
                        ui.add_space(4.0);
                        for (name, did) in suggestions {
                            ui.horizontal(|ui| {
                                theme::avatar(ui, &name, &did, 32.0);
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(&name).strong());
                                    theme::muted(ui, &short_did(&did));
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.add(theme::secondary_button("Follow")).clicked() {
                                            self.follow_did(&did, &name);
                                        }
                                    },
                                );
                            });
                        }
                    });
                    ui.add_space(14.0);
                }

                theme::card_frame().show(ui, |ui| {
                    theme::section_title(ui, "Make it your internet");
                    theme::muted(
                        ui,
                        "Follow people, join a community, share what you create. Chronological by default; no hidden paid ranking.",
                    );
                    ui.add_space(6.0);
                    if ui.add(theme::secondary_button("Find your people")).clicked() {
                        self.view = View::People;
                    }
                    if ui.add(theme::secondary_button("Browse communities")).clicked() {
                        self.view = View::Communities;
                    }
                });
                ui.add_space(14.0);
                if !self.activity.is_empty() {
                    theme::card_frame().show(ui, |ui| {
                        theme::section_title(ui, "Recent activity");
                        for line in self.activity.iter().rev().take(4) {
                            theme::muted(ui, line);
                        }
                    });
                }
            });
    }

    fn set_muted(&mut self, did: &str, name: &str, mute: bool) {
        let result = if mute {
            self.muted.mute(did).map(|_| ())
        } else {
            self.muted.unmute(did);
            Ok(())
        };
        self.notice = match result.and_then(|()| mute_list::save(&data_root(), &self.muted)) {
            Ok(()) if mute => format!(
                "Muted {name} on this device. Their posts, suggestions and directory entry are hidden here; nothing was published or deleted."
            ),
            Ok(()) => format!("Unmuted {name}."),
            Err(error) => format!("Mute list not saved: {error}"),
        };
        self.timeline_refresh = Instant::now();
    }

    /// Owner label for an endpoint, or the endpoint itself.
    fn peer_label(&self, endpoint: &str) -> String {
        self.connections
            .peers
            .iter()
            .find(|peer| peer.endpoint == endpoint)
            .map(|peer| peer.label.clone())
            .unwrap_or_else(|| endpoint.to_owned())
    }

    /// Received signed profiles the owner does not follow yet.
    fn follow_suggestions(&self, limit: usize) -> Vec<(String, String)> {
        let Some(workspace) = self.workspace.as_ref() else {
            return Vec::new();
        };
        let own = workspace.human.clone();
        workspace
            .known_profiles()
            .into_iter()
            .filter(|profile| own.as_ref() != Some(&profile.human))
            .filter(|profile| !self.muted.contains(profile.human.as_str()))
            .filter(|profile| !workspace.follows(&profile.human))
            .take(limit)
            .map(|profile| (profile.display_name, profile.human.as_str().to_owned()))
            .collect()
    }

    fn follow_did(&mut self, did: &str, name: &str) {
        self.notice = match self.workspace.as_mut() {
            Some(workspace) => match workspace.set_follow_target_confirmed(did, true) {
                Ok(()) => {
                    format!("Following {name}. The signed follow goes out on the next exchange.")
                }
                Err(error) => format!("Could not follow {name}: {error}"),
            },
            None => "Local workspace unavailable.".to_string(),
        };
        self.timeline_refresh = Instant::now();
    }

    // ----- timeline -------------------------------------------------------

    fn poll_timeline(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = self.timeline_rx.as_ref() {
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Timeline worker stopped. Retry refresh.".into()))
                }
            };
            if let Some(result) = result {
                self.timeline_rx = None;
                self.timeline_refresh = Instant::now() + Duration::from_secs(5);
                match result {
                    Ok(cards) => {
                        if self.view != View::Home && !self.timeline_cards.is_empty() {
                            for card in &cards {
                                let id = card.id.as_str();
                                if !card.own
                                    && !self.muted.contains(&card.did)
                                    && !self.timeline_cards.iter().any(|old| old.id.as_str() == id)
                                    && !self.unseen_posts.iter().any(|seen| seen == id)
                                {
                                    self.unseen_posts.push(id.to_owned());
                                }
                            }
                        }
                        self.timeline_cards = cards;
                        self.timeline_error = None;
                    }
                    Err(error) => self.timeline_error = Some(error),
                }
            }
        }
        if self.view == View::Home {
            self.unseen_posts.clear();
        }
        let networking = self.network_session.is_some() || self.host.is_some();
        if !matches!(self.view, View::Home | View::Discover | View::Media) && !networking {
            return;
        }
        let wanted = (self.feed_filter, self.timeline_scope);
        if self.timeline_rx.is_none()
            && (Instant::now() >= self.timeline_refresh || self.timeline_loaded != wanted)
        {
            if self.timeline_loaded != wanted {
                self.timeline_cards.clear();
            }
            let Some((root, human)) = self.workspace.as_ref().and_then(|workspace| {
                workspace
                    .human
                    .clone()
                    .map(|human| (workspace.root.clone(), human))
            }) else {
                return;
            };
            self.timeline_loaded = wanted;
            let (filter, scope) = wanted;
            let (sender, receiver) = mpsc::channel();
            self.timeline_rx = Some(receiver);
            let repaint = ctx.clone();
            std::thread::spawn(move || {
                let _ = sender.send(timeline::snapshot(&root, &human, filter, scope));
                repaint.request_repaint();
            });
        }
        ctx.request_repaint_after(Duration::from_secs(5));
    }

    fn timeline_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (scope, label) in [
                (timeline::Scope::Following, "Following"),
                (timeline::Scope::Everyone, "Everyone"),
            ] {
                let selected = self.timeline_scope == scope;
                let text = egui::RichText::new(label).strong();
                let text = if selected {
                    text.color(theme::TEXT_PRIMARY)
                } else {
                    text.color(theme::TEXT_SECONDARY)
                };
                if ui.selectable_label(selected, text).clicked() {
                    self.timeline_scope = scope;
                }
            }
            ui.separator();
            egui::ComboBox::from_id_salt("feed_filter")
                .selected_text(match self.feed_filter {
                    FeedFilter::Chronological => "Newest first",
                    FeedFilter::MostSupported => "Most supported",
                    _ => "Custom",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.feed_filter,
                        FeedFilter::Chronological,
                        "Newest first",
                    );
                    ui.selectable_value(
                        &mut self.feed_filter,
                        FeedFilter::MostSupported,
                        "Most supported",
                    );
                });
            if ui
                .add(theme::secondary_button("🔄"))
                .on_hover_text("Refresh")
                .clicked()
            {
                self.timeline_refresh = Instant::now();
            }
        });
    }

    fn render_timeline(&mut self, ui: &mut egui::Ui, media_only: bool) {
        if self.timeline_rx.is_some() && self.timeline_cards.is_empty() {
            ui.horizontal(|ui| {
                ui.spinner();
                theme::muted(ui, "Loading timeline…");
            });
        }
        if let Some(error) = &self.timeline_error {
            ui.colored_label(
                theme::WARN_AMBER,
                format!("Could not refresh: {error}. Showing the last received view."),
            );
        }
        let query = if self.view == View::Discover {
            self.timeline_query.trim().to_lowercase()
        } else {
            String::new()
        };
        let cards: Vec<_> = self
            .timeline_cards
            .iter()
            .filter(|card| {
                !self.muted.contains(&card.did)
                    && (!media_only || card.media.is_some())
                    && (query.is_empty()
                        || card.body.to_lowercase().contains(&query)
                        || card.author.to_lowercase().contains(&query)
                        || card.did.to_lowercase().contains(&query))
            })
            .cloned()
            .collect();
        if cards.is_empty() && self.timeline_rx.is_none() {
            ui.add_space(24.0);
            theme::card_frame().show(ui, |ui| {
                ui.heading(if !query.is_empty() {
                    "No matching posts received yet"
                } else if self.timeline_scope == timeline::Scope::Following {
                    "Your network starts with people"
                } else {
                    "Nothing received yet"
                });
                theme::muted(
                    ui,
                    "Connect to a peer to exchange posts and profiles, then follow the people you find. Saved content stays available offline.",
                );
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.add(theme::primary_button("Connect a peer")).clicked() {
                        self.view = View::Connections;
                    }
                    if ui.add(theme::secondary_button("Find people")).clicked() {
                        self.view = View::People;
                    }
                    if self.timeline_scope == timeline::Scope::Following
                        && ui.add(theme::secondary_button("Show everyone")).clicked()
                    {
                        self.timeline_scope = timeline::Scope::Everyone;
                    }
                });
            });
        }
        for card in cards {
            ui.push_id(&card.id, |ui| {
                self.post_card(ui, &card);
            });
        }
    }

    fn discover(&mut self, ui: &mut egui::Ui) {
        ui.add_sized(
            [ui.available_width(), 40.0],
            egui::TextEdit::singleline(&mut self.timeline_query)
                .hint_text("🔍  Search posts, people or a DID"),
        );
        theme::muted(
            ui,
            "Search scope: the latest 50 posts in your received timeline. This is not internet-wide search.",
        );
        ui.add_space(6.0);
        if self.timeline_scope != timeline::Scope::Everyone {
            self.timeline_scope = timeline::Scope::Everyone;
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(theme::secondary_button("People directory"))
                .clicked()
            {
                self.view = View::People;
            }
            if ui.add(theme::secondary_button("Communities")).clicked() {
                self.view = View::Communities;
            }
            if ui.add(theme::secondary_button("🔄  Refresh")).clicked() {
                self.timeline_refresh = Instant::now();
            }
        });
        ui.add_space(8.0);
        self.render_timeline(ui, false);
    }

    fn media_timeline(&mut self, ui: &mut egui::Ui) {
        theme::muted(
            ui,
            "Photo and video posts you have received. Images show inline once every chunk has arrived; video playback is not integrated yet.",
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(theme::primary_button("🖼  Share a photo or video"))
                .clicked()
            {
                self.view = View::Creator;
            }
        });
        ui.add_space(6.0);
        self.timeline_controls(ui);
        ui.add_space(8.0);
        self.render_timeline(ui, true);
    }

    /// Decode a post's image once and cache the texture; a manifest that is
    /// incomplete or not an image is remembered as unavailable.
    fn media_texture(
        &mut self,
        ctx: &egui::Context,
        manifest: &mini_objects::ObjectId,
    ) -> Option<egui::TextureHandle> {
        if let Some(cached) = self.media_textures.get(manifest.as_str()) {
            return cached.clone();
        }
        let texture = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.profile_image(manifest).ok())
            .and_then(|bytes| decode_profile_image(&bytes).ok())
            .map(|(image, _)| {
                let image = image.thumbnail(640, 640);
                let rgba = image.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                ctx.load_texture(
                    format!("media:{}", manifest.as_str()),
                    color,
                    egui::TextureOptions::LINEAR,
                )
            });
        self.media_textures
            .insert(manifest.as_str().to_string(), texture.clone());
        texture
    }

    /// What a media post links to, for the non-image fallback.
    fn media_description(&self, manifest: &mini_objects::ObjectId) -> String {
        let Some(workspace) = self.workspace.as_ref() else {
            return "media unavailable".into();
        };
        let Ok(object) = workspace.store.get(manifest) else {
            return "media manifest not received yet".into();
        };
        let Ok(manifest) = read_manifest(&object) else {
            return "unreadable media manifest".into();
        };
        let complete = mini_media::missing_chunks(&workspace.store, &manifest)
            .map(|missing| missing.is_empty())
            .unwrap_or(false);
        format!(
            "{} · {} KB · {}",
            manifest.content_type,
            manifest.total_len / 1024,
            if complete {
                "received"
            } else {
                "still arriving from peers"
            }
        )
    }

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

        self.timeline_refresh = Instant::now();
        let Some(endpoint) = nearby_endpoint_for(&self.nearby_profiles, &profile.human) else {
            self.notice = if self.network_session.is_some() || self.host.is_some() {
                format!(
                    "Following {}. The signed follow goes out on the next exchange.",
                    profile.display_name
                )
            } else {
                format!(
                    "Following {}. Start a session in Connections to deliver the signed follow.",
                    profile.display_name
                )
            };
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

    fn post_card(&mut self, ui: &mut egui::Ui, card: &timeline::Card) {
        theme::row_frame().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                match card
                    .avatar
                    .as_ref()
                    .and_then(|avatar| self.profile_texture(ui.ctx(), avatar))
                {
                    Some(texture) => {
                        ui.add(
                            egui::Image::new((texture.id(), egui::vec2(42.0, 42.0)))
                                .corner_radius(21.0),
                        );
                    }
                    None => {
                        theme::avatar(ui, &card.author, &card.did, 42.0);
                    }
                }
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(&card.author)
                                .strong()
                                .color(theme::TEXT_PRIMARY),
                        );
                        theme::muted(ui, &short_did(&card.did));
                        theme::muted(
                            ui,
                            &format!("· {}", timeline::age(card.timestamp_ms, now_ms())),
                        );
                        if card.own {
                            theme::pill_badge(ui, "You", theme::ACCENT);
                        } else if card.reason == "Received from a peer" {
                            theme::pill_badge(ui, "Not followed", theme::TEXT_SECONDARY);
                        }
                    });
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(&card.body).color(theme::TEXT_PRIMARY));
                    if let Some(manifest) = card.media.as_ref() {
                        ui.add_space(6.0);
                        match self.media_texture(ui.ctx(), manifest) {
                            Some(texture) => {
                                let size = texture.size_vec2();
                                let width = ui.available_width().min(size.x).min(520.0);
                                let scale = width / size.x.max(1.0);
                                ui.add(
                                    egui::Image::new((texture.id(), size * scale))
                                        .corner_radius(12.0),
                                );
                            }
                            None => {
                                let description = self.media_description(manifest);
                                theme::card_frame().show(ui, |ui| {
                                    ui.label(egui::RichText::new("🎬  Media attachment").strong());
                                    theme::muted(ui, &description);
                                });
                            }
                        }
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if theme::icon_action(
                            ui,
                            "💬",
                            &card.comment_count.to_string(),
                            theme::ACCENT,
                        ) {
                            self.reply_target = Some(card.id.clone());
                        }
                        ui.add_space(10.0);
                        if theme::icon_action(
                            ui,
                            "♥",
                            &card.support_count.to_string(),
                            theme::LIKE_PINK,
                        ) {
                            self.notice = if let Some(workspace) = self.workspace.as_mut() {
                                match workspace.react_like(&card.id) {
                                    Ok(()) => {
                                        "Like signed and saved. It shares on the next exchange."
                                            .to_string()
                                    }
                                    Err(error) => format!("Could not react: {error}"),
                                }
                            } else {
                                "Local workspace unavailable.".to_string()
                            };
                            self.timeline_refresh = Instant::now();
                        }
                        ui.add_space(10.0);
                        if !card.own {
                            let follows = self
                                .workspace
                                .as_ref()
                                .zip(Did::parse(&card.did).ok())
                                .is_some_and(|(workspace, did)| workspace.follows(&did));
                            if !follows && theme::icon_action(ui, "👥", "Follow", theme::ACCENT) {
                                let (did, name) = (card.did.clone(), card.author.clone());
                                self.follow_did(&did, &name);
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.menu_button("ℹ", |ui| {
                                ui.set_min_width(420.0);
                                ui.label(egui::RichText::new("Post identity").strong());
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&card.did).monospace().small(),
                                    )
                                    .wrap_mode(egui::TextWrapMode::Extend),
                                );
                                theme::muted(ui, card.reason);
                                theme::muted(
                                    ui,
                                    "Time is claimed by the author; it is not a server receipt.",
                                );
                                if ui.button("Copy author DID").clicked() {
                                    ui.ctx().copy_text(card.did.clone());
                                    ui.close_menu();
                                }
                                if !card.own && ui.button("Mute author on this device").clicked() {
                                    let (did, name) = (card.did.clone(), card.author.clone());
                                    self.set_muted(&did, &name, true);
                                    ui.close_menu();
                                }
                            });
                        });
                    });
                });
            });
        });
        ui.separator();
    }

    fn nav_button(&mut self, ui: &mut egui::Ui, view: View, glyph: &str, label: &str) {
        let selected = self.view == view;
        let text = egui::RichText::new(format!("{glyph}  {label}"))
            .size(15.5)
            .color(if selected {
                theme::TEXT_PRIMARY
            } else {
                theme::TEXT_SECONDARY
            });
        let text = if selected { text.strong() } else { text };
        let response = ui.add_sized(
            [ui.available_width(), 40.0],
            egui::SelectableLabel::new(selected, text),
        );
        if response.clicked() {
            self.view = view;
        }
        ui.add_space(2.0);
    }

    fn header(&self, ui: &mut egui::Ui) {
        let (title, subtitle) = match self.view {
            View::Onboarding => (
                "Welcome to Mininet",
                "Create your local root, then publish the public profile you choose to share.",
            ),
            View::Home => (
                "Home",
                "People you follow. Conversations that matter. Your choice of order.",
            ),
            View::Discover => (
                "Explore",
                "Search everything your device has received. Connect peers to bring more into view.",
            ),
            View::Media => ("Media", "Photo and video posts from people on your network."),
            View::Inbox => (
                "Messages",
                "Encrypted conversations. Delivered through your connection sessions when you allow it.",
            ),
            View::People => (
                "People",
                "Signed profiles already on your device, plus opt-in nearby discovery.",
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
                "Your saved peers, hosting, and the sessions that keep content flowing.",
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
        ui.label(
            egui::RichText::new(title)
                .strong()
                .size(24.0)
                .color(theme::TEXT_PRIMARY),
        );
        theme::muted(ui, subtitle);
    }

    fn home(&mut self, ui: &mut egui::Ui) {
        let (name, did) = self
            .workspace
            .as_ref()
            .and_then(|workspace| {
                workspace.current_profile().map(|profile| {
                    (
                        profile.display_name,
                        workspace
                            .human
                            .as_ref()
                            .map(|did| did.as_str().to_owned())
                            .unwrap_or_default(),
                    )
                })
            })
            .unwrap_or_else(|| ("You".to_string(), String::new()));
        theme::card_frame().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                theme::avatar(ui, &name, &did, 44.0);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    ui.add(
                        egui::TextEdit::multiline(&mut self.composer)
                            .hint_text("What is happening?")
                            .frame(false)
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                    ui.checkbox(
                        &mut self.signing_confirmation,
                        "I confirm this creates a signed Mininet object",
                    );
                    ui.horizontal(|ui| {
                        if ui.add(theme::secondary_button("🖼  Media")).clicked() {
                            self.view = View::Creator;
                        }
                        if ui.add(theme::secondary_button("🏢  Community")).clicked() {
                            self.view = View::Communities;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let unlocked =
                                self.workspace.as_ref().is_some_and(Workspace::is_unlocked);
                            if ui
                                .add_enabled(unlocked, theme::primary_button("Post"))
                                .on_disabled_hover_text("Unlock your identity to sign a post.")
                                .clicked()
                            {
                                self.notice = if self.composer.trim().is_empty() {
                                    "Nothing published: write something first.".to_string()
                                } else if !self.signing_confirmation {
                                    "Confirm signing before publishing.".to_string()
                                } else if let Some(workspace) = self.workspace.as_mut() {
                                    match workspace.publish_post(self.composer.trim()) {
                                        Ok(()) => {
                                            self.composer.clear();
                                            self.signing_confirmation = false;
                                            self.timeline_refresh = Instant::now();
                                            if self.network_session.is_some() || self.host.is_some() {
                                                "Posted. It shares with your peers on the next exchange.".to_string()
                                            } else {
                                                "Posted and saved locally. Start a session in Connections to share it.".to_string()
                                            }
                                        }
                                        Err(error) => format!("Could not publish: {error}"),
                                    }
                                } else {
                                    "Local workspace unavailable; no content was published.".to_string()
                                };
                            }
                        });
                    });
                });
            });
        });
        ui.add_space(10.0);
        self.timeline_controls(ui);
        ui.add_space(6.0);
        if let Some(target) = self.reply_target.clone() {
            theme::card_frame().show(ui, |ui| {
                ui.label(egui::RichText::new("Reply to selected post").strong());
                ui.add(
                    egui::TextEdit::multiline(&mut self.reply_text)
                        .desired_width(f32::INFINITY)
                        .desired_rows(2),
                );
                ui.checkbox(
                    &mut self.signing_confirmation,
                    "I confirm this creates a signed reply",
                );
                ui.horizontal(|ui| {
                    if ui.add(theme::primary_button("Reply")).clicked() {
                        self.notice = if self.reply_text.trim().is_empty() {
                            "Write a reply first.".to_string()
                        } else if !self.signing_confirmation {
                            "Confirm signing before publishing.".to_string()
                        } else if let Some(workspace) = self.workspace.as_mut() {
                            match workspace.publish_comment(&target, self.reply_text.trim()) {
                                Ok(()) => {
                                    self.reply_text.clear();
                                    self.reply_target = None;
                                    self.signing_confirmation = false;
                                    self.timeline_refresh = Instant::now();
                                    "Reply signed and saved. It shares on the next exchange."
                                        .to_string()
                                }
                                Err(error) => format!("Could not publish reply: {error}"),
                            }
                        } else {
                            "Local workspace unavailable.".to_string()
                        };
                    }
                    if ui.add(theme::secondary_button("Cancel")).clicked() {
                        self.reply_target = None;
                    }
                });
            });
            ui.add_space(6.0);
        }
        self.render_timeline(ui, false);
    }

    fn connections(&mut self, ui: &mut egui::Ui) {
        let now = Instant::now();
        let own_did = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.human.as_ref())
            .map(|did| did.as_str().to_owned());
        let own_name = self
            .workspace
            .as_ref()
            .and_then(Workspace::current_profile)
            .map(|profile| profile.display_name);

        // --- status -------------------------------------------------------
        theme::card_frame().show(ui, |ui| {
            let (state, color) = self.connection_state();
            ui.horizontal(|ui| {
                theme::pill_badge(ui, &state.to_uppercase(), color);
                theme::muted(ui, &self.connection_summary());
            });
            ui.add_space(6.0);
            match self.network_session.as_ref() {
                Some(session) => {
                    for slot in session.peers() {
                        ui.horizontal_wrapped(|ui| {
                            let (glyph, color) = match slot.last_ok() {
                                Some(true) => ("✔", theme::ONLINE_GREEN),
                                Some(false) => ("✖", theme::WARN_AMBER),
                                None => ("•", theme::TEXT_SECONDARY),
                            };
                            ui.label(egui::RichText::new(glyph).color(color));
                            ui.label(egui::RichText::new(self.peer_label(slot.endpoint())).strong());
                            theme::muted(ui, slot.endpoint());
                            theme::muted(
                                ui,
                                &format!(
                                    "· {} ok / {} failed · next in {}s",
                                    slot.successes(),
                                    slot.failures(),
                                    slot.due_in(now)
                                ),
                            );
                        });
                        if !slot.last_summary().is_empty() {
                            theme::muted(ui, slot.last_summary());
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui.add(theme::secondary_button("Sync now")).clicked() {
                            if let Some(session) = self.network_session.as_mut() {
                                session.sync_now(now);
                            }
                        }
                        if ui.add(theme::secondary_button("Stop session")).clicked() {
                            self.stop_session();
                        }
                    });
                }
                None => {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Session length:");
                        for length in [
                            network_session::SessionLength::Short,
                            network_session::SessionLength::Hour,
                            network_session::SessionLength::WhileOpen,
                        ] {
                            if ui
                                .selectable_value(
                                    &mut self.connections.session_length,
                                    length,
                                    length.label(),
                                )
                                .changed()
                            {
                                self.save_connections();
                            }
                        }
                    });
                    let can_start = !self.connections.peers.is_empty();
                    if ui
                        .add_enabled(can_start, theme::primary_button("▶  Start session"))
                        .on_disabled_hover_text("Save a peer first.")
                        .clicked()
                    {
                        self.start_session();
                    }
                    theme::muted(
                        ui,
                        "A session dials each saved peer every 30 seconds (backing off to 2 minutes after failures), exchanges public posts, profiles, follows and reactions, and stops at the chosen limit. It never restarts on its own unless you enable that below.",
                    );
                }
            }
        });
        ui.add_space(12.0);

        // --- hosting ------------------------------------------------------
        theme::card_frame().show(ui, |ui| {
            theme::section_title(ui, "Accept connections (host)");
            theme::muted(
                ui,
                "Let peers reach you. The port must be reachable from the internet (router port-forward, VPS, or the same LAN); no NAT traversal or relay is provided yet. Only signed public objects and, if enabled, your own conversations' encrypted envelopes are exchanged. The first time you host, Windows Firewall asks whether to allow mininet-desktop; hosting only works if you allow it.",
            );
            match self.host.as_ref() {
                Some(host) => {
                    ui.horizontal_wrapped(|ui| {
                        theme::pill_badge(
                            ui,
                            if host.listening { "HOSTING" } else { "STARTING" },
                            theme::ONLINE_GREEN,
                        );
                        theme::muted(
                            ui,
                            &format!(
                                "port {} · {} served · {} failed · {} min",
                                host.port,
                                host.served,
                                host.failed,
                                host.started.elapsed().as_secs() / 60
                            ),
                        );
                    });
                    if !host.last.is_empty() {
                        theme::muted(ui, &host.last);
                    }
                    if ui.add(theme::secondary_button("■  Stop hosting")).clicked() {
                        self.stop_host();
                    }
                }
                None => {
                    ui.horizontal(|ui| {
                        ui.label("Port");
                        if ui
                            .add(egui::TextEdit::singleline(&mut self.listen_port).desired_width(80.0))
                            .lost_focus()
                        {
                            match self.listen_port.trim().parse::<u16>() {
                                Ok(port) if port != 0 => {
                                    self.connections.listen_port = port;
                                    self.save_connections();
                                }
                                _ => {
                                    self.listen_port = self.connections.listen_port.to_string();
                                    self.notice = "Enter a valid non-zero port.".into();
                                }
                            }
                        }
                        if ui.add(theme::primary_button("▶  Start hosting")).clicked() {
                            self.start_host();
                        }
                    });
                }
            }
        });
        ui.add_space(12.0);

        // --- your card ----------------------------------------------------
        if let (Some(did), Some(name)) = (own_did.as_ref(), own_name.as_ref()) {
            theme::card_frame().show(ui, |ui| {
                theme::section_title(ui, "Your connection card");
                theme::muted(
                    ui,
                    "Send this to a friend. It carries the address peers can reach you at and your DID; it grants nothing by itself. Use your public IP or hostname for the internet, or your LAN address for the same network.",
                );
                ui.horizontal(|ui| {
                    ui.label("Reachable host");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.card_host)
                            .hint_text("public IP, hostname, or LAN address")
                            .desired_width(240.0),
                    );
                    if ui
                        .add(theme::secondary_button("Detect LAN address"))
                        .on_hover_text("Asks the OS which local address routes outward. No packet is sent.")
                        .clicked()
                    {
                        match connectivity::local_address() {
                            Ok(address) => {
                                self.card_host = address;
                                self.notice = "LAN address filled in. Peers outside your network still need your public IP or a port-forward.".into();
                            }
                            Err(error) => self.notice = format!("Could not detect a LAN address: {error}"),
                        }
                    }
                });
                let host = if self.card_host.trim().is_empty() {
                    "YOUR-PUBLIC-IP".to_string()
                } else {
                    self.card_host.trim().to_string()
                };
                let card = connectivity::ConnectionCard {
                    endpoint: format!("{host}:{}", self.connections.listen_port),
                    did: did.clone(),
                    name: name.clone(),
                }
                .encode();
                ui.add(
                    egui::TextEdit::singleline(&mut card.clone())
                        .desired_width(f32::INFINITY)
                        .interactive(false),
                );
                if ui.add(theme::secondary_button("📋  Copy card")).clicked() {
                    ui.ctx().copy_text(card);
                    self.notice = if self.card_host.trim().is_empty() {
                        "Connection card copied. Replace YOUR-PUBLIC-IP with an address peers can reach.".into()
                    } else {
                        "Connection card copied.".into()
                    };
                }
            });
            ui.add_space(12.0);
        }

        // --- peers --------------------------------------------------------
        theme::card_frame().show(ui, |ui| {
            theme::section_title(ui, "Saved peers");
            if self.connections.peers.is_empty() {
                theme::muted(ui, "No peers saved. Paste a friend's connection card or add an endpoint by hand.");
            }
            let mut remove: Option<String> = None;
            let mut dial: Option<String> = None;
            for peer in self.connections.peers.clone() {
                ui.horizontal_wrapped(|ui| {
                    theme::avatar(ui, &peer.label, peer.did.as_deref().unwrap_or(&peer.endpoint), 30.0);
                    ui.label(egui::RichText::new(&peer.label).strong());
                    theme::muted(ui, &peer.endpoint);
                    if let Some(did) = peer.did.as_deref() {
                        theme::muted(ui, &short_did(did));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(theme::secondary_button("Remove")).clicked() {
                            remove = Some(peer.endpoint.clone());
                        }
                        if ui
                            .add_enabled(self.sync_rx.is_none(), theme::secondary_button("Sync once"))
                            .clicked()
                        {
                            dial = Some(peer.endpoint.clone());
                        }
                    });
                });
            }
            if let Some(endpoint) = remove {
                self.connections.remove_peer(&endpoint);
                self.save_connections();
                self.notice = "Peer removed. A running session keeps its own list until restarted.".into();
            }
            if let Some(endpoint) = dial {
                self.peer_address = endpoint;
                self.start_peer_sync(false, None);
            }
            ui.separator();
            ui.label(egui::RichText::new("Add from a connection card").strong());
            ui.add(
                egui::TextEdit::multiline(&mut self.card_input)
                    .hint_text("mininet-peer-v1;endpoint=…;did=…;name=…")
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            if ui.add(theme::primary_button("Add peer and follow")).clicked() {
                self.notice = match connectivity::ConnectionCard::decode(&self.card_input) {
                    Ok(card) => {
                        match self.connections.upsert_peer(&card.name, &card.endpoint, Some(&card.did)) {
                            Ok(_) => {
                                self.save_connections();
                                self.card_input.clear();
                                let follow = self.workspace.as_mut().map(|workspace| {
                                    workspace.set_follow_target_confirmed(&card.did, true)
                                });
                                match follow {
                                    Some(Ok(())) => format!(
                                        "Saved {} and signed a follow. Start a session to exchange with them.",
                                        card.name
                                    ),
                                    Some(Err(error)) => format!(
                                        "Saved {}, but the follow was not signed: {error}",
                                        card.name
                                    ),
                                    None => format!("Saved {}.", card.name),
                                }
                            }
                            Err(error) => error,
                        }
                    }
                    Err(error) => error,
                };
            }
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Or add an endpoint by hand").strong());
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_peer_label)
                        .hint_text("Name")
                        .desired_width(140.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_peer_endpoint)
                        .hint_text("peer.example:46000 or [IPv6]:46000")
                        .desired_width(240.0),
                );
                if ui.add(theme::secondary_button("Save peer")).clicked() {
                    self.notice = match self.connections.upsert_peer(
                        &self.new_peer_label,
                        &self.new_peer_endpoint,
                        None,
                    ) {
                        Ok(_) => {
                            self.save_connections();
                            self.new_peer_label.clear();
                            self.new_peer_endpoint.clear();
                            "Peer saved.".into()
                        }
                        Err(error) => error,
                    };
                }
            });
            theme::muted(ui, "An endpoint is a dial hint, not a verified identity. Names and profiles are verified only when their signed objects arrive.");
        });
        ui.add_space(12.0);

        // --- policy -------------------------------------------------------
        theme::card_frame().show(ui, |ui| {
            theme::section_title(ui, "Connection policy");
            let mut changed = false;
            changed |= ui
                .checkbox(
                    &mut self.connections.session_on_launch,
                    "Start a session with my saved peers when Mininet opens",
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut self.connections.host_on_launch,
                    "Accept connections when Mininet opens",
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut self.connections.include_private,
                    "Include my private conversations in sessions and hosting",
                )
                .changed();
            theme::muted(
                ui,
                "Private conversations exchange only encrypted envelopes for routes both sides already hold. Changing this takes effect for the next session or hosting window.",
            );
            if changed {
                self.save_connections();
                self.notice = "Connection policy saved. These are the only ways Mininet starts networking on launch.".into();
            }
        });
        ui.add_space(12.0);

        // --- activity -----------------------------------------------------
        if !self.activity.is_empty() {
            theme::card_frame().show(ui, |ui| {
                theme::section_title(ui, "Activity");
                for line in self.activity.iter().rev().take(12) {
                    theme::muted(ui, line);
                }
            });
            ui.add_space(12.0);
        }

        // --- advanced -----------------------------------------------------
        ui.collapsing("Advanced: one-shot sync and offline transfer", |ui| {
            theme::card_frame().show(ui, |ui| {
                ui.label(egui::RichText::new("One-shot direct peer sync").strong());
                theme::muted(ui, "Encrypted TCP bearer + verified MINI/SYNC1 ingest. One connection, then it stops.");
                ui.horizontal(|ui| {
                    ui.label("Peer address");
                    ui.text_edit_singleline(&mut self.peer_address);
                });
                ui.horizontal(|ui| {
                    ui.label("Listen port");
                    ui.text_edit_singleline(&mut self.listen_port);
                });
                ui.horizontal(|ui| {
                    if ui.add(theme::secondary_button("Connect once")).clicked() {
                        self.start_peer_sync(false, None);
                    }
                    if ui.add(theme::secondary_button("Listen once")).clicked() {
                        self.start_peer_sync(true, None);
                    }
                });
                if self.sync_rx.is_some() || self.visibility_rx.is_some() {
                    theme::muted(ui, "Peer operation active…");
                }
            });
            ui.add_space(8.0);
            theme::card_frame().show(ui, |ui| {
                ui.label(egui::RichText::new("Offline transfer").strong());
                theme::muted(ui, "Move signed objects by USB or a trusted folder. Bundles never contain the identity vault.");
                ui.text_edit_singleline(&mut self.export_path);
                if ui.add(theme::secondary_button("Export local objects")).clicked() {
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
                if ui.add(theme::secondary_button("Import local objects")).clicked() {
                    self.notice = if let Some(workspace) = self.workspace.as_mut() {
                        match workspace.import_bundle(self.import_path.trim()) {
                            Ok(count) => {
                                self.timeline_refresh = Instant::now();
                                format!("Imported {count} verified object(s). No network used.")
                            }
                            Err(error) => format!("Import failed: {error}"),
                        }
                    } else {
                        "Local workspace unavailable.".to_string()
                    };
                }
            });
        });
        ui.add_space(8.0);
        theme::muted(ui, "A blocked domain or unavailable peer does not delete local data. Export, peer transfer, and alternate peers remain separate paths.");
    }

    fn onboarding(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BG).inner_margin(egui::Margin::same(24)))
            .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(54.0);
                ui.heading("MININET");
                ui.label(
                    egui::RichText::new("Your identity. Your objects. Your transport choices.")
                        .color(egui::Color32::LIGHT_GRAY),
                );
                ui.add_space(22.0);
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
                                theme::card_frame().show(ui, |ui| {
                                    ui.heading("1. Create your Mininet root");
                                    ui.label("This creates a new local signing root protected by the Windows user vault. It never uploads a seed or contacts a server.");
                                    ui.label("You will be able to export recovery material only through a separate, deliberate backup flow.");
                                    if ui.button("Create local root").clicked() {
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
                                theme::card_frame().show(ui, |ui| {
                                    ui.heading("2. Create your public account");
                                    ui.label("Start with a display name and optional bio. Next, you can choose a photo, location, age, and any custom public details before becoming visible to anyone.");
                                    ui.label("Your cryptographic identity remains the DID shown in Privacy & safety.");
                                    ui.add_space(8.0);
                                    ui.label("Display name");
                                    ui.text_edit_singleline(&mut self.account_name);
                                    ui.label("Bio");
                                    ui.add_sized(
                                        [ui.available_width(), 90.0],
                                        egui::TextEdit::multiline(&mut self.account_bio),
                                    );
                                    if !is_unlocked {
                                        ui.label("This setup action will unlock the local root only long enough to sign the profile, then lock it again.");
                                    }
                                    ui.checkbox(
                                        &mut self.signing_confirmation,
                                        "I confirm this creates my signed public profile",
                                    );
                                    if ui.button("Publish public account locally").clicked() {
                                        self.notice = if self.account_name.trim().is_empty() {
                                            "Choose a display name first.".to_string()
                                        } else if !self.signing_confirmation {
                                            "Confirm signing before publishing the account.".to_string()
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
                                                    self.signing_confirmation = false;
                                                    self.view = View::Creator;
                                                    "Public account created locally and identity locked again. Add any optional public details below, or open People when you are ready.".to_string()
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
                            ui.colored_label(egui::Color32::YELLOW, "The local workspace could not be opened.");
                            ui.label(&self.notice);
                        }
                        ui.add_space(14.0);
                        ui.label(egui::RichText::new(&self.notice).small());
                    },
                );
            });
        });
    }

    fn inbox(&mut self, ui: &mut egui::Ui) {
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("How messages travel").strong());
            if self.connections.include_private {
                ui.label("Private conversations are included in your sessions and hosting: encrypted envelopes are exchanged automatically with peers that hold the same conversation.");
            } else {
                ui.label("Automatic delivery is off. Enable \"Include my private conversations\" in Connections, or use the manual one-shot sync below.");
                if ui.add(theme::secondary_button("Open Connections")).clicked() {
                    self.view = View::Connections;
                }
            }
        });
        ui.add_space(8.0);
        theme::card_frame().show(ui, |ui| {
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

        theme::card_frame().show(ui, |ui| {
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
            theme::card_frame().show(&mut columns[0], |ui| {
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
            theme::card_frame().show(&mut columns[1], |ui| {
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
            theme::card_frame().show(ui, |ui| {
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
        theme::card_frame().show(ui, |ui| {
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
                    theme::card_frame().show(ui, |ui| {
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
            if ui.add(theme::primary_button("Send")).clicked() {
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
                            if self.connections.include_private
                                && (self.network_session.is_some() || self.host.is_some())
                            {
                                "Encrypted message saved. It delivers on the next exchange.".to_string()
                            } else {
                                "Encrypted message saved locally. Enable private delivery in Connections or sync the conversation manually.".to_string()
                            }
                        }
                        Err(error) => format!("Could not send message: {error}"),
                    }
                } else {
                    "Local workspace unavailable.".to_string()
                };
            }
        });

        ui.add_space(12.0);
        theme::card_frame().show(ui, |ui| {
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
            theme::muted(ui, "Manual path. Sessions and hosting deliver automatically when private conversations are included.");
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

    fn people(&mut self, ui: &mut egui::Ui) {
        let profile_needs_upgrade = self
            .workspace
            .as_ref()
            .is_some_and(Workspace::profile_needs_device_upgrade);
        if profile_needs_upgrade {
            theme::card_frame().show(ui, |ui| {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    egui::RichText::new("One-time verified-sync upgrade").strong(),
                );
                ui.label("This account was created by an earlier desktop beta that signed directly with the human root. Peers correctly reject those objects. Re-sign the same public profile with a scoped delegated device; your DID and published details stay unchanged.");
                if ui.button("Upgrade public profile for peer sync").clicked() {
                    self.notice = match self.workspace.as_mut() {
                        Some(workspace) => match workspace.upgrade_profile_for_sync() {
                            Ok(()) => "Public profile upgraded with a delegated-device signature. Nearby verified sync is ready.".to_string(),
                            Err(error) => format!("Could not upgrade public profile: {error}"),
                        },
                        None => "Local workspace unavailable.".to_string(),
                    };
                }
            });
            ui.add_space(12.0);
        }
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("Find people").strong());
            ui.add_sized(
                [ui.available_width(), 34.0],
                egui::TextEdit::singleline(&mut self.people_search)
                    .hint_text("Search locally by display name or did:mini identifier"),
            );
            ui.label("Names are searchable labels and are not unique. The DID remains the identity anchor.");
            if ui
                .checkbox(
                    &mut self.privacy.lan_discovery,
                    "Allow opt-in nearby discovery on this local network",
                )
                .changed()
            {
                self.notice = match save_privacy_settings(self.privacy) {
                    Ok(()) => "Nearby discovery preference saved in the Windows user vault."
                        .to_string(),
                    Err(error) => format!("Could not save discovery preference: {error}"),
                };
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button("Find nearby for 3 seconds").clicked() {
                    self.start_nearby_scan();
                }
                if ui.button("Be visible nearby for 60 seconds").clicked() {
                    self.start_profile_visibility();
                }
                ui.label("Nearby visibility reveals your chosen display name and DID to the local network only during this window.");
            });
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
            ui.add_space(12.0);
            ui.label(egui::RichText::new("Nearby — not yet verified").strong());
            for profile in nearby {
                theme::card_frame().show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(&profile.display_name).strong());
                        ui.label(profile.did.as_str());
                        ui.label(profile.address.to_string());
                        if ui.button("Sync signed profile").clicked() {
                            self.peer_address = profile.address.to_string();
                            self.start_peer_sync(false, None);
                        }
                    });
                    ui.label("This LAN announcement can be spoofed. Sync and verify the signed profile before trusting its name or details.");
                });
            }
        }

        ui.add_space(12.0);
        ui.label(egui::RichText::new("Signed profiles on this device").strong());
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
            theme::muted(ui, "No matching signed profiles yet. Connect to a peer (Connections) and their profile arrives with the first exchange.");
        }
        for profile in profiles {
            let texture = profile
                .avatar
                .as_ref()
                .and_then(|avatar| self.profile_texture(ui.ctx(), avatar));
            let is_own = own_did.as_ref() == Some(&profile.human);
            let follows = self
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.follows(&profile.human));
            let friend = self
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.is_friend(&profile.human));
            theme::card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(texture) = texture {
                        ui.add(egui::Image::new((
                            texture.id(),
                            egui::vec2(76.0, 76.0),
                        )));
                    } else {
                        theme::avatar(ui, &profile.display_name, profile.human.as_str(), 76.0);
                    }
                    ui.vertical(|ui| {
                        ui.heading(&profile.display_name);
                        ui.label(&profile.bio);
                        ui.label(egui::RichText::new(profile.human.as_str()).small());
                        ui.horizontal_wrapped(|ui| {
                            if let Some(location) = &profile.location {
                                ui.label(format!("Location: {location}"));
                            }
                            if let Some(age) = profile.age {
                                ui.label(format!("Age: {age}"));
                            }
                        });
                    });
                });
                for field in &profile.fields {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(format!("{}:", field.label)).strong());
                        ui.label(&field.value);
                    });
                }
                ui.horizontal_wrapped(|ui| {
                    if is_own {
                        ui.label("This is your public profile.");
                    } else if friend {
                        ui.label(egui::RichText::new("Friends").strong());
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
                        ui.label("Friend request/follow sent");
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
                    } else if ui.add(theme::primary_button("Follow")).clicked() {
                        self.add_friend(&profile);
                    }
                    if !is_own && ui.add(theme::secondary_button("✉  Message")).clicked() {
                        self.conversation_peer = profile.human.as_str().to_string();
                        if self.conversation_label.trim().is_empty() {
                            self.conversation_label = profile.display_name.clone();
                        }
                        self.view = View::Inbox;
                        self.notice = format!(
                            "Create a sensitive invite for {} and send it to them over a trusted channel; they import it in Messages.",
                            profile.display_name
                        );
                    }
                    if ui.add(theme::secondary_button("Copy DID")).clicked() {
                        ui.ctx().copy_text(profile.human.as_str().to_string());
                    }
                    if !is_own {
                        let muted = self.muted.contains(profile.human.as_str());
                        if ui
                            .add(theme::secondary_button(if muted { "Unmute" } else { "Mute" }))
                            .clicked()
                        {
                            self.set_muted(profile.human.as_str(), &profile.display_name, !muted);
                        }
                        if muted {
                            theme::pill_badge(ui, "MUTED HERE", theme::WARN_AMBER);
                        }
                    }
                });
            });
            ui.add_space(8.0);
        }
    }

    fn communities(&mut self, ui: &mut egui::Ui) {
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("Create a local community").strong());
            ui.text_edit_singleline(&mut self.community_name);
            ui.add_sized(
                [ui.available_width(), 48.0],
                egui::TextEdit::multiline(&mut self.community_charter)
                    .hint_text("Charter and norms"),
            );
            ui.checkbox(
                &mut self.signing_confirmation,
                "I confirm this action will create a signed community object",
            );
            if ui.button("Publish community locally").clicked() {
                self.notice = if self.community_name.trim().is_empty() {
                    "A community name is required.".to_string()
                } else if !self.signing_confirmation {
                    "Confirm signing before publishing.".to_string()
                } else if let Some(workspace) = self.workspace.as_mut() {
                    match workspace.publish_community(
                        self.community_name.trim(),
                        self.community_charter.trim(),
                    ) {
                        Ok(()) => {
                            self.community_name.clear();
                            self.community_charter.clear();
                            self.signing_confirmation = false;
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
        let cards = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.communities())
            .unwrap_or_default();
        if cards.is_empty() {
            ui.label("No community cards are present locally yet.");
        }
        for (id, name, charter, member_count, joined) in cards {
            theme::card_frame().show(ui, |ui| {
                ui.heading(name);
                ui.label(charter);
                ui.label(format!("{member_count} locally known members"));
                if ui
                    .button(if joined {
                        "Leave community"
                    } else {
                        "Join community"
                    })
                    .clicked()
                {
                    self.notice = if !self.signing_confirmation {
                        "Confirm signing before changing membership.".to_string()
                    } else if let Some(workspace) = self.workspace.as_mut() {
                        match workspace.set_community_membership(&id, !joined) {
                            Ok(()) => {
                                self.signing_confirmation = false;
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
            });
        }
        ui.add_space(16.0);
        ui.label(egui::RichText::new("Community content remains fetchable by object id. Labels and local filters can change your view; they do not erase the author's copy.").italics());
    }

    fn creator(&mut self, ui: &mut egui::Ui) {
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("Your public profile").strong());
            ui.label("You choose every optional detail below. Only the display name is required; blank or disabled fields are not published.");
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
            ui.label(egui::RichText::new("Profile photo (optional)").strong());
            if self.profile_avatar.is_some() && !self.profile_remove_photo {
                ui.label("A profile photo is currently published.");
            }
            ui.horizontal_wrapped(|ui| {
                ui.label("Drop an image onto this window or paste its local path:");
                ui.text_edit_singleline(&mut self.profile_photo_path);
            });
            ui.label("PNG, JPEG, WebP, or GIF; maximum 8 MiB. The image is stored as signed Mininet media, not uploaded to a third party.");
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
                ui.label("Tip: a city or region is usually safer than a home or live location.");
            }
            ui.checkbox(&mut self.profile_share_age, "Publish my age");
            if self.profile_share_age {
                ui.add_sized(
                    [140.0, 32.0],
                    egui::TextEdit::singleline(&mut self.profile_age).hint_text("Age"),
                );
            }
            ui.separator();
            ui.label(egui::RichText::new("Custom public details (optional)").strong());
            ui.label("Add one Label: Value pair per line, such as Pronouns, Website, Languages, Interests, or Availability.");
            ui.add_sized(
                [ui.available_width(), 96.0],
                egui::TextEdit::multiline(&mut self.profile_custom_fields)
                    .hint_text("Pronouns: they/them\nWebsite: https://example.org\nLanguages: English, Slovene"),
            );
            ui.checkbox(
                &mut self.signing_confirmation,
                "I reviewed these details and want to publish them in my signed public profile",
            );
            if ui.button("Save signed public profile").clicked() {
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
                    } else if !self.signing_confirmation {
                        Err("Review the profile and confirm signing before publishing.".to_string())
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
                                    self.signing_confirmation = false;
                                    "Public profile saved locally. Use People to become visible nearby or sync it to another peer.".to_string()
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
        ui.add_space(12.0);
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("Public wall").strong());
            ui.label("A voluntary public-facing surface separate from your profile. It does not reveal another root unless you explicitly publish a linkage object.");
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
            ui.checkbox(
                &mut self.signing_confirmation,
                "I confirm this creates a signed public wall",
            );
            if ui.button("Publish wall locally").clicked() {
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
                } else if !self.signing_confirmation {
                    "Confirm signing before publishing the wall.".to_string()
                } else if let Some(workspace) = self.workspace.as_mut() {
                    match workspace.publish_public_wall(
                        self.wall_name.trim(),
                        self.wall_bio.trim(),
                        &link_refs,
                        self.wall_unlisted,
                    ) {
                        Ok(()) => {
                            self.signing_confirmation = false;
                            "Public wall written locally. No directory was contacted.".to_string()
                        }
                        Err(error) => format!("Could not publish public wall: {error}"),
                    }
                } else {
                    "Local workspace unavailable.".to_string()
                };
            }
        });
        ui.add_space(12.0);
        let target_valid =
            self.follow_target.trim().is_empty() || Did::parse(self.follow_target.trim()).is_ok();
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("People and follows").strong());
            ui.label("Exchange the full did:mini identifier through a trusted channel. Usernames are not unique contact identifiers.");
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
                    egui::TextEdit::singleline(&mut self.follow_target)
                        .hint_text("did:mini:..."),
                );
            });
            if !target_valid {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "Enter a complete did:mini identifier, not a display name.",
                );
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(target_valid && !self.follow_target.trim().is_empty(), egui::Button::new("Follow locally"))
                    .clicked()
                {
                    self.notice = if self.follow_target.trim().is_empty() {
                        "Enter a did:mini target first.".to_string()
                    } else if !self.signing_confirmation {
                        "Confirm signing before changing the follow graph.".to_string()
                    } else if let Some(workspace) = self.workspace.as_mut() {
                        match workspace.set_follow_target(&self.follow_target, true) {
                            Ok(()) => {
                                self.signing_confirmation = false;
                                "Follow object written locally.".to_string()
                            }
                            Err(error) => format!("Could not follow target: {error}"),
                        }
                    } else {
                        "Local workspace unavailable.".to_string()
                    };
                }
                if ui
                    .add_enabled(target_valid && !self.follow_target.trim().is_empty(), egui::Button::new("Unfollow locally"))
                    .clicked()
                {
                    self.notice = if self.follow_target.trim().is_empty() {
                        "Enter a did:mini target first.".to_string()
                    } else if !self.signing_confirmation {
                        "Confirm signing before changing the follow graph.".to_string()
                    } else if let Some(workspace) = self.workspace.as_mut() {
                        match workspace.set_follow_target(&self.follow_target, false) {
                            Ok(()) => {
                                self.signing_confirmation = false;
                                "Unfollow object written locally.".to_string()
                            }
                            Err(error) => format!("Could not unfollow target: {error}"),
                        }
                    } else {
                        "Local workspace unavailable.".to_string()
                    };
                }
            });
            ui.checkbox(
                &mut self.signing_confirmation,
                "I confirm this changes my signed follow graph",
            );
            if let Some(workspace) = self.workspace.as_ref() {
                ui.label(format!(
                    "Following {} · {} follower(s) · {} mutual friend(s) known locally",
                    workspace.following_count(),
                    workspace.follower_count(),
                    workspace.mutual_follow_count()
                ));
            }
        });
        ui.add_space(12.0);
        theme::card_frame().show(ui, |ui| {
            ui.heading("Your creator page");
            ui.label("Profile + pinned collections + progressive media");
            ui.separator();
            ui.label(egui::RichText::new("Publish local media").strong());
            ui.text_edit_singleline(&mut self.media_path);
            ui.text_edit_singleline(&mut self.media_content_type);
            ui.text_edit_singleline(&mut self.media_caption);
            ui.checkbox(
                &mut self.signing_confirmation,
                "I confirm this action will create a signed media post",
            );
            if ui.button("Publish media locally").clicked() {
                self.notice = if self.media_path.trim().is_empty() {
                    "Choose a local file path first.".to_string()
                } else if !self.signing_confirmation {
                    "Confirm signing before publishing media.".to_string()
                } else if let Some(workspace) = self.workspace.as_mut() {
                    match workspace.publish_media_post(
                        self.media_path.trim(),
                        self.media_content_type.trim(),
                        self.media_caption.trim(),
                    ) {
                        Ok(()) => {
                            self.signing_confirmation = false;
                            "Media chunks and linked post written locally. No upload occurred.".to_string()
                        }
                        Err(error) => format!("Could not publish media: {error}"),
                    }
                } else {
                    "Local workspace unavailable.".to_string()
                };
            }
            ui.label("The path is read locally; no file picker, browser, uploader, or remote preview is used.");
            ui.separator();
            ui.label("Collections and analytics are derived from the same local objects; no third-party dashboard is required.");
        });
        ui.add_space(12.0);
        ui.label("Media playback is designed to work from local chunks first. External catalog adapters are opt-in and never become update or identity authorities.");
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
        theme::card_frame().show(ui, |ui| {
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
                theme::card_frame().show(ui, |ui| {
                    ui.heading(value.to_string());
                    ui.label(label);
                });
            }
        });
        ui.add_space(10.0);
        theme::card_frame().show(ui, |ui| {
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
        theme::card_frame().show(ui, |ui| {
            ui.label(egui::RichText::new("Protocol coverage").strong());
            ui.label("Integrated here: signed social objects, local feed assembly, communities, threaded replies, reactions, chunked media, DPAPI identity/conversation storage, Inbox beta, offline bundles, encrypted one-shot TCP sync, Windows install inspection with re-verification and rollback, and a diagnostics suite that executes the real identity, storage, social, media, messaging, sync, governance, erasure-coding, and storage-proof code.");
            ui.label("Available in the repository but not yet a finished desktop workflow: production chat sessions/mailboxes, forge repository/PR administration, presence/keystone encounters, reward accounting, privacy-cost routing, and release adoption decisions. Diagnostics *exercises* the governed-review path end to end, which is not the same as offering a desktop workflow for running it.");
            ui.label("Those foundations are deliberately shown as boundaries rather than unsafe pretend buttons. Public object types remain inspectable and syncable when another Mininet tool creates them; private messages currently require an explicit one-shot conversation sync.");
        });
        ui.add_space(10.0);
        theme::card_frame().show(ui, |ui| {
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
        theme::card_frame().show(ui, |ui| {
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
            theme::card_frame().show(ui, |ui| {
                for line in self.install_notice.lines() {
                    ui.label(line);
                }
            });
        }
        ui.add_space(12.0);
        theme::card_frame().show(ui, |ui| {
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
        theme::card_frame().show(ui, |ui| {
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
        theme::card_frame().show(ui, |ui| {
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
        ui.label("There is no analytics client, ad SDK, embedded browser, remote configuration, or silent update executor. Networking runs only inside sessions and hosting windows you start in Connections, or on launch only if you enabled that there.");
        ui.horizontal_wrapped(|ui| {
            theme::muted(
                ui,
                &format!(
                "Launch policy: session on open {}, hosting on open {}, private conversations {}.",
                if self.connections.session_on_launch { "ON" } else { "off" },
                if self.connections.host_on_launch { "ON" } else { "off" },
                if self.connections.include_private { "included" } else { "excluded" },
            ),
            );
            if ui.add(theme::secondary_button("Change")).clicked() {
                self.view = View::Connections;
            }
        });
        if let Some(workspace) = self.workspace.as_ref() {
            if let Some(human) = workspace.human.as_ref() {
                ui.label(format!("Current session identity: {}", human.as_str()));
            } else {
                ui.label("No Mininet root exists yet. Complete onboarding to create one.");
            }
            ui.label("The identity seed envelope is protected by Windows DPAPI for the current user. This does not defend against malware or an administrator running as that user.");
        }
        ui.add_space(10.0);
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(format!("Muted on this device ({})", self.muted.len())).strong(),
        );
        if self.muted.is_empty() {
            theme::muted(ui, "Nobody. Mute an author from a post's ℹ menu or from People. Muting hides content here only; it publishes nothing.");
        } else {
            let muted: Vec<String> = self.muted.iter().map(str::to_owned).collect();
            for did in muted {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(short_did(&did)).small());
                    if ui.add(theme::secondary_button("Unmute")).clicked() {
                        self.set_muted(&did, &short_did(&did), false);
                    }
                });
            }
        }
        ui.add_space(10.0);
        ui.colored_label(egui::Color32::YELLOW, "Windows boundary");
        ui.label("This reduces Mininet's own tracking and censorship dependencies. It cannot stop a compromised Windows kernel, a malicious administrator, malware, accessibility abuse, screen capture, or a hardware/driver keylogger. Sensitive entry should use a trusted OS/device and Mininet should keep secrets out of logs and URLs.");
    }
}

/// A DID shortened for display; the full value stays one click away.
fn short_did(did: &str) -> String {
    const KEEP: usize = 18;
    if did.chars().count() <= KEEP + 1 {
        did.to_owned()
    } else {
        let head: String = did.chars().take(KEEP).collect();
        format!("{head}…")
    }
}

/// One session exchange with one saved peer: the public sync, then every
/// private route the owner opted in. Runs on a worker thread.
fn exchange_with_peer(
    root: &std::path::Path,
    endpoint: &str,
    routes: &[(String, OpaqueRoute)],
) -> Result<String, String> {
    let public = peer_link::dial_public(root, endpoint)?;
    if routes.is_empty() {
        return Ok(public);
    }
    let mut synced = 0usize;
    let mut accepted = 0usize;
    let mut absent = 0usize;
    let mut errors = Vec::new();
    for (label, route) in routes {
        match peer_link::dial_private(root, endpoint, *route) {
            Ok(peer_link::PrivateOutcome::Synced { accepted: got, .. }) => {
                synced += 1;
                accepted += got;
            }
            Ok(peer_link::PrivateOutcome::NotOnThisPeer) => absent += 1,
            Err(error) => errors.push(format!("{label}: {error}")),
        }
    }
    let mut summary = format!(
        "{public} Private: {synced} conversation(s) synced, {accepted} new envelope(s), {absent} not on this peer."
    );
    if !errors.is_empty() {
        summary.push_str(&format!(" Errors: {}", errors.join("; ")));
    }
    Ok(summary)
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
    /// The connected-beta path end to end over real TCP: Bob hosts for a
    /// window; Alice's session worker dials him twice (public + a shared
    /// private conversation). Bob ends up with Alice's profile, follow and
    /// encrypted message; Alice gets Bob's profile. The private route must
    /// be served by the multi-route host responder, and a route Bob does
    /// not hold must be declined without error.
    #[cfg(windows)]
    #[test]
    fn host_serves_session_exchanges_public_and_private_over_tcp() {
        use super::{
            conversation_state, exchange_with_peer, followers, known_profiles,
            load_desktop_identity, load_or_create, peer_link, publish_profile, set_follow,
            ConversationRecord,
        };
        use mini_messaging::{scan as scan_messages, send as send_message, MessageDraft};
        use mini_objects::OpaqueRoute;
        use mini_store::FsBackend;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{mpsc, Arc};
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
                "host test",
                None,
                1,
                0,
            )
            .unwrap();
            human
        }

        let test_root = std::env::temp_dir().join(format!(
            "mininet-desktop-host-{}-{}",
            std::process::id(),
            super::now_ms()
        ));
        let bob_root = test_root.join("bob");
        let alice_root = test_root.join("alice");
        let bob_did = profile_root(&bob_root, "Bob");
        let alice_did = profile_root(&alice_root, "Alice");

        // One shared conversation (created by Alice, imported by Bob) and one
        // Alice-only conversation Bob must decline.
        let (shared, invite) =
            ConversationRecord::create("shared".into(), bob_did.clone(), alice_did.clone())
                .unwrap();
        let (alice_only, alice_only_invite) =
            ConversationRecord::create("mine".into(), bob_did.clone(), alice_did.clone()).unwrap();
        let bob_copy = ConversationRecord::import("shared".into(), &invite).unwrap();
        let alice_records = [
            ConversationRecord::import("shared".into(), &invite).unwrap(),
            ConversationRecord::import("mine".into(), &alice_only_invite).unwrap(),
        ];
        conversation_state::save(&alice_root.join("conversations.dpapi"), &alice_records).unwrap();
        let bob_records = [ConversationRecord::import("shared".into(), &invite).unwrap()];
        conversation_state::save(&bob_root.join("conversations.dpapi"), &bob_records).unwrap();

        // Alice follows Bob and writes him a message before any connection.
        let alice_identity = load_desktop_identity(&alice_root, false).unwrap();
        {
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
            send_message(
                &mut alice_store,
                &shared.secret().unwrap(),
                alice_did.clone(),
                &alice_identity.device,
                3,
                2,
                MessageDraft::text("hello over the internet"),
            )
            .unwrap();
        }

        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let stop = Arc::new(AtomicBool::new(false));
        let (events, event_rx) = mpsc::channel();
        let host_root = bob_root.clone();
        let host_stop = Arc::clone(&stop);
        let host_routes = vec![bob_copy.route()];
        let host = std::thread::spawn(move || {
            peer_link::run_host(host_root, port, host_routes, host_stop, events)
        });
        let listening = event_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(listening, peer_link::HostEvent::Listening { .. }));

        let endpoint = format!("127.0.0.1:{port}");
        let routes: Vec<(String, OpaqueRoute)> = vec![
            ("shared".into(), shared.route()),
            ("mine".into(), alice_only.route()),
        ];
        let summary = exchange_with_peer(&alice_root, &endpoint, &routes).unwrap();
        assert!(summary.contains("1 conversation(s) synced"), "{summary}");
        assert!(summary.contains("1 not on this peer"), "{summary}");
        assert!(!summary.contains("Errors"), "{summary}");
        // A second exchange is idempotent and the host is still accepting.
        let again = exchange_with_peer(&alice_root, &endpoint, &routes).unwrap();
        assert!(again.contains("0 new envelope(s)"), "{again}");

        stop.store(true, Ordering::Relaxed);
        host.join().unwrap();
        let mut served = 0;
        let mut declined = 0;
        for event in event_rx.try_iter() {
            if let peer_link::HostEvent::Connection { result, .. } = event {
                let text = result.unwrap();
                served += 1;
                if text.contains("declined") {
                    declined += 1;
                }
            }
        }
        assert_eq!(served, 6);
        assert_eq!(declined, 2);

        let bob_store = Store::new(FsBackend::open(&bob_root).unwrap());
        let names: Vec<String> = known_profiles(&bob_store)
            .unwrap()
            .into_iter()
            .map(|profile| profile.display_name)
            .collect();
        assert!(names.iter().any(|name| name == "Alice"));
        assert_eq!(
            followers(&bob_store, &bob_did).unwrap(),
            vec![alice_did.clone()]
        );
        let scan = scan_messages(&bob_store, &bob_copy.secret().unwrap()).unwrap();
        assert_eq!(scan.messages.len(), 1);
        assert_eq!(scan.messages[0].body, "hello over the internet");
        assert!(bob_store
            .private_by_route(&alice_only.route())
            .unwrap()
            .is_empty());

        let alice_store = Store::new(FsBackend::open(&alice_root).unwrap());
        let names: Vec<String> = known_profiles(&alice_store)
            .unwrap()
            .into_iter()
            .map(|profile| profile.display_name)
            .collect();
        assert!(names.iter().any(|name| name == "Bob"));

        std::fs::remove_dir_all(test_root).unwrap();
    }
}
