//! Owner-started ResidentAI view. Provider work never runs on the UI thread;
//! context is a previewed immutable snapshot, never implicit home access.
use eframe::egui;
use mini_resident::{ContextItem, ProviderConfig, ResidentReply, ResidentRequest};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// Share the CLI's owner-local model configuration, independently of the
/// desktop's object-store subdirectory. This contains no signing material.
pub(crate) fn config_home() -> PathBuf {
    std::env::var_os("MININET_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".mininet")
        })
}

#[derive(Default)]
pub(crate) struct Panel {
    patches: crate::resident_patch::Panel,
    loaded: bool,
    endpoint: String,
    model: String,
    configured: bool,
    prompt: String,
    workspace_root: String,
    workspace_candidate: String,
    discard_workspace_edits: bool,
    file: String,
    preview: Vec<ContextItem>,
    include_preview: bool,
    turns: VecDeque<Turn>,
    pending: Option<Pending>,
    local_query: String,
    local_output: String,
    local_pending: Option<Receiver<Result<String, String>>>,
    status: String,
}
struct Turn {
    question: String,
    source: String,
    context: Vec<String>,
    answer: Option<Result<ResidentReply, String>>,
}
struct Pending {
    receiver: Receiver<Result<ResidentReply, String>>,
    cancelled: bool,
}
impl Panel {
    pub(crate) fn poll_edits(&mut self) {
        self.patches.poll();
    }
    pub(crate) fn edit_in_progress(&self) -> bool {
        self.patches.is_writing()
    }

    pub(crate) fn request_close(&mut self) -> bool {
        self.patches.request_close()
    }
    pub(crate) fn close_dialog(&mut self, ctx: &egui::Context) {
        self.patches.close_dialog(ctx);
    }
    fn poll(&mut self) {
        match self.local_pending.as_ref().map(|rx| rx.try_recv()) {
            Some(Ok(result)) => {
                self.local_pending = None;
                self.local_output = result.unwrap_or_else(|e| e);
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.local_pending = None;
                self.local_output = "Local inspection worker stopped without a result.".into();
            }
            _ => (),
        }
        let result = match self.pending.as_ref().map(|p| p.receiver.try_recv()) {
            Some(Ok(result)) => Some(result),
            Some(Err(TryRecvError::Disconnected)) => {
                Some(Err("Local model worker stopped without a reply.".into()))
            }
            _ => None,
        };
        if let Some(result) = result {
            let pending = self
                .pending
                .take()
                .expect("worker result has pending state");
            if !pending.cancelled {
                if let Some(turn) = self.turns.back_mut() {
                    turn.answer = Some(result);
                }
                self.status = "Request finished. The response is advisory; any code remains an unapplied draft.".into();
            } else {
                self.status = "Cancelled response discarded. Ready for another request.".into();
            }
        }
    }
    fn config(&self) -> ProviderConfig {
        ProviderConfig {
            endpoint: self.endpoint.trim().into(),
            model: self.model.trim().into(),
        }
    }
    fn request(&self) -> ResidentRequest {
        ResidentRequest {
            question: self.prompt.clone(),
            context: if self.include_preview {
                self.preview.clone()
            } else {
                Vec::new()
            },
        }
    }
    fn send(&mut self, ctx: &egui::Context) {
        let config = self.config();
        let request = self.request();
        if let Err(e) = mini_resident::validate_provider_config(&config)
            .and_then(|()| mini_resident::validate_request(&request))
        {
            self.status = e.to_string();
            return;
        }
        let source = format!("{} at {}", config.model, config.endpoint);
        let turn = Turn {
            question: request.question.clone(),
            source,
            context: request
                .context
                .iter()
                .map(|c| format!("{} — {} bytes — {}", c.path, c.content.len(), c.hash))
                .collect(),
            answer: None,
        };
        let (tx, receiver) = mpsc::channel();
        let repaint = ctx.clone();
        match std::thread::Builder::new()
            .name("resident-ai-model".into())
            .spawn(move || {
                let result = mini_resident::chat(&config, &request).map_err(|e| e.to_string());
                let _ = tx.send(result);
                repaint.request_repaint();
            }) {
            Ok(_) => {
                if self.turns.len() == 8 {
                    self.turns.pop_front();
                }
                self.turns.push_back(turn);
                self.pending = Some(Pending {
                    receiver,
                    cancelled: false,
                });
                self.include_preview = false;
                self.status = "Waiting for the owner-selected local model. Only this request and the selected preview were sent.".into();
            }
            Err(e) => self.status = format!("Could not start local model worker: {e}"),
        }
    }
    fn inspect_local(&mut self, search: bool, ctx: &egui::Context) {
        let root = self.workspace_root.clone();
        let query = self.local_query.clone();
        let (tx, receiver) = mpsc::channel();
        let repaint = ctx.clone();
        match std::thread::Builder::new().name("resident-ai-workspace".into()).spawn(move || {
            let result = mini_resident::Workspace::open(Path::new(root.trim())).and_then(|workspace| {
                if search {
                    workspace.search(query.trim()).map(|hits| {
                        let mut output = format!("{} — query {:?}\n{} local search hits; line excerpts limited to 512 characters. No model request made.\n", root.trim(), query.trim(), hits.len());
                        for hit in hits { let excerpt: String = hit.text.chars().take(512).collect(); output.push_str(&format!("{}:{} {}\n", hit.path, hit.line, excerpt)); }
                        output
                    })
                } else {
                    workspace.inspect().map(|inspection| {
                        let mut output = format!("{}\n{} eligible files, {} excluded entries. Showing at most 100 paths. No model request made.\n", inspection.root, inspection.files.len(), inspection.excluded_entries);
                        for file in inspection.files.iter().take(100) { output.push_str(&format!("{} ({} bytes)\n", file.path, file.bytes)); }
                        output
                    })
                }
            }).map_err(|e| e.to_string());
            let _ = tx.send(result);
            repaint.request_repaint();
        }) {
            Ok(_) => { self.local_pending = Some(receiver); self.local_output = "Reading the explicitly selected local source workspace…".into(); }
            Err(e) => self.local_output = format!("Could not start local inspection: {e}"),
        }
    }
    pub(crate) fn ui(&mut self, ui: &mut egui::Ui, home: &Path) {
        if !self.loaded {
            self.loaded = true;
            self.configured = false;
            self.model.clear();
            self.endpoint = "http://127.0.0.1:11434".into();
            match mini_resident::load_config(home) {
                Ok(Some(config)) => {
                    self.endpoint = config.endpoint;
                    self.model = config.model;
                    self.configured = true;
                }
                Ok(None) => {
                    self.status =
                        "Choose a local model server and save its configuration to get started."
                            .into()
                }
                Err(e) => {
                    self.status = format!("ResidentAI configuration could not be loaded: {e}")
                }
            }
        }
        self.poll();
        ui.small(format!(
            "Owner-local configuration: {}",
            home.join("resident.json").display()
        ));
        ui.label("Your local assistant for Mininet. AI-generated advice has no governance vote, approval, spending or execution authority.");
        egui::CollapsingHeader::new("Local model setup").default_open(!self.configured).show(ui, |ui| {
            ui.label("Run an OpenAI-compatible model server yourself, then enter its numeric loopback HTTP endpoint and model name. ResidentAI does not install or start a model.");
            ui.horizontal(|ui| { ui.label("Endpoint"); if ui.text_edit_singleline(&mut self.endpoint).changed() { self.configured = false; } });
            ui.horizontal(|ui| { ui.label("Model"); if ui.text_edit_singleline(&mut self.model).changed() { self.configured = false; } });
            if ui.button("Reload saved configuration").clicked() { self.loaded = false; }
            if ui.button("Save local model configuration").clicked() {
                self.status = match mini_resident::save_config(home, &self.config()) {
                    Ok(()) => { self.configured = true; "Configuration saved locally. No model request has been made.".into() }
                    Err(e) => e.to_string(),
                };
            }
        });
        ui.separator();
        ui.label("Start a task — these buttons prepare a question; they do not run tools.");
        ui.horizontal_wrapped(|ui| {
            for (label, prompt) in [
                ("Coding", "Help me review this code and propose a small, unapplied patch. Explain any assumptions and validation I should perform."),
                ("Checks", "Help me choose focused checks for this change. Suggest commands for me to review; do not claim they have run."),
                ("Search", "Help me understand the attached source and identify useful symbols or files to search locally."),
                ("Governance", "Explain the governance implications of the attached material. Distinguish advisory analysis from actual human approval or canonical decisions."),
                ("Hosting", "Help me plan owner-controlled hosting in Mininet, including explicit peer trust and what information is shared."),
                ("Fees", "Explain Mininet fee concepts and what evidence I should inspect. Do not execute payments or claim a balance or settlement status."),
            ] {
                if ui.add_enabled(self.pending.is_none(), egui::Button::new(label)).clicked() { self.prompt = prompt.into(); }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Source workspace for context and edits");
            if ui
                .add_enabled(
                    !self.patches.is_busy(),
                    egui::TextEdit::singleline(&mut self.workspace_candidate),
                )
                .changed()
            {
                self.discard_workspace_edits = false;
            }
            if self.patches.has_unsaved_work() {
                ui.add_enabled(
                    !self.patches.is_busy(),
                    egui::Checkbox::new(
                        &mut self.discard_workspace_edits,
                        "Changing workspace may discard unsaved edits",
                    ),
                );
            }
            if ui
                .add_enabled(
                    !self.patches.is_busy()
                        && (!self.patches.has_unsaved_work() || self.discard_workspace_edits),
                    egui::Button::new("Use this workspace"),
                )
                .clicked()
            {
                self.workspace_root = self.workspace_candidate.trim().into();
                self.discard_workspace_edits = false;
                self.preview.clear();
                self.include_preview = false;
            }
        });
        ui.small(format!(
            "Active workspace: {}",
            if self.workspace_root.is_empty() {
                "none"
            } else {
                &self.workspace_root
            }
        ));
        egui::CollapsingHeader::new("Optional file context — preview before sharing").show(ui, |ui| {
            ui.label("Choose a source workspace and one relative UTF-8 file. Known secret paths, symlinks and oversized context are refused. Review the preview for sensitive content.");
            ui.horizontal(|ui| { ui.label("Relative file"); if ui.text_edit_singleline(&mut self.file).changed() { self.preview.clear(); self.include_preview = false; } });
            if ui.add_enabled(self.pending.is_none(), egui::Button::new("Preview attachment")).clicked() {
                self.include_preview = false;
                self.preview.clear();
                match mini_resident::Workspace::open(Path::new(self.workspace_root.trim()))
                    .and_then(|workspace| workspace.context(&[self.file.trim().to_string()])) {
                    Ok(context) => self.preview = context,
                    Err(e) => self.status = e.to_string(),
                }
            }
            for item in &self.preview {
                ui.label(format!("{} — {} UTF-8 bytes — hash {}", item.path, item.content.len(), item.hash));
                egui::ScrollArea::vertical().id_salt("resident-context-preview").max_height(180.0).show(ui, |ui| {
                    let mut exact_preview = item.content.as_str();
                    ui.add(egui::TextEdit::multiline(&mut exact_preview).code_editor().desired_width(f32::INFINITY).interactive(false));
                });
            }
            ui.add_enabled(!self.preview.is_empty() && self.pending.is_none(), egui::Checkbox::new(&mut self.include_preview, "Include this exact preview in the next request"));
        });
        egui::CollapsingHeader::new("Local workspace tools — no model required").show(ui, |ui| {
            ui.label("These tools read only the source workspace entered above. Their output stays here and is not automatically sent to the model.");
            ui.horizontal(|ui| {
                let idle = self.pending.is_none() && self.local_pending.is_none();
                if ui.add_enabled(idle && !self.workspace_root.trim().is_empty(), egui::Button::new("Inspect local workspace")).clicked() { self.inspect_local(false, ui.ctx()); }
                ui.label("Find text");
                ui.add(egui::TextEdit::singleline(&mut self.local_query).char_limit(256));
                if ui.add_enabled(idle && !self.workspace_root.trim().is_empty() && !self.local_query.trim().is_empty(), egui::Button::new("Search locally")).clicked() { self.inspect_local(true, ui.ctx()); }
            });
            if !self.local_output.is_empty() {
                egui::ScrollArea::vertical().id_salt("resident-local-tools").max_height(200.0).show(ui, |ui| {
                    let mut output = self.local_output.as_str();
                    ui.add(egui::TextEdit::multiline(&mut output).code_editor().desired_width(f32::INFINITY).interactive(false));
                });
            }
        });
        self.patches.ui(ui, self.workspace_root.trim());
        ui.label("Question");
        ui.add(
            egui::TextEdit::multiline(&mut self.prompt)
                .desired_rows(4)
                .desired_width(f32::INFINITY)
                .char_limit(mini_resident::MAX_QUESTION_BYTES),
        );
        ui.small(format!("{} / {} question bytes. Each request stands alone; previous chat is not automatically sent.", self.prompt.len(), mini_resident::MAX_QUESTION_BYTES));
        ui.horizontal(|ui| {
            if ui.add_enabled(self.configured && self.pending.is_none() && self.local_pending.is_none() && !self.prompt.trim().is_empty(), egui::Button::new("Send to local model")).clicked() { self.send(ui.ctx()); }
            if ui.add_enabled(self.pending.as_ref().is_some_and(|p| !p.cancelled), egui::Button::new("Cancel response")).clicked() {
                if let Some(pending) = self.pending.as_mut() { pending.cancelled = true; }
                if let Some(turn) = self.turns.back_mut() { turn.answer = Some(Err("Response cancelled; any late output will be discarded.".into())); }
                self.status = "Response cancelled. The bounded local request may finish in the background; another request waits until it exits.".into();
            }
            if ui.button("Clear conversation").clicked() {
                self.turns.clear(); self.prompt.clear(); self.preview.clear(); self.include_preview = false;
                if let Some(pending) = self.pending.as_mut() { pending.cancelled = true; }
                self.status = "Conversation and attachment cleared from this panel.".into();
            }
        });
        if !self.configured {
            ui.label("No local model is configured. Save configuration above to enable requests.");
        }
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
        ui.separator();
        for turn in &self.turns {
            ui.strong("You");
            ui.label(&turn.question);
            ui.small(format!("Owner-selected provider: {}", turn.source));
            for source in &turn.context {
                ui.small(source);
            }
            match &turn.answer {
                None => {
                    ui.label("ResidentAI is waiting for the local model…");
                }
                Some(Err(error)) => {
                    ui.colored_label(egui::Color32::from_rgb(220, 120, 90), error);
                }
                Some(Ok(reply)) => {
                    ui.strong("ResidentAI · AI-generated");
                    ui.small(format!(
                        "Model: {} · {}",
                        reply.model, reply.provenance.label
                    ));
                    ui.small("Advisory output. Code and commands are displayed as unapplied text.");
                    if ui
                        .add_enabled(
                            !self.patches.is_busy(),
                            egui::Button::new("Use answer as unapplied replacement draft"),
                        )
                        .clicked()
                    {
                        self.patches
                            .draft_reply(self.workspace_root.trim(), &reply.text);
                    }
                    let mut text = reply.text.as_str();
                    ui.add(
                        egui::TextEdit::multiline(&mut text)
                            .desired_width(f32::INFINITY)
                            .desired_rows(6)
                            .interactive(false),
                    );
                }
            }
            ui.separator();
        }
        if self.pending.is_some() || self.local_pending.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_owner_selected_preview_enters_request_snapshot() {
        let mut panel = Panel {
            prompt: "Review this".into(),
            preview: vec![ContextItem {
                path: "src/lib.rs".into(),
                content: "original".into(),
                hash: mini_resident::content_hash(b"original"),
            }],
            ..Panel::default()
        };
        assert!(panel.request().context.is_empty());
        panel.include_preview = true;
        let request = panel.request();
        panel.preview[0].content = "changed after preview".into();
        assert_eq!(request.context[0].content, "original");
        mini_resident::validate_request(&request).unwrap();
    }

    #[test]
    fn cancelled_late_reply_never_enters_cleared_conversation() {
        let (tx, receiver) = mpsc::channel();
        let mut panel = Panel {
            pending: Some(Pending {
                receiver,
                cancelled: true,
            }),
            ..Panel::default()
        };
        tx.send(Ok(ResidentReply::new(
            "old conversation output".into(),
            "local".into(),
        )
        .unwrap()))
            .unwrap();
        panel.poll();
        assert!(panel.pending.is_none());
        assert!(panel.turns.is_empty());
    }
}
