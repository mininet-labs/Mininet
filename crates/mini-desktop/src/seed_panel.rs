//! Typed owner controls for the external seed CLI. Opening this view runs no
//! command; all operations use bounded subprocess workers, not desktop keys.
use crate::seed_runtime::{Action, Job, Node};
use eframe::egui;
use serde_json::Value;
use std::path::PathBuf;

struct Pending {
    label: &'static str,
    job: Job,
    preview_key: Option<PreviewKey>,
    pairing_key: Option<PairingKey>,
}
#[derive(Clone, PartialEq, Eq)]
struct PreviewKey {
    node: Node,
    root: String,
    kind: String,
    days: u16,
    output: PathBuf,
}
struct Preview {
    key: PreviewKey,
    id: String,
    document: Value,
}

#[derive(Clone, PartialEq, Eq)]
struct PairingKey {
    node: Node,
    hex: String,
}
struct PairingReview {
    key: PairingKey,
    id: String,
    human: String,
    device: String,
}
fn pairing_review(key: PairingKey, envelope: &Value) -> Option<PairingReview> {
    let result = envelope.get("result")?;
    let id = result.get("bundle_id")?.as_str()?;
    let human = result.get("human_did")?.as_str()?;
    let device = result.get("device_did")?.as_str()?;
    if result.get("trusted")?.as_bool()?
        || id.len() != 64
        || !id.bytes().all(|b| b.is_ascii_hexdigit())
        || human.is_empty()
        || device.is_empty()
        || human == device
    {
        return None;
    }
    Some(PairingReview {
        key,
        id: id.into(),
        human: human.into(),
        device: device.into(),
    })
}

pub(crate) struct Panel {
    initialize_ack: bool,
    initialize_confirmation: String,
    pairing_hex: String,
    pairing_confirmation: String,
    pairing: Option<PairingReview>,
    program: String,
    home: String,
    store: String,
    primary: Option<Pending>,
    service: Option<Pending>,
    follow: Option<Pending>,
    control: Option<Pending>,
    status: String,
    report: Option<(&'static str, Value)>,
    preview: Option<Preview>,
    confirmation: String,
    quota: u64,
    contact_name: String,
    contact_address: String,
    contact_peer: String,
    contact_filter: String,
    announce_address: String,
    announcement_minutes: u32,
    allow_announcement: bool,
    query: String,
    after: String,
    search_limit: u16,
    object_id: String,
    reseed: bool,
    collection_root: String,
    collection_kind: String,
    days: u16,
    preview_output: String,
    fetch_objects: u16,
    fetch_bytes: u64,
    fetch_downloads: u16,
    export_output: String,
    service_address: String,
    allowed_peers: String,
    allow_service: bool,
    follow_rounds: u32,
    follow_interval: u64,
    follow_objects: u16,
    follow_resume: bool,
}
impl Default for Panel {
    fn default() -> Self {
        let home = crate::resident::config_home();
        let program = std::env::current_exe()
            .ok()
            .and_then(|p| {
                p.parent()
                    .map(|p| p.join(if cfg!(windows) { "mini.exe" } else { "mini" }))
            })
            .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "mini.exe" } else { "mini" }));
        Self {
            initialize_ack: false,
            initialize_confirmation: String::new(),
            pairing_hex: String::new(),
            pairing_confirmation: String::new(),
            pairing: None,
            program: program.display().to_string(),
            home: home.display().to_string(),
            store: home.join("store").display().to_string(),
            primary: None,
            service: None,
            follow: None,
            control: None,
            status:
                "Choose an existing CLI node profile. No command or network session has started."
                    .into(),
            report: None,
            preview: None,
            confirmation: String::new(),
            quota: 1024 * 1024 * 1024,
            contact_name: String::new(),
            contact_address: String::new(),
            contact_peer: String::new(),
            contact_filter: String::new(),
            announce_address: String::new(),
            announcement_minutes: 15,
            allow_announcement: false,
            query: String::new(),
            after: String::new(),
            search_limit: 8,
            object_id: String::new(),
            reseed: false,
            collection_root: String::new(),
            collection_kind: "media".into(),
            days: 7,
            preview_output: String::new(),
            fetch_objects: 128,
            fetch_bytes: 64 * 1024 * 1024,
            fetch_downloads: 16,
            export_output: String::new(),
            service_address: "127.0.0.1:7400".into(),
            allowed_peers: String::new(),
            allow_service: false,
            follow_rounds: 10,
            follow_interval: 5000,
            follow_objects: 128,
            follow_resume: true,
        }
    }
}
fn optional(s: &str) -> Option<String> {
    (!s.trim().is_empty()).then(|| s.trim().to_owned())
}
fn field(ui: &mut egui::Ui, label: &str, text: &mut String) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(
            egui::TextEdit::singleline(text)
                .desired_width(430.0)
                .char_limit(4096),
        )
        .changed()
    })
    .inner
}
fn number<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).range(range)).changed()
    })
    .inner
}
fn preview_matches(key: &PreviewKey, id: &str, document: &Value) -> bool {
    let plan = &document["plan"];
    document["id"].as_str() == Some(id)
        && plan["root"].as_str() == Some(key.root.as_str())
        && plan["kind"].as_str() == Some(key.kind.as_str())
        && plan["days"].as_u64() == Some(key.days as u64)
        && plan["objects"].as_array().is_some_and(|objects| {
            !objects.is_empty()
                && objects.len() <= 4096
                && objects.iter().all(|entry| {
                    entry["id"].as_str().is_some_and(|id| !id.is_empty())
                        && entry["bytes"].as_u64().is_some()
                        && entry["object_type"].is_string()
                })
        })
}

impl Panel {
    fn node(&self) -> Node {
        Node {
            program: PathBuf::from(self.program.trim()),
            home: PathBuf::from(self.home.trim()),
            store: PathBuf::from(self.store.trim()),
        }
    }
    fn key(&self) -> PreviewKey {
        PreviewKey {
            node: self.node(),
            root: self.collection_root.trim().into(),
            kind: self.collection_kind.clone(),
            days: self.days,
            output: PathBuf::from(self.preview_output.trim()),
        }
    }
    fn pairing_key(&self) -> PairingKey {
        PairingKey {
            node: self.node(),
            hex: self.pairing_hex.trim().into(),
        }
    }
    fn invalidate_pairing(&mut self) {
        self.pairing = None;
        self.pairing_confirmation.clear();
    }
    fn invalidate(&mut self) {
        self.initialize_ack = false;
        self.initialize_confirmation.clear();
        self.pairing_hex.clear();
        self.invalidate_pairing();
        for slot in [
            &mut self.primary,
            &mut self.service,
            &mut self.follow,
            &mut self.control,
        ] {
            if let Some(p) = slot.take() {
                p.job.cancel();
            }
        }
        self.preview = None;
        self.confirmation.clear();
        self.report = None;
        // Result-derived selectors and profile-specific grants must not carry
        // over to a different node, even though every action needs a click.
        self.object_id.clear();
        self.after.clear();
        self.contact_filter.clear();
        self.contact_name.clear();
        self.contact_address.clear();
        self.contact_peer.clear();
        self.announce_address.clear();
        self.allowed_peers.clear();
        self.collection_root.clear();
        self.preview_output.clear();
        self.export_output.clear();
        self.service_address = "127.0.0.1:7400".into();
        self.reseed = false;
        self.allow_service = false;
        self.allow_announcement = false;
        self.status = "Node paths changed. Previous jobs cancelled and review results discarded. Run a fresh status or preview.".into();
    }
    fn start(
        &mut self,
        action: Action,
        label: &'static str,
        lane: u8,
        preview_key: Option<PreviewKey>,
    ) {
        let pairing_key = matches!(action, Action::KelInspect { .. }).then(|| self.pairing_key());
        let slot = match lane {
            1 => &mut self.service,
            2 => &mut self.follow,
            3 => &mut self.control,
            _ => &mut self.primary,
        };
        if slot.is_some() {
            self.status =
                "An operation in this lane is still running. Wait or cancel it first.".into();
            return;
        }
        let node = Node {
            program: PathBuf::from(self.program.trim()),
            home: PathBuf::from(self.home.trim()),
            store: PathBuf::from(self.store.trim()),
        };
        match Job::start(node, action) {
            Ok(job) => {
                *slot = Some(Pending {
                    label,
                    job,
                    preview_key,
                    pairing_key,
                });
                self.status = format!("{label} started for the selected CLI profile.");
            }
            Err(error) => self.status = error,
        }
    }
    fn poll(&mut self) {
        let current = self.key();
        let current_pairing = self.pairing_key();
        for slot in [
            &mut self.primary,
            &mut self.service,
            &mut self.follow,
            &mut self.control,
        ] {
            let result = slot.as_mut().and_then(|p| p.job.poll());
            if let Some(result) = result {
                let pending = slot.take().expect("completed seed job has state");
                match result {
                    Err(error) => self.status = format!("{} failed: {error}", pending.label),
                    Ok(value) => {
                        if let Some(key) = pending.pairing_key {
                            self.pairing = None;
                            self.pairing_confirmation.clear();
                            if key == current_pairing {
                                self.pairing = pairing_review(key, &value);
                            }
                            self.status = if self.pairing.is_some() {
                                "Public keys verified. Compare the full human/device DIDs with the peer over a trusted channel, then type the exact bundle ID to trust them locally.".into()
                            } else {
                                "Pairing review does not match the current profile and bundle. Inspect again.".into()
                            };
                        } else if let Some(key) = pending.preview_key {
                            self.preview = None;
                            self.confirmation.clear();
                            let result = value.get("result").unwrap_or(&value);
                            let id = result.get("preview_id").and_then(Value::as_str);
                            let doc = result.get("preview_document");
                            if key == current {
                                if let (Some(id), Some(document)) = (id, doc) {
                                    if preview_matches(&key, id, document) {
                                        self.preview = Some(Preview {
                                            key,
                                            id: id.into(),
                                            document: document.clone(),
                                        });
                                    }
                                }
                            }
                            if self.preview.is_none() {
                                self.status = "Preview could not be bound to the current form and full document. Apply is disabled; create a fresh preview.".into();
                            } else {
                                self.status = "Preview ready. Review every listed object and retention, then type its exact ID to share.".into();
                            }
                        } else {
                            self.status = format!(
                                "{} finished. Inspect its reported outcome below.",
                                pending.label
                            );
                        }
                        self.report = Some((pending.label, value));
                    }
                }
            }
        }
    }
    pub(crate) fn ui(&mut self, ui: &mut egui::Ui) {
        self.poll();
        ui.label("Select a mini CLI node profile, or explicitly create a separate new profile below. Its signing identity and trust settings are separate from this desktop's protected vault. This panel never creates a wallet or starts networking on its own.");
        egui::CollapsingHeader::new("Node profile and executable")
            .default_open(true)
            .show(ui, |ui| {
                let mut changed = field(ui, "mini executable", &mut self.program);
                changed |= field(ui, "CLI node home", &mut self.home);
                changed |= field(ui, "Object store", &mut self.store);
                if changed {
                    self.invalidate();
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.primary.is_none(),
                            egui::Button::new("Inspect node identity"),
                        )
                        .clicked()
                    {
                        self.start(Action::NodeIdentity, "Node identity", 0, None);
                    }
                    if ui
                        .add_enabled(self.primary.is_none(), egui::Button::new("Pool status"))
                        .clicked()
                    {
                        self.start(Action::Status, "Pool status", 0, None);
                    }
                    if ui
                        .add_enabled(self.primary.is_none(), egui::Button::new("Inventory"))
                        .clicked()
                    {
                        self.start(Action::Inventory, "Inventory", 0, None);
                    }
                });
            });
        ui.label(&self.status);
        ui.horizontal_wrapped(|ui| {
            for (name, slot) in [
                ("Operation", &self.primary),
                ("Seed host", &self.service),
                ("Mesh follow", &self.follow),
                ("Stop control", &self.control),
            ] {
                if let Some(pending) = slot {
                    ui.label(format!(
                        "{name}: {}{}",
                        pending.label,
                        if pending.job.is_finished() {
                            " (finishing)"
                        } else {
                            ""
                        }
                    ));
                    if ui.button(format!("Cancel {name}")).clicked() {
                        pending.job.cancel();
                    }
                }
            }
        });
        let idle = self.primary.is_none();
        egui::CollapsingHeader::new("Owner quota and retention").show(ui, |ui| {
            number(ui, "Maximum pool bytes", &mut self.quota, 0..=1024 * 1024 * 1024);
            ui.horizontal(|ui| {
                if ui.add_enabled(idle, egui::Button::new("Save quota")).clicked() { self.start(Action::Configure { max_bytes: self.quota }, "Configure quota", 0, None); }
                if ui.add_enabled(idle, egui::Button::new("Prune expired leases")).clicked() { self.start(Action::Prune, "Prune expired leases", 0, None); }
                if ui.add_enabled(idle && !self.object_id.trim().is_empty(), egui::Button::new("Revoke selected object")).clicked() { self.start(Action::Revoke { id: self.object_id.trim().into() }, "Revoke object", 0, None); }
            });
            ui.small("Quota is 0–1 GiB and must cover retained bytes. Retention is finite; collection sharing below requires an exact preview confirmation.");
        });
        egui::CollapsingHeader::new("Seed contacts and signed catalogs").show(ui, |ui| {
            field(ui, "Contact name", &mut self.contact_name); field(ui, "Numeric IP:port", &mut self.contact_address); field(ui, "Expected root DID", &mut self.contact_peer);
            ui.small("Adding an endpoint does not grant identity trust. The CLI profile must already trust the peer's KEL.");
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(idle, egui::Button::new("List contacts")).clicked() { self.start(Action::ContactList, "Contacts", 0, None); }
                if ui.add_enabled(idle && !self.contact_name.trim().is_empty() && !self.contact_address.trim().is_empty() && !self.contact_peer.trim().is_empty(), egui::Button::new("Add contact")).clicked() { self.start(Action::ContactAdd { name: self.contact_name.trim().into(), address: self.contact_address.trim().into(), peer: self.contact_peer.trim().into() }, "Add contact", 0, None); }
                if ui.add_enabled(idle && !self.contact_name.trim().is_empty(), egui::Button::new("Remove named contact")).clicked() { self.start(Action::ContactRemove { name: self.contact_name.trim().into() }, "Remove contact", 0, None); }
            });
            field(ui, "Contact filter (blank = activated contacts)", &mut self.contact_filter);
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(idle, egui::Button::new("List cached catalogs")).clicked() { self.start(Action::CatalogList, "Catalogs", 0, None); }
                if ui.add_enabled(idle, egui::Button::new("Refresh contacts")).clicked() { self.start(Action::CatalogRefresh { contact: optional(&self.contact_filter) }, "Refresh catalogs", 0, None); }
                if ui.add_enabled(idle, egui::Button::new("Pull transitive catalog hints")).clicked() { self.start(Action::CatalogPull { contact: optional(&self.contact_filter) }, "Pull catalog hints", 0, None); }
            });
            ui.small("Network requests run only when clicked. Catalog availability claims are hints; retrieval verifies the actual signed objects.");
        });
        egui::CollapsingHeader::new("Advertise this seed node").show(ui, |ui| {
            field(ui, "Advertised numeric endpoint", &mut self.announce_address);
            number(ui, "Announcement lifetime (minutes)", &mut self.announcement_minutes, 1..=15);
            ui.checkbox(&mut self.allow_announcement, "Advertise this endpoint and the pool's selected public object IDs to allowed peers");
            ui.horizontal(|ui| {
                if ui.add_enabled(idle && self.allow_announcement && !self.announce_address.trim().is_empty(), egui::Button::new("Create signed announcement")).clicked() { self.start(Action::Announce { address: self.announce_address.trim().into(), ttl_ms: self.announcement_minutes as u64 * 60_000 }, "Announce availability", 0, None); self.allow_announcement = false; }
                if ui.add_enabled(idle, egui::Button::new("Withdraw announcement")).clicked() { self.start(Action::Unannounce, "Withdraw announcement", 0, None); }
            });
        });
        egui::CollapsingHeader::new("Search and retrieve signed public content").default_open(true).show(ui, |ui| {
            field(ui, "Literal query", &mut self.query); field(ui, "Continue after object ID (optional)", &mut self.after);
            number(ui, "Hits per contact", &mut self.search_limit, 1..=8);
            if ui.add_enabled(idle && !self.query.trim().is_empty(), egui::Button::new("Search selected contacts")).clicked() { self.start(Action::Search { query: self.query.trim().into(), contact: optional(&self.contact_filter), limit: self.search_limit, after: optional(&self.after) }, "Signed search", 0, None); }
            ui.small("The query is disclosed to the selected contacts. Results cover received verified hits, not a complete index.");
            field(ui, "Selected object ID", &mut self.object_id);
            ui.checkbox(&mut self.reseed, "After successful retrieval, explicitly reseed this object for the retention below");
            ui.horizontal(|ui| {
                if ui.add_enabled(idle && !self.object_id.trim().is_empty(), egui::Button::new("Locate providers")).clicked() { self.start(Action::Locate { id: self.object_id.trim().into() }, "Locate providers", 0, None); }
                if ui.add_enabled(idle && !self.object_id.trim().is_empty(), egui::Button::new("Retrieve verified object")).clicked() { self.start(Action::Retrieve { id: self.object_id.trim().into(), reseed_days: self.reseed.then_some(self.days) }, "Retrieve object", 0, None); self.reseed = false; }
            });
        });
        self.onboarding(ui, idle);
        self.collections(ui, idle);
        self.hosting(ui, idle);
        if let Some((label, value)) = &self.report {
            ui.separator();
            ui.heading(*label);
            result_view(
                ui,
                value,
                &mut self.object_id,
                &mut self.after,
                &mut self.contact_filter,
            );
        }
        if self.primary.is_some()
            || self.service.is_some()
            || self.follow.is_some()
            || self.control.is_some()
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    fn onboarding(&mut self, ui: &mut egui::Ui, idle: bool) {
        egui::CollapsingHeader::new("Create node identity and pair public keys").show(ui, |ui| {
            ui.label("Creation writes a new human/device signing identity in the selected CLI home. It never imports the desktop vault identity or overwrites an existing identity. Keep this folder private: CLI seed files grant control, and this developer profile has no OS-keychain protection or key rotation.");
            ui.checkbox(&mut self.initialize_ack, "I want a separate CLI identity at the node home shown above");
            field(ui, "Type CREATE to create signing keys", &mut self.initialize_confirmation);
            let no_jobs = idle && self.service.is_none() && self.follow.is_none() && self.control.is_none();
            if ui.add_enabled(no_jobs && self.initialize_ack && self.initialize_confirmation == "CREATE", egui::Button::new("Create new node identity")).clicked() {
                self.initialize_ack = false; self.initialize_confirmation.clear();
                self.invalidate_pairing();
                self.start(Action::NodeInitialize, "Create node identity", 0, None);
            }
            ui.separator();
            ui.label("Exchange only the public key bundle. Never paste or send human.seed/device.seed. Exporting public keys grants no endpoint access; local trust and seed contact activation are separate choices.");
            if ui.add_enabled(idle, egui::Button::new("Export my public key bundle")).clicked() { self.start(Action::KelExport, "Public key export", 0, None); }
            ui.label("Peer public bundle (hex, up to 8192 characters)");
            let changed = ui.add(egui::TextEdit::multiline(&mut self.pairing_hex).desired_rows(3).char_limit(8192)).changed();
            if changed { self.invalidate_pairing(); }
            if ui.add_enabled(idle && !self.pairing_hex.trim().is_empty(), egui::Button::new("Inspect peer public keys — no trust granted")).clicked() {
                self.invalidate_pairing();
                self.start(Action::KelInspect { hex:self.pairing_hex.trim().into() }, "Inspect peer public keys", 0, None);
            }
            let mut trust = None;
            if let Some(review) = &self.pairing {
                ui.label(format!("Human root: {}", review.human));
                ui.label(format!("Device: {}", review.device));
                ui.label(format!("Exact bundle ID: {}", review.id));
                ui.label("Cryptographic verification proves key consistency, not who the person is. Compare these DIDs independently before trusting. Trust permits verification of that identity's signed objects; it grants no governance vote, payment authority or network access.");
                field(ui, "Type the exact bundle ID to trust these public keys", &mut self.pairing_confirmation);
                if ui.add_enabled(no_jobs && review.key == self.pairing_key() && self.pairing_confirmation == review.id, egui::Button::new("Trust reviewed peer keys locally")).clicked() { trust = Some((review.key.hex.clone(), review.id.clone())); }
            }
            if let Some((hex, confirmation)) = trust {
                self.invalidate_pairing();
                self.start(Action::KelTrust { hex, confirmation }, "Trust reviewed peer keys", 0, None);
            }
        });
    }
    fn collections(&mut self, ui: &mut egui::Ui, idle: bool) {
        egui::CollapsingHeader::new("Linked media and history collections").show(ui, |ui| {
            let mut changed = field(ui, "Collection root ID", &mut self.collection_root);
            ui.horizontal(|ui| { ui.label("Collection kind"); egui::ComboBox::from_id_salt("seed-collection-kind").selected_text(&self.collection_kind).show_ui(ui, |ui| { for kind in ["media", "history", "links"] { changed |= ui.selectable_value(&mut self.collection_kind, kind.into(), kind).changed(); } }); });
            changed |= number(ui, "Retention (days)", &mut self.days, 1..=3650);
            changed |= field(ui, "New preview output file", &mut self.preview_output);
            if changed { self.preview = None; self.confirmation.clear(); }
            if ui.add_enabled(idle && !self.collection_root.trim().is_empty() && !self.preview_output.trim().is_empty(), egui::Button::new("Create collection preview")).clicked() {
                let key = self.key();
                self.preview = None; self.confirmation.clear();
                self.start(Action::CollectionPreview { root: key.root.clone(), kind: key.kind.clone(), days: key.days, output: key.output.clone() }, "Collection preview", 0, Some(key));
            }
            let mut apply = None;
            if let Some(preview) = &self.preview {
                ui.label(format!("Exact preview: {}", preview.id));
                ui.label("Apply shares every public object listed below and resets its retention from apply time. It does not modify source files.");
                let plan = preview.document.get("plan").unwrap_or(&preview.document);
                collection_plan(ui, plan);
                field(ui, "Type the exact preview ID to confirm sharing", &mut self.confirmation);
                if ui.add_enabled(idle && preview.key == self.key() && self.confirmation == preview.id, egui::Button::new("Apply reviewed public seed collection")).clicked() { apply = Some((preview.key.output.clone(), preview.id.clone())); }
            }
            if let Some((preview, confirmation)) = apply {
                self.preview = None; self.confirmation.clear();
                self.start(Action::CollectionApply { preview, confirmation }, "Apply collection", 0, None);
            }
            ui.separator();
            ui.label("Fetch a bounded window into the ordinary object store. Fetching alone does not share or execute it.");
            number(ui, "Maximum graph objects", &mut self.fetch_objects, 1..=4096);
            number(ui, "Maximum received bytes", &mut self.fetch_bytes, 1..=1024 * 1024 * 1024);
            number(ui, "Maximum downloads this window", &mut self.fetch_downloads, 1..=4096);
            if ui.add_enabled(idle && !self.collection_root.trim().is_empty(), egui::Button::new("Fetch / resume collection window")).clicked() { self.start(Action::CollectionFetch { root: self.collection_root.trim().into(), kind: self.collection_kind.clone(), max_objects: self.fetch_objects, max_bytes: self.fetch_bytes, max_downloads: self.fetch_downloads }, "Fetch collection window", 0, None); }
            field(ui, "New media export file", &mut self.export_output);
            if ui.add_enabled(idle && !self.collection_root.trim().is_empty() && !self.export_output.trim().is_empty(), egui::Button::new("Export verified media to new file")).clicked() { self.start(Action::MediaExport { root: self.collection_root.trim().into(), output: PathBuf::from(self.export_output.trim()) }, "Export media", 0, None); }
        });
    }
    fn hosting(&mut self, ui: &mut egui::Ui, idle: bool) {
        egui::CollapsingHeader::new("Owner-started seed hosting and follow").show(ui, |ui| {
            field(ui, "Listen numeric IP:port", &mut self.service_address); field(ui, "Allowed root DIDs (comma separated)", &mut self.allowed_peers);
            ui.checkbox(&mut self.allow_service, "Serve the selected public seed pool to these explicitly allowed peers");
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(self.service.is_none() && self.allow_service && !self.allowed_peers.trim().is_empty(), egui::Button::new("Start seed host")).clicked() { self.start(Action::ServiceRun { address: self.service_address.trim().into(), allowed_peers: self.allowed_peers.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect() }, "Seed host", 1, None); self.allow_service = false; }
                if ui.add_enabled(idle, egui::Button::new("Host status")).clicked() { self.start(Action::ServiceStatus, "Host status", 0, None); }
                if ui.add_enabled(self.control.is_none(), egui::Button::new("Request host stop")).clicked() { self.start(Action::ServiceStop, "Stop seed host", 3, None); }
            });
            ui.small("Stop requests are cooperative: an in-flight bounded transfer may finish. Cancelling a panel job terminates only the child process this panel started.");
            ui.small("Hosting is a separate CLI process. Its listening state is only confirmed by Host status; creating an announcement does not start a listener.");
            ui.separator();
            number(ui, "Follow rounds", &mut self.follow_rounds, 1..=86400);
            number(ui, "Interval milliseconds", &mut self.follow_interval, 1000..=60000);
            number(ui, "Follow object limit", &mut self.follow_objects, 1..=4096);
            ui.checkbox(&mut self.follow_resume, "Resume the previous bounded follow checkpoint");
            ui.label(format!("Follow explicitly retains fetched public objects for {} days, using the activated seed contacts.", self.days));
            ui.horizontal(|ui| {
                if ui.add_enabled(self.follow.is_none(), egui::Button::new("Start bounded mesh follow")).clicked() { self.start(Action::Follow { days: self.days, rounds: self.follow_rounds, interval_ms: self.follow_interval, max_objects: self.follow_objects, resume: self.follow_resume }, "Mesh follow", 2, None); }
                if ui.add_enabled(self.control.is_none(), egui::Button::new("Request follow stop")).clicked() { self.start(Action::FollowStop, "Stop mesh follow", 3, None); }
            });
        });
    }
}
fn collection_plan(ui: &mut egui::Ui, plan: &Value) {
    for (key, label) in [
        ("root", "Root"),
        ("kind", "Kind"),
        ("days", "Days"),
        ("owner", "Owner root"),
        ("home", "Node home"),
        ("source", "Source store"),
        ("total_bytes", "Stored bytes"),
        ("reconstructed_bytes", "Reconstructed bytes"),
    ] {
        if let Some(value) = plan.get(key) {
            ui.label(format!("{label}: {}", scalar(value)));
        }
    }
    if let Some(objects) = plan.get("objects").and_then(Value::as_array) {
        ui.label(format!(
            "{} objects — review the entire list",
            objects.len()
        ));
        egui::ScrollArea::vertical()
            .id_salt("seed-preview-objects")
            .max_height(240.0)
            .show(ui, |ui| {
                for object in objects {
                    ui.monospace(format!(
                        "{} | {} bytes | {}",
                        scalar(&object["id"]),
                        scalar(&object["bytes"]),
                        scalar(&object["object_type"])
                    ));
                }
            });
    }
}
fn scalar(value: &Value) -> String {
    match value {
        Value::String(s) => s.chars().take(2048).collect(),
        Value::Null => "—".into(),
        _ => value.to_string(),
    }
}
fn result_view(
    ui: &mut egui::Ui,
    envelope: &Value,
    object_id: &mut String,
    after: &mut String,
    contact: &mut String,
) {
    let value = envelope.get("result").unwrap_or(envelope);
    for (key, label) in [
        ("text", "Identity"),
        ("human_did", "Human root"),
        ("device_did", "Device"),
        ("bundle_id", "Public bundle ID"),
        ("trusted", "Locally trusted"),
        ("created", "Created"),
        ("complete", "Complete"),
        ("running", "Running"),
        ("active", "Active leases"),
        ("retrieved", "Retrieved"),
        ("seeded", "Seeded"),
        ("downloaded", "Downloaded"),
        ("reused", "Reused"),
        ("objects", "Objects"),
        ("bytes", "Bytes"),
        ("output", "Output file"),
        ("stop_requested", "Stop requested"),
        ("note", "Note"),
        ("error", "Reported error"),
        ("reason", "Reason"),
    ] {
        if let Some(v) = value.get(key).filter(|v| !v.is_array() && !v.is_object()) {
            ui.label(format!("{label}: {}", scalar(v)));
        }
    }
    if let Some(hex) = value.get("hex").and_then(Value::as_str) {
        ui.label("Public key bundle (contains no signing seeds)");
        let mut displayed = hex.to_owned();
        ui.add(
            egui::TextEdit::multiline(&mut displayed)
                .desired_rows(3)
                .interactive(false),
        );
        if ui.button("Copy public key bundle").clicked() {
            ui.ctx().copy_text(hex.to_owned());
        }
    }
    for key in ["pool", "price"] {
        if let Some(v) = value.get(key).and_then(Value::as_object) {
            for (key, value) in v {
                if !value.is_object() && !value.is_array() {
                    ui.label(format!("{key}: {}", scalar(value)));
                }
            }
        }
    }
    if value.get("complete").and_then(Value::as_bool) == Some(false) {
        ui.label("This bounded operation is incomplete. Review its continuation or errors before repeating the same window.");
    }
    if let Some(contacts) = value.get("contacts").and_then(Value::as_object) {
        for (name, c) in contacts {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{name}: {} — {}",
                    scalar(&c["address"]),
                    scalar(&c["peer"])
                ));
                if ui.button("Use contact filter").clicked() {
                    *contact = name.clone();
                }
            });
        }
    }
    for key in [
        "items",
        "results",
        "providers",
        "candidates",
        "reports",
        "rejected",
        "catalogs",
        "failures",
        "continuations",
        "refreshed",
    ] {
        if let Some(rows) = value.get(key).and_then(Value::as_array) {
            ui.strong(format!("{key}: {}", rows.len()));
            for (index, row) in rows.iter().enumerate().take(512) {
                ui.push_id((key, index), |ui| {
                    if key == "results" {
                        ui.label(format!(
                            "{} — author {} — verified {}",
                            scalar(&row["object_id"]),
                            scalar(&row["author"]),
                            scalar(&row["content_verified"])
                        ));
                        if let Some(excerpt) = row.get("excerpt").and_then(Value::as_str) {
                            ui.label(excerpt);
                        }
                    } else if key == "items" {
                        ui.label(format!(
                            "{} — {} bytes — expires at {} ms UTC",
                            scalar(&row["id"]),
                            scalar(&row["bytes"]),
                            scalar(&row["expires_at_ms"])
                        ));
                    } else if key == "candidates" {
                        ui.label(format!(
                            "{} — provider {} at {}",
                            scalar(&row["contact"]),
                            scalar(&row["provider"]),
                            scalar(&row["active_endpoint"])
                        ));
                    } else if key == "providers" {
                        ui.label(format!(
                            "Provider {} — endpoint {} — expires at {} ms UTC",
                            scalar(&row["provider"]),
                            scalar(&row["address"]),
                            scalar(&row["expires_at_ms"])
                        ));
                        if let Some(entries) = row.get("entries").and_then(Value::as_array) {
                            ui.small(format!(
                                "{} advertised object entries; actual availability is unproven",
                                entries.len()
                            ));
                        }
                    } else if key == "failures" || key == "rejected" {
                        ui.label(format!(
                            "{}: {}",
                            scalar(&row["contact"]),
                            scalar(&row["error"])
                        ));
                    } else if key == "continuations" {
                        ui.label(format!(
                            "Continue {} after {}",
                            scalar(&row["contact"]),
                            scalar(&row["after"])
                        ));
                    } else {
                        ui.monospace(scalar(row));
                    }
                    let id = row
                        .get("object_id")
                        .or_else(|| row.get("id"))
                        .and_then(Value::as_str);
                    if let Some(id) = id {
                        if ui.button("Select object ID").clicked() {
                            *object_id = id.into();
                        }
                    }
                    if let Some(next) = row.get("after").and_then(Value::as_str) {
                        if ui.button("Use continuation").clicked() {
                            *after = next.into();
                            if let Some(name) = row.get("contact").and_then(Value::as_str) {
                                *contact = name.into();
                            }
                        }
                    }
                });
            }
            if rows.len() > 512 {
                ui.label(
                    "Showing the first 512 rows; inspect the bounded raw result for the rest.",
                );
            }
        }
    }
    egui::CollapsingHeader::new("Command details — JSON displayed as untrusted text").show(
        ui,
        |ui| {
            if let Ok(text) = serde_json::to_string_pretty(envelope) {
                let mut text = text.as_str();
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut text)
                                .code_editor()
                                .interactive(false)
                                .desired_width(f32::INFINITY),
                        );
                    });
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collection_review_requires_full_exact_plan_not_only_a_matching_id() {
        let key = PreviewKey {
            node: Node {
                program: "mini".into(),
                home: "node".into(),
                store: "store".into(),
            },
            root: "root".into(),
            kind: "media".into(),
            days: 7,
            output: "preview.json".into(),
        };
        let doc = serde_json::json!({"id":"review-id", "plan":{"root":"root","kind":"media","days":7,"objects":[{"id":"object","bytes":16,"object_type":"Media"}]}});
        assert!(preview_matches(&key, "review-id", &doc));
        let mut changed = doc.clone();
        changed["plan"]["days"] = 8.into();
        assert!(!preview_matches(&key, "review-id", &changed));
        assert!(!preview_matches(&key, "wrong-id", &doc));
        assert!(!preview_matches(
            &key,
            "review-id",
            &serde_json::json!({"id":"review-id"})
        ));
        let mut changed = doc;
        changed["plan"]["objects"] = serde_json::json!([]);
        assert!(!preview_matches(&key, "review-id", &changed));
    }
    #[test]
    fn changing_node_profile_drops_result_selectors_and_public_grants() {
        let mut panel = Panel {
            object_id: "old-selected-object".into(),
            after: "old-continuation".into(),
            contact_filter: "old-contact".into(),
            allowed_peers: "old-peer".into(),
            allow_service: true,
            allow_announcement: true,
            reseed: true,
            ..Panel::default()
        };
        panel.invalidate();
        assert!(
            panel.object_id.is_empty() && panel.after.is_empty() && panel.contact_filter.is_empty()
        );
        assert!(panel.allowed_peers.is_empty());
        assert!(!panel.allow_service && !panel.allow_announcement && !panel.reseed);
        assert!(panel.report.is_none() && panel.preview.is_none());
    }
    #[test]
    fn pairing_review_is_invalidated_by_bundle_edits_and_profile_changes() {
        let mut panel = Panel::default();
        panel.pairing_hex = "abcd".into();
        let key = panel.pairing_key();
        let doc = serde_json::json!({"result":{"human_did":"human","device_did":"device","bundle_id":"a".repeat(64),"trusted":false}});
        panel.pairing = pairing_review(key.clone(), &doc);
        assert!(panel.pairing.is_some());
        panel.pairing_confirmation = "a".repeat(64);
        panel.pairing_hex = "dcba".into();
        assert!(panel.pairing.as_ref().unwrap().key != panel.pairing_key());
        panel.invalidate_pairing();
        assert!(panel.pairing.is_none() && panel.pairing_confirmation.is_empty());
        panel.pairing = pairing_review(key, &doc);
        panel.initialize_ack = true;
        panel.initialize_confirmation = "CREATE".into();
        panel.invalidate();
        assert!(panel.pairing.is_none() && panel.pairing_hex.is_empty());
        assert!(!panel.initialize_ack && panel.initialize_confirmation.is_empty());
        let mut invalid = doc;
        invalid["result"]["trusted"] = true.into();
        assert!(pairing_review(panel.pairing_key(), &invalid).is_none());
    }
}
