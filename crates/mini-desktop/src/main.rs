//! Windows-first Mininet reference client shell.
//!
//! This UI deliberately has no analytics, remote configuration, background
//! fetch, embedded browser, or update executor. Those are security properties
//! of the shell, not merely settings displayed to the user. Network and
//! protocol integration should be added behind explicit local interfaces.

#![forbid(unsafe_code)]

use did_mini::{Controller, Did};
use eframe::egui;
use mini_bearer::{Bearer, Initiator, Responder, TcpBearer};
use mini_media::publish_media;
use mini_objects::{ObjectBuilder, ObjectType, Payload};
use mini_social::{
    comments, community_members, feed, followers, following, publish_comment, publish_community,
    publish_profile, publish_wall, resolve_community, set_follow, set_membership, set_reaction,
    FeedFilter, FeedItem, MembershipMode, ReactionKind, VisibilityPolicy,
};
use mini_store::{FsBackend, Store};
use mini_sync::{kel_carrier, sync_bidirectional, KelCache, SyncRole};
use mini_windows_vault::{load_existing, load_or_create, load_user_data, save_user_data};
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Onboarding,
    Home,
    Communities,
    Creator,
    Connections,
    System,
    Privacy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdatePolicy {
    Ask,
    ManualOnly,
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
    profile_name: String,
    profile_bio: String,
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
    sync_rx: Option<Receiver<Result<String, String>>>,
    notice: String,
}

struct Workspace {
    store: Store<FsBackend>,
    identity: Option<Controller>,
    human: Option<Did>,
    root: PathBuf,
    sequence: u64,
}

impl Workspace {
    fn open() -> Result<Self, String> {
        let root = data_root();
        let store = Store::new(FsBackend::open(&root).map_err(|error| error.to_string())?);
        let identity_path = root.join("identity.dpapi");
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
        Ok(Self {
            store,
            // Open every session read-only. The DPAPI-protected signing
            // material is reconstructed only after the user presses
            // "Unlock identity". A new root is never created implicitly.
            identity: None,
            human,
            root,
            sequence: 1,
        })
    }

    fn is_unlocked(&self) -> bool {
        self.identity.is_some()
    }

    fn root_created(&self) -> bool {
        self.human.is_some()
    }

    fn has_public_account(&self) -> bool {
        self.store
            .by_type(&ObjectType::PROFILE)
            .map(|ids| !ids.is_empty())
            .unwrap_or(false)
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
        let seeds =
            load_or_create(&self.root.join("identity.dpapi")).map_err(|error| error.to_string())?;
        let identity = Controller::incept_single_from_seeds(&seeds.current, &seeds.next)
            .map_err(|error| error.to_string())?;
        self.human = Some(identity.did());
        self.identity = Some(identity);
        Ok(())
    }

    fn lock(&mut self) {
        self.identity = None;
    }

    fn unlock(&mut self) -> Result<(), String> {
        let seeds =
            load_existing(&self.root.join("identity.dpapi")).map_err(|error| error.to_string())?;
        let identity = Controller::incept_single_from_seeds(&seeds.current, &seeds.next)
            .map_err(|error| error.to_string())?;
        self.human = Some(identity.did());
        self.identity = Some(identity);
        Ok(())
    }

    fn publish_post(&mut self, text: &str) -> Result<(), String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| "identity is locked".to_string())?;
        let post = ObjectBuilder::new(ObjectType::POST)
            .timestamp_ms(now_ms())
            .sequence(self.sequence)
            .payload(Payload::Public(text.as_bytes().to_vec()))
            .sign(self.human_did()?, identity)
            .map_err(|error| error.to_string())?;
        self.store
            .insert(&post)
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
            identity,
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
            identity,
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
            identity,
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
            identity,
            content_type,
            &bytes,
            now_ms(),
            base_sequence,
        )
        .map_err(|error| error.to_string())?;
        self.sequence = base_sequence.saturating_add(manifest.chunks.len() as u64 + 1);
        let post = ObjectBuilder::new(ObjectType::POST)
            .timestamp_ms(now_ms())
            .sequence(self.sequence)
            .link("media", manifest.id)
            .payload(Payload::Public(caption.as_bytes().to_vec()))
            .sign(&human, identity)
            .map_err(|error| error.to_string())?;
        self.store
            .insert(&post)
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
            identity,
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
            identity,
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
            identity,
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
            identity,
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
        self.store
            .get(&item.id)
            .ok()
            .and_then(|object| match object.payload {
                Payload::Public(bytes) => String::from_utf8(bytes).ok(),
                Payload::Encrypted(_) => None,
            })
            .unwrap_or_else(|| "[unreadable or encrypted post]".to_string())
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

fn run_peer_sync(root: &std::path::Path, endpoint: &str, listener: bool) -> Result<String, String> {
    let seeds = load_or_create(&root.join("identity.dpapi")).map_err(|error| error.to_string())?;
    let identity = Controller::incept_single_from_seeds(&seeds.current, &seeds.next)
        .map_err(|error| error.to_string())?;
    let mut store = Store::new(FsBackend::open(root).map_err(|error| error.to_string())?);
    let carrier = kel_carrier(&identity.kel(), &identity.did(), &identity)
        .map_err(|error| error.to_string())?;
    store.insert(&carrier).map_err(|error| error.to_string())?;
    let mut cache = KelCache::new();
    cache.insert_verified(identity.kel());

    if listener {
        let listener = std::net::TcpListener::bind(endpoint).map_err(|error| error.to_string())?;
        let (stream, _) = listener.accept().map_err(|error| error.to_string())?;
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
            "Peer sync complete: received {}, accepted {}, invalid {}.",
            report.received, report.accepted, report.invalid
        ))
    } else {
        let address = endpoint
            .to_socket_addrs()
            .map_err(|error| error.to_string())?
            .next()
            .ok_or_else(|| "peer address did not resolve".to_string())?;
        let stream = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(10))
            .map_err(|error| error.to_string())?;
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
            "Peer sync complete: received {}, accepted {}, invalid {}.",
            report.received, report.accepted, report.invalid
        ))
    }
}

impl Default for MininetApp {
    fn default() -> Self {
        let workspace = Workspace::open().ok();
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
            profile_name: String::new(),
            profile_bio: String::new(),
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
            sync_rx: None,
            notice:
                "Local object store ready. Identity is locked; no network activity has started."
                    .to_string(),
        }
    }
}

impl eframe::App for MininetApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_theme(ctx);
        if let Some(result) = self
            .sync_rx
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok())
        {
            self.sync_rx = None;
            self.workspace = Workspace::open().ok();
            self.notice = match result {
                Ok(summary) => summary,
                Err(error) => format!("Peer sync failed: {error}"),
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
            .default_width(228.0)
            .show(ctx, |ui| {
                ui.add_space(12.0);
                ui.label(egui::RichText::new("YOUR NETWORK").small().strong());
                ui.add_space(6.0);
                self.nav_button(ui, View::Home, "Home");
                self.nav_button(ui, View::Communities, "Communities");
                self.nav_button(ui, View::Creator, "Creator studio");
                self.nav_button(ui, View::Connections, "Connections");
                self.nav_button(ui, View::System, "System & storage");
                ui.add_space(18.0);
                ui.label(egui::RichText::new("CONTROL PLANE").small().strong());
                ui.add_space(6.0);
                self.nav_button(ui, View::Privacy, "Privacy & safety");
                ui.separator();
                ui.label(
                    egui::RichText::new("No analytics\nNo ad SDKs\nNo embedded web view").small(),
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
                        View::Communities => self.communities(ui),
                        View::Creator => self.creator(ui),
                        View::Connections => self.connections(ui),
                        View::System => self.system(ui),
                        View::Privacy => self.privacy(ui),
                    }
                    ui.add_space(18.0);
                });
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(&self.notice).small());
                ui.separator();
                ui.label(egui::RichText::new("Updates: manual approval").small());
                ui.label(egui::RichText::new("No background sync").small());
            });
        });
    }
}

impl MininetApp {
    fn start_peer_sync(&mut self, listener: bool) {
        if self.sync_rx.is_some() {
            self.notice = "A peer sync is already running.".to_string();
            return;
        }
        let endpoint = if listener {
            format!("0.0.0.0:{}", self.listen_port.trim())
        } else {
            self.peer_address.trim().to_string()
        };
        let (sender, receiver) = mpsc::channel();
        self.sync_rx = Some(receiver);
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
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(65, 116, 154);
        visuals.selection.bg_fill = egui::Color32::from_rgb(42, 105, 145);
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

    fn nav_button(&mut self, ui: &mut egui::Ui, view: View, label: &str) {
        let selected = self.view == view;
        if ui
            .add_sized(
                [ui.available_width(), 38.0],
                egui::SelectableLabel::new(selected, label),
            )
            .clicked()
        {
            self.view = view;
        }
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
            View::Communities => (
                "Communities",
                "Portable spaces for discussion, not platform-owned silos.",
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
        ui.heading(title);
        ui.label(egui::RichText::new(subtitle).color(egui::Color32::LIGHT_GRAY));
    }

    fn onboarding(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
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
                                ui.group(|ui| {
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
                                ui.group(|ui| {
                                    ui.heading("2. Create your public account");
                                    ui.label("This profile is a signed public object. Your display name and bio are shared only when you later choose a transport path.");
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
                                            let result = workspace
                                                .unlock()
                                                .and_then(|()| {
                                                    workspace.publish_profile(
                                                        self.account_name.trim(),
                                                        self.account_bio.trim(),
                                                    )
                                                });
                                            match result {
                                                Ok(()) => {
                                                    workspace.lock();
                                                    self.signing_confirmation = false;
                                                    self.view = View::Home;
                                                    "Public account created locally. Identity is locked again.".to_string()
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

    fn home(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Create something").strong());
            ui.add_space(4.0);
            ui.add_sized(
                [ui.available_width(), 72.0],
                egui::TextEdit::multiline(&mut self.composer)
                    .hint_text("Write a post… (saved locally before sync)"),
            );
            ui.checkbox(
                &mut self.signing_confirmation,
                "I confirm this action will create a signed Mininet object",
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.workspace.as_ref().is_some_and(Workspace::is_unlocked),
                        egui::Button::new("Publish locally"),
                    )
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
            });
        });
        ui.add_space(14.0);
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
            ui.label("No local posts yet. Your first post stays on this device until you choose a connection path.");
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
                            workspace.post_text(item),
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
        for (id, body, reason, support_count, comment_count) in cards {
            self.post_card(
                ui,
                &id,
                "Local post",
                &body,
                reason,
                support_count,
                comment_count,
            );
        }
        if let Some(target) = self.reply_target.clone() {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Reply to selected post").strong());
                ui.text_edit_multiline(&mut self.reply_text);
                ui.checkbox(
                    &mut self.signing_confirmation,
                    "I confirm this action will create a signed reply",
                );
                if ui.button("Publish reply locally").clicked() {
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
                                "Reply written locally. No network used.".to_string()
                            }
                            Err(error) => format!("Could not publish reply: {error}"),
                        }
                    } else {
                        "Local workspace unavailable.".to_string()
                    };
                }
            });
        }
    }

    fn post_card(
        &mut self,
        ui: &mut egui::Ui,
        id: &mini_objects::ObjectId,
        title: &str,
        body: &str,
        reason: &str,
        support_count: usize,
        comment_count: usize,
    ) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("●").color(egui::Color32::from_rgb(100, 210, 160)));
                ui.label(egui::RichText::new(title).strong());
                ui.label(
                    egui::RichText::new("  2m")
                        .small()
                        .color(egui::Color32::GRAY),
                );
            });
            ui.label(body);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("Why here: {reason}")).small());
                ui.label(egui::RichText::new(format!("{support_count} likes")).small());
                ui.label(egui::RichText::new(format!("{comment_count} replies")).small());
                if ui.button("Reply").clicked() {
                    self.reply_target = Some(id.clone());
                }
                if ui.button("React").clicked() {
                    self.notice = if !self.signing_confirmation {
                        "Confirm signing before reacting.".to_string()
                    } else if let Some(workspace) = self.workspace.as_mut() {
                        match workspace.react_like(id) {
                            Ok(()) => {
                                self.signing_confirmation = false;
                                "Like written locally. No network used.".to_string()
                            }
                            Err(error) => format!("Could not react: {error}"),
                        }
                    } else {
                        "Local workspace unavailable.".to_string()
                    };
                }
            });
        });
        ui.add_space(8.0);
    }

    fn communities(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
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
            ui.group(|ui| {
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
        ui.group(|ui| {
            ui.label(egui::RichText::new("Profile identity").strong());
            ui.text_edit_singleline(&mut self.profile_name);
            ui.text_edit_singleline(&mut self.profile_bio);
            ui.checkbox(
                &mut self.signing_confirmation,
                "I confirm this action will create a signed profile object",
            );
            if ui.button("Publish profile locally").clicked() {
                self.notice = if self.profile_name.trim().is_empty() {
                    "A display name is required.".to_string()
                } else if !self.signing_confirmation {
                    "Confirm signing before publishing.".to_string()
                } else if let Some(workspace) = self.workspace.as_mut() {
                    match workspace
                        .publish_profile(self.profile_name.trim(), self.profile_bio.trim())
                    {
                        Ok(()) => {
                            self.signing_confirmation = false;
                            "Profile written locally. It has not been announced to a network."
                                .to_string()
                        }
                        Err(error) => format!("Could not publish profile: {error}"),
                    }
                } else {
                    "Local workspace unavailable.".to_string()
                };
            }
        });
        ui.add_space(12.0);
        ui.group(|ui| {
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
        ui.group(|ui| {
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
        ui.group(|ui| {
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
            ui.label("Each person uses a separate Mininet home and DID. Exchange DIDs through a trusted channel, then add the person in Creator studio.");
            if let Some(workspace) = self.workspace.as_ref() {
                if let Some(human) = workspace.human.as_ref() {
                    ui.label(format!("Your DID: {human}"));
                }
            }
            if ui.button("Open friend manager").clicked() {
                self.view = View::Creator;
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
                    self.start_peer_sync(false);
                }
                if ui.button("Listen once").clicked() {
                    self.start_peer_sync(true);
                }
            });
            if self.sync_rx.is_some() {
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
            });
        });
        ui.add_space(10.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("Protocol coverage").strong());
            ui.label("Integrated here: signed social objects, local feed assembly, communities, threaded replies, reactions, chunked media, DPAPI identity storage, offline bundles, and encrypted one-shot TCP sync.");
            ui.label("Available in the repository but not yet a finished desktop workflow: forge repository/PR operations, presence/keystone encounters, reward accounting, privacy-cost routing, update adoption, and governance administration.");
            ui.label("Those foundations are deliberately shown as boundaries rather than unsafe pretend buttons. Their object types remain inspectable and syncable when another Mininet tool creates them.");
        });
        ui.add_space(10.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("Production readiness").strong());
            for (feature, status, owner) in [
                ("Local social, profiles, follows, walls, communities", "Integrated / test-covered", "Desktop"),
                ("Offline bundles and manual encrypted TCP sync", "Integrated / operator-configured", "Desktop + networking"),
                ("Internet relay and NAT traversal", "Partial: self-hosted relay foundation exists", "Networking"),
                ("Forge repositories, pull requests, releases", "Protocol foundation; desktop workflow missing", "Forge UI"),
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
    use super::{decode_privacy_settings, encode_privacy_settings, PrivacyState, UpdatePolicy};

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
}
