//! Owner-reviewed edits. Provider output enters only as an editable draft;
//! filesystem writes require a current preview and a separate exact confirmation.
use eframe::egui;
use mini_resident::{ContextItem, PatchEdit, PatchPreview, PatchReceipt, Workspace};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[derive(Clone)]
struct Edit {
    edit: PatchEdit,
    original: Option<String>,
}
struct Loaded {
    path: String,
    original: Option<ContextItem>,
}
enum Outcome {
    Loaded(Loaded),
    Preview(PatchPreview),
    Applied(PatchReceipt),
}
struct Pending {
    receiver: Receiver<Result<Outcome, String>>,
    applying: bool,
    root: String,
}
#[derive(Default)]
pub(crate) struct Panel {
    root: String,
    path: String,
    create: bool,
    loaded: Option<Loaded>,
    replacement: String,
    replacement_dirty: bool,
    edits: Vec<Edit>,
    preview: Option<PatchPreview>,
    confirmation: String,
    pending: Option<Pending>,
    status: String,
}
impl Panel {
    pub(crate) fn is_busy(&self) -> bool {
        self.pending.is_some()
    }
    pub(crate) fn is_applying(&self) -> bool {
        self.pending.as_ref().is_some_and(|p| p.applying)
    }
    fn invalidate(&mut self) {
        self.preview = None;
        self.confirmation.clear();
    }
    fn sync_root(&mut self, root: &str) {
        if self.root == root {
            return;
        }
        self.root = root.into();
        self.loaded = None;
        self.replacement.clear();
        self.replacement_dirty = false;
        self.edits.clear();
        self.invalidate();
        self.status =
            "Workspace changed. Draft files, review and confirmation were cleared.".into();
    }
    pub(crate) fn draft_reply(&mut self, root: &str, text: &str) {
        if self.is_busy() {
            self.status = "Wait for the current edit operation before taking another draft.".into();
            return;
        }
        self.sync_root(root);
        if text.len() > mini_resident::MAX_CONTEXT_BYTES {
            self.status = "Reply exceeds the replacement size limit.".into();
            return;
        }
        self.replacement = text.into();
        self.replacement_dirty = true;
        self.invalidate();
        self.status = "Model answer copied literally into an unapplied replacement draft. Review prose/code fences and choose/load the intended file before staging.".into();
    }
    fn spawn(
        &mut self,
        applying: bool,
        work: impl FnOnce() -> Result<Outcome, String> + Send + 'static,
    ) {
        if self.is_busy() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        match std::thread::Builder::new()
            .name("resident-reviewed-edit".into())
            .spawn(move || {
                let _ = sender.send(work());
            }) {
            Ok(_) => {
                self.pending = Some(Pending {
                    receiver,
                    applying,
                    root: self.root.clone(),
                });
                self.status = if applying { "Applying the exact reviewed edit. This write cannot be cancelled; wait for its receipt." } else { "Reading and validating the selected workspace locally…" }.into();
            }
            Err(e) => self.status = e.to_string(),
        }
    }
    pub(crate) fn poll(&mut self) {
        let result = match self.pending.as_ref().map(|p| p.receiver.try_recv()) {
            Some(Ok(result)) => Some(result),
            Some(Err(TryRecvError::Disconnected)) => Some(Err("Edit worker stopped without a receipt. Inspect the workspace before retrying; completed writes are not undone.".into())),
            _ => None,
        };
        let Some(result) = result else {
            return;
        };
        let pending = self.pending.take().expect("completed operation has state");
        if pending.root != self.root {
            self.status = format!("Operation for previous workspace {} finished. Inspect that workspace before retrying.",pending.root);
            return;
        }
        match result {
            Err(error) => {
                self.invalidate();
                self.status =
                    format!("Edit operation failed: {error}. Inspect files before retrying.");
            }
            Ok(Outcome::Loaded(loaded)) => {
                if !self.replacement_dirty {
                    self.replacement = loaded
                        .original
                        .as_ref()
                        .map(|f| f.content.clone())
                        .unwrap_or_default();
                }
                self.loaded = Some(loaded);
                self.invalidate();
                self.status = "File baseline loaded locally. Edit its complete replacement, then stage it for review.".into();
            }
            Ok(Outcome::Preview(preview)) => {
                self.confirmation.clear();
                self.preview = Some(preview);
                self.status = "Preview ready. Review every original and replacement; type the exact ID to authorize these writes.".into();
            }
            Ok(Outcome::Applied(receipt)) => {
                self.edits.clear();
                self.loaded = None;
                self.replacement.clear();
                self.replacement_dirty = false;
                self.invalidate();
                self.status = format!("Applied preview {} to workspace {}: {}. {}. This is a local file edit, not a commit, test result, approval or publication.",receipt.preview_id,pending.root,receipt.patched_paths.join(", "),receipt.provenance.label);
            }
        }
    }
    fn load(&mut self) {
        let (root, path, create) = (self.root.clone(), self.path.trim().to_owned(), self.create);
        self.loaded = None;
        self.invalidate();
        self.spawn(false, move || {
            let workspace = Workspace::open(Path::new(&root)).map_err(|e| e.to_string())?;
            let original = if create {
                workspace
                    .preview_patch(&[PatchEdit {
                        path: path.clone(),
                        before_hash: None,
                        replacement: String::new(),
                    }])
                    .map_err(|e| e.to_string())?;
                None
            } else {
                Some(
                    workspace
                        .context(std::slice::from_ref(&path))
                        .map_err(|e| e.to_string())?
                        .into_iter()
                        .next()
                        .ok_or("No file context returned")?,
                )
            };
            Ok(Outcome::Loaded(Loaded { path, original }))
        });
    }
    fn stage(&mut self) -> Result<(), String> {
        let loaded = self.loaded.as_ref().ok_or("Load a file baseline first")?;
        if loaded.path != self.path.trim() || self.create != loaded.original.is_none() {
            return Err("Reload the selected file before staging".into());
        }
        let entry = Edit {
            edit: PatchEdit {
                path: loaded.path.clone(),
                before_hash: loaded.original.as_ref().map(|f| f.hash.clone()),
                replacement: self.replacement.clone(),
            },
            original: loaded.original.as_ref().map(|f| f.content.clone()),
        };
        let mut edits = self.edits.clone();
        if let Some(index) = edits.iter().position(|e| e.edit.path == entry.edit.path) {
            edits[index] = entry;
        } else {
            edits.push(entry);
        }
        if edits.len() > 16
            || edits
                .iter()
                .map(|e| e.edit.replacement.len())
                .sum::<usize>()
                > mini_resident::MAX_CONTEXT_BYTES
        {
            return Err("Stage at most 16 files / 256 KiB replacement bytes".into());
        }
        self.edits = edits;
        self.invalidate();
        Ok(())
    }
    pub(crate) fn ui(&mut self, ui: &mut egui::Ui, root: &str) {
        self.sync_root(root);
        self.poll();
        egui::CollapsingHeader::new("Reviewed local code edits").show(ui,|ui| {
            ui.label("Work in a clean owner-controlled workspace with a backup. Edit proposals stay local and are never sent to the model automatically. No shell, build, commit or network operation runs here.");
            ui.label(format!("Selected workspace: {}", self.root));
            let idle = !self.is_busy();
            ui.add_enabled_ui(idle,|ui| {
                let changed = ui.horizontal(|ui| { ui.label("Relative UTF-8 source file"); ui.add(egui::TextEdit::singleline(&mut self.path).char_limit(1024)).changed() }).inner;
                let changed = ui.checkbox(&mut self.create,"Create a new file (parent directory must exist)").changed() || changed;
                if changed { self.loaded = None; self.replacement.clear(); self.replacement_dirty = false; self.invalidate(); }
                if ui.add_enabled(!self.root.trim().is_empty() && !self.path.trim().is_empty(),egui::Button::new("Load / validate file baseline")).clicked() { self.load(); }
                ui.label("Complete replacement text (an empty replacement creates an empty file; it does not delete the path)");
                if ui.add(egui::TextEdit::multiline(&mut self.replacement).code_editor().desired_rows(8).desired_width(f32::INFINITY).char_limit(mini_resident::MAX_CONTEXT_BYTES)).changed() { self.replacement_dirty = true; self.invalidate(); }
                if ui.add_enabled(self.loaded.is_some(),egui::Button::new("Stage this file for review")).clicked() { self.status = match self.stage() { Ok(())=>"File staged. Create a fresh preview before applying.".into(),Err(e)=>e }; }
                ui.separator();
                ui.label(format!("{} staged files (maximum 16 / 256 KiB replacement bytes)",self.edits.len()));
                let mut remove = None;
                for (index,entry) in self.edits.iter().enumerate() { ui.horizontal(|ui| { ui.monospace(format!("{} — {} bytes",entry.edit.path,entry.edit.replacement.len())); if ui.button("Remove").clicked() { remove = Some(index); } }); }
                if let Some(index) = remove { self.edits.remove(index); self.invalidate(); }
                if ui.add_enabled(!self.edits.is_empty(),egui::Button::new("Validate and preview all staged edits")).clicked() {
                    self.invalidate();
                    let root = self.root.clone(); let edits = self.edits.iter().map(|e|e.edit.clone()).collect::<Vec<_>>();
                    self.spawn(false, move || Workspace::open(Path::new(&root)).and_then(|w|w.preview_patch(&edits)).map(Outcome::Preview).map_err(|e|e.to_string()));
                }
            });
            let mut apply = None;
            if let Some(preview) = &self.preview {
                ui.label(format!("Exact preview ID: {}",preview.id));
                ui.small(&preview.provenance.label);
                for entry in &self.edits {
                    ui.strong(&entry.edit.path);
                    ui.small(format!("Before hash: {}",entry.edit.before_hash.as_deref().unwrap_or("new path; must remain absent")));
                    ui.collapsing(format!("Review original and complete replacement: {}",entry.edit.path),|ui| {
                        ui.label("Original"); let mut before = entry.original.as_deref().unwrap_or("(new file)");
                        ui.add(egui::TextEdit::multiline(&mut before).code_editor().desired_rows(5).desired_width(f32::INFINITY).interactive(false));
                        ui.label("Replacement"); let mut after = entry.edit.replacement.as_str();
                        ui.add(egui::TextEdit::multiline(&mut after).code_editor().desired_rows(5).desired_width(f32::INFINITY).interactive(false));
                    });
                }
                ui.label("Type the exact preview ID to authorize replacing every listed file");
                ui.add_enabled(idle,egui::TextEdit::singleline(&mut self.confirmation));
                if ui.add_enabled(idle && self.confirmation == preview.id,egui::Button::new("Apply these reviewed local edits")).clicked() { apply = Some((preview.clone(),self.confirmation.clone())); }
            }
            if let Some((preview,confirmation)) = apply {
                self.invalidate(); let root = self.root.clone();
                self.spawn(true,move || Workspace::open(Path::new(&root)).and_then(|w|w.apply_patch(&preview,&confirmation)).map(Outcome::Applied).map_err(|e|e.to_string()));
            }
            if ui.add_enabled(idle,egui::Button::new("Discard all edit drafts and review")).clicked() { self.edits.clear(); self.loaded = None; self.replacement.clear(); self.replacement_dirty = false; self.invalidate(); self.status = "Unapplied edit drafts discarded.".into(); }
            if let Some(pending) = &self.pending { ui.label(if pending.applying { "Writing the exact approved local patch; do not close the application." } else { "Reading/validating locally…" }); }
            ui.label(&self.status);
        });
        if self.is_busy() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn loaded_panel() -> Panel {
        Panel {
            root: "workspace".into(),
            path: "src.rs".into(),
            loaded: Some(Loaded {
                path: "src.rs".into(),
                original: Some(ContextItem {
                    path: "src.rs".into(),
                    content: "before".into(),
                    hash: mini_resident::content_hash(b"before"),
                }),
            }),
            replacement: "after".into(),
            ..Panel::default()
        }
    }
    #[test]
    fn model_answer_is_only_a_draft_and_clears_previous_authorization() {
        let mut panel = loaded_panel();
        panel.stage().unwrap();
        panel.preview = Some(PatchPreview {
            id: "old-id".into(),
            edits: panel.edits.iter().map(|e| e.edit.clone()).collect(),
            provenance: Default::default(),
        });
        panel.confirmation = "old-id".into();
        panel.draft_reply("workspace", "model prose and ```code``` are literal text");
        assert_eq!(panel.edits[0].edit.replacement, "after");
        assert!(panel.preview.is_none() && panel.confirmation.is_empty());
        assert!(panel.pending.is_none());
        panel.stage().unwrap();
        assert_eq!(
            panel.edits[0].edit.replacement,
            "model prose and ```code``` are literal text"
        );
        assert_eq!(panel.edits[0].original.as_deref(), Some("before"));
    }
    #[test]
    fn workspace_changes_and_wrong_file_baselines_cannot_carry_authority() {
        let mut panel = loaded_panel();
        panel.path = "other.rs".into();
        assert!(panel.stage().is_err());
        panel.path = "src.rs".into();
        panel.stage().unwrap();
        panel.confirmation = "stale".into();
        panel.sync_root("different-workspace");
        assert!(panel.edits.is_empty() && panel.loaded.is_none());
        assert!(panel.preview.is_none() && panel.confirmation.is_empty());
        assert!(panel.replacement.is_empty());
    }
    #[test]
    fn loading_baseline_preserves_an_explicit_model_draft_without_staging_it() {
        let mut panel = Panel::default();
        panel.draft_reply("workspace", "replacement chosen from model output");
        let (sender, receiver) = mpsc::channel();
        sender
            .send(Ok(Outcome::Loaded(Loaded {
                path: "src.rs".into(),
                original: Some(ContextItem {
                    path: "src.rs".into(),
                    content: "old source".into(),
                    hash: mini_resident::content_hash(b"old source"),
                }),
            })))
            .unwrap();
        panel.pending = Some(Pending {
            receiver,
            applying: false,
            root: "workspace".into(),
        });
        panel.poll();
        assert_eq!(panel.replacement, "replacement chosen from model output");
        assert_eq!(
            panel
                .loaded
                .as_ref()
                .unwrap()
                .original
                .as_ref()
                .unwrap()
                .content,
            "old source"
        );
        assert!(panel.edits.is_empty() && panel.preview.is_none());
        assert!(panel.pending.is_none());
    }
}
