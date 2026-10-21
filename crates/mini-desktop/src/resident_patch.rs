//! Owner-reviewed edits. Provider output enters only as an editable draft;
//! filesystem writes require a current preview and a separate exact confirmation.
use eframe::egui;
use mini_resident::{
    ContextItem, DraftInfo, DraftReview, PatchEdit, PatchPreview, PatchReceipt, SavedDraft,
    Workspace,
};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[derive(Clone)]
struct Edit {
    edit: PatchEdit,
    original: Option<String>,
}
#[derive(Clone)]
struct Loaded {
    path: String,
    original: Option<ContextItem>,
}
struct EditorDraft {
    path: String,
    create: bool,
    loaded: Option<Loaded>,
    replacement: String,
}
enum Outcome {
    Loaded(Loaded),
    Preview(PatchPreview),
    Applied(PatchReceipt),
    Saved(SavedDraft),
    Resumed(SavedDraft),
    DraftInfo(DraftInfo),
    Cleared(String),
    Recovery(DraftReview),
}
struct Pending {
    receiver: Receiver<Result<Outcome, String>>,
    writing: bool,
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
    saved_info: Option<DraftInfo>,
    delete_confirmation: String,
    replace_unsaved: bool,
    saved_edits: Option<Vec<PatchEdit>>,
    close_requested: bool,
    recovery: Option<DraftReview>,
    recovery_index: Option<usize>,
    accept_recovery: bool,
    pending_editor: Option<EditorDraft>,
}
impl Panel {
    pub(crate) fn is_busy(&self) -> bool {
        self.pending.is_some() || self.pending_editor.is_some()
    }
    pub(crate) fn is_writing(&self) -> bool {
        self.pending.as_ref().is_some_and(|p| p.writing)
    }
    fn editor_unstaged(&self) -> bool {
        if !self.replacement_dirty {
            return false;
        }
        let Some(loaded) = &self.loaded else {
            return !self.replacement.is_empty();
        };
        if let Some(staged) = self.edits.iter().find(|e| e.edit.path == loaded.path) {
            self.replacement != staged.edit.replacement
                || loaded.original.as_ref().map(|file| &file.hash)
                    != staged.edit.before_hash.as_ref()
        } else {
            loaded
                .original
                .as_ref()
                .is_none_or(|original| self.replacement != original.content)
        }
    }
    pub(crate) fn has_unsaved_work(&self) -> bool {
        self.editor_unstaged()
            || (!self.edits.is_empty()
                && self
                    .saved_edits
                    .as_ref()
                    .is_none_or(|saved| saved.iter().ne(self.edits.iter().map(|e| &e.edit))))
    }
    pub(crate) fn request_close(&mut self) -> bool {
        if self.is_writing() {
            return true;
        }
        if self.has_unsaved_work() {
            self.close_requested = true;
            return true;
        }
        false
    }
    pub(crate) fn close_dialog(&mut self, ctx: &egui::Context) {
        if !self.close_requested {
            return;
        }
        egui::Window::new("Unsaved Resident edits").collapsible(false).resizable(false).show(ctx,|ui| {
            ui.label("There are source edits that have not been saved. Saving a staged draft does not save unstaged editor text. Choose Keep editing, stage the intended text, and save before closing if you want to retain it.");
            if ui.button("Keep editing to stage / save").clicked() { self.close_requested = false; }
            if ui.add_enabled(!self.is_busy(),egui::Button::new("Close without saving in-memory edits")).clicked() {
                self.discard_memory(); self.close_requested = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
    fn discard_memory(&mut self) {
        self.edits.clear();
        self.loaded = None;
        self.replacement.clear();
        self.replacement_dirty = false;
        self.invalidate();
    }
    fn invalidate(&mut self) {
        self.recovery = None;
        self.recovery_index = None;
        self.accept_recovery = false;
        self.replace_unsaved = false;
        self.preview = None;
        self.confirmation.clear();
    }
    fn sync_root(&mut self, root: &str) {
        if self.root == root {
            return;
        }
        self.root = root.into();
        self.pending_editor = None;
        self.saved_edits = None;
        self.close_requested = false;
        self.loaded = None;
        self.replacement.clear();
        self.replacement_dirty = false;
        self.edits.clear();
        self.saved_info = None;
        self.delete_confirmation.clear();
        self.replace_unsaved = false;
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
        let target = EditorDraft {
            path: self.path.clone(),
            create: self.create,
            loaded: self.loaded.clone(),
            replacement: text.into(),
        };
        self.request_editor(target);
    }
    fn request_editor(&mut self, target: EditorDraft) {
        if self.editor_unstaged() {
            self.pending_editor = Some(target);
        } else {
            self.install_editor(target);
        }
    }
    fn install_editor(&mut self, target: EditorDraft) {
        self.path = target.path;
        self.create = target.create;
        self.loaded = target.loaded;
        self.replacement = target.replacement;
        self.replacement_dirty = true;
        self.invalidate();
        self.status="Selected text loaded literally into an unapplied editor draft. Existing staged files are unchanged; review and stage this text separately.".into();
    }
    fn resolve_editor(&mut self, replace: bool) {
        if let Some(target) = self.pending_editor.take() {
            if replace {
                self.install_editor(target);
            } else {
                self.status =
                    "Kept existing editor text. The selected replacement was not loaded.".into();
            }
        }
    }
    fn spawn(
        &mut self,
        writing: bool,
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
                    writing,
                    root: self.root.clone(),
                });
                self.status = if writing { "Completing the requested local write. It cannot be cancelled halfway through; wait for its result." } else { "Reading and validating the selected workspace locally…" }.into();
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
            Ok(Outcome::Recovery(review)) => {
                self.invalidate();
                self.saved_info = Some(DraftInfo {
                    id: review.id.clone(),
                    paths: review.files.iter().map(|f| f.edit.path.clone()).collect(),
                });
                self.recovery = Some(review);
                self.status="Saved replacements recovered for comparison only. Select a file, inspect the current source, and explicitly accept its current baseline before loading a repair draft.".into();
            }
            Ok(Outcome::Saved(saved)) => {
                self.saved_edits = Some(saved.preview.edits.clone());
                let id = saved.preview.id;
                self.saved_info = Some(DraftInfo {
                    id: id.clone(),
                    paths: saved.preview.edits.iter().map(|e| e.path.clone()).collect(),
                });
                self.delete_confirmation.clear();
                self.invalidate();
                self.status = format!("Saved staged draft {id} privately in .resident-draft.json. Source files are unchanged. Review again before applying.");
            }
            Ok(Outcome::Resumed(saved)) => {
                self.resume(saved);
                self.status = "Saved edits resumed with fresh baseline checks. Inspect the staged files and create a fresh preview; no confirmation or apply was restored.".into();
            }
            Ok(Outcome::DraftInfo(info)) => {
                if self
                    .saved_info
                    .as_ref()
                    .is_some_and(|old| old.id != info.id)
                {
                    self.saved_edits = None;
                }
                self.saved_info = Some(info);
                self.delete_confirmation.clear();
                self.status = "Stored draft metadata inspected. This does not prove the source files still match; Resume performs those checks.".into();
            }
            Ok(Outcome::Cleared(id)) => {
                self.saved_edits = None;
                self.saved_info = None;
                self.delete_confirmation.clear();
                self.status = format!(
                    "Removed stored draft {id}. Source files and in-memory edits were not changed."
                );
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
    fn resume(&mut self, saved: SavedDraft) {
        self.saved_edits = Some(saved.preview.edits.clone());
        self.saved_info = Some(DraftInfo {
            id: saved.preview.id.clone(),
            paths: saved.preview.edits.iter().map(|e| e.path.clone()).collect(),
        });
        self.edits = saved
            .preview
            .edits
            .into_iter()
            .map(|edit| {
                let original = saved
                    .originals
                    .iter()
                    .find(|f| f.path == edit.path)
                    .map(|f| f.content.clone());
                Edit { edit, original }
            })
            .collect();
        self.path.clear();
        self.loaded = None;
        self.replacement.clear();
        self.replacement_dirty = false;
        self.replace_unsaved = false;
        self.delete_confirmation.clear();
        self.invalidate();
    }
    fn edit_staged(&mut self, index: usize) {
        let entry = &self.edits[index];
        let target = EditorDraft {
            path: entry.edit.path.clone(),
            create: entry.edit.before_hash.is_none(),
            loaded: Some(Loaded {
                path: entry.edit.path.clone(),
                original: entry.original.as_ref().map(|content| ContextItem {
                    path: entry.edit.path.clone(),
                    content: content.clone(),
                    hash: entry
                        .edit
                        .before_hash
                        .clone()
                        .expect("existing baseline has hash"),
                }),
            }),
            replacement: entry.edit.replacement.clone(),
        };
        self.request_editor(target);
    }
    fn recover_file(&mut self) -> Result<(), String> {
        if !self.accept_recovery {
            return Err("Explicitly accept the displayed current baseline first".into());
        }
        let index = self.recovery_index.ok_or("Choose a saved file first")?;
        let file = self
            .recovery
            .as_ref()
            .and_then(|review| review.files.get(index))
            .cloned()
            .ok_or("Refresh saved-file comparison")?;
        self.path = file.edit.path.clone();
        self.create = file.current.is_none();
        self.loaded = Some(Loaded {
            path: file.edit.path,
            original: file.current,
        });
        self.replacement = file.edit.replacement;
        self.replacement_dirty = true;
        self.invalidate();
        self.status="Saved replacement loaded as an unstaged repair draft against the explicitly accepted current baseline. Edit, stage and create a fresh exact preview. Source is unchanged.".into();
        Ok(())
    }
    fn can_resume(&self) -> bool {
        self.replace_unsaved || !self.has_unsaved_work()
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
        if self.pending_editor.is_some() {
            egui::Window::new("Keep unsaved editor text?").collapsible(false).resizable(false).show(ui.ctx(),|ui| {
                ui.label("Loading this selection would replace unstaged editor text. Existing staged edits and stored checkpoints are not changed by this choice.");
                if ui.button("Keep existing editor text").clicked() { self.resolve_editor(false); }
                if ui.button("Replace editor text with selected draft").clicked() { self.resolve_editor(true); }
            });
        }
        egui::CollapsingHeader::new("Reviewed local code edits").show(ui,|ui| {
            ui.label("Work in a clean owner-controlled workspace with a backup. Edit proposals stay local and are never sent to the model automatically. No shell, build, commit or network operation runs here.");
            ui.label(format!("Selected workspace: {}", self.root));
            if self.has_unsaved_work() { ui.label("Unsaved edits: stage editor changes before saving. Only explicitly saved staged edits survive closing."); }
            let idle = !self.is_busy();
            ui.add_enabled_ui(idle,|ui| {
                let changed = ui.horizontal(|ui| { ui.label("Relative UTF-8 source file"); ui.add(egui::TextEdit::singleline(&mut self.path).char_limit(1024)).changed() }).inner;
                let changed = ui.checkbox(&mut self.create,"Create a new file (parent directory must exist)").changed() || changed;
                if changed { let keep_text=self.editor_unstaged(); self.loaded = None; if !keep_text { self.replacement.clear(); self.replacement_dirty = false; } self.invalidate(); }
                if ui.add_enabled(!self.root.trim().is_empty() && !self.path.trim().is_empty(),egui::Button::new("Load / validate file baseline")).clicked() { self.load(); }
                ui.label("Complete replacement text (an empty replacement creates an empty file; it does not delete the path)");
                if ui.add(egui::TextEdit::multiline(&mut self.replacement).code_editor().desired_rows(8).desired_width(f32::INFINITY).char_limit(mini_resident::MAX_CONTEXT_BYTES)).changed() { self.replacement_dirty = true; self.invalidate(); }
                if ui.add_enabled(self.loaded.is_some(),egui::Button::new("Stage this file for review")).clicked() { self.status = match self.stage() { Ok(())=>"File staged. Create a fresh preview before writing.".into(),Err(e)=>e }; }
                ui.separator();
                ui.label(format!("{} staged files (maximum 16 / 256 KiB replacement bytes)",self.edits.len()));
                let mut remove = None;
                let mut edit_index = None;
                for (index, entry) in self.edits.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.monospace(format!("{} — {} bytes", entry.edit.path, entry.edit.replacement.len()));
                        if ui.button("Edit staged replacement").clicked() { edit_index = Some(index); }
                        if ui.button("Remove").clicked() { remove = Some(index); }
                    });
                }
                if let Some(index) = edit_index { self.edit_staged(index); }
                if let Some(index) = remove { self.edits.remove(index); self.invalidate(); }
                if ui.add_enabled(!self.edits.is_empty(),egui::Button::new("Validate and preview all staged edits")).clicked() {
                    self.invalidate();
                    let root = self.root.clone(); let edits = self.edits.iter().map(|e|e.edit.clone()).collect::<Vec<_>>();
                    self.spawn(false, move || Workspace::open(Path::new(&root)).and_then(|w|w.preview_patch(&edits)).map(Outcome::Preview).map_err(|e|e.to_string()));
                }
            });
            egui::CollapsingHeader::new("Save and resume staged edits locally").show(ui, |ui| {
                ui.label("Save writes staged edits to .resident-draft.json in this workspace. A new checkpoint is created only if absent; replacing one requires inspecting its current ID first. Another window changing that ID causes save to refuse. Unstaged editor text is not saved. The file contains source replacements in plaintext: keep it private. Saving does not change source files or authorize applying them.");
                if ui.add_enabled(idle && !self.edits.is_empty(), egui::Button::new("Save / replace stored staged draft")).clicked() {
                    self.invalidate(); let root=self.root.clone(); let expected=self.saved_info.as_ref().map(|info|info.id.clone()); let edits=self.edits.iter().map(|e|e.edit.clone()).collect::<Vec<_>>();
                    self.spawn(true,move || Workspace::open(Path::new(&root)).and_then(|w|w.save_draft_if(&edits,expected.as_deref())).map(Outcome::Saved).map_err(|e|e.to_string()));
                }
                if ui.add_enabled(idle && !self.root.trim().is_empty(),egui::Button::new("Inspect stored draft metadata")).clicked() {
                    let root=self.root.clone();
                    self.spawn(false,move || Workspace::open(Path::new(&root)).and_then(|w|w.draft_info()).map(Outcome::DraftInfo).map_err(|e|e.to_string()));
                }
                if self.has_unsaved_work() { ui.add_enabled(idle,egui::Checkbox::new(&mut self.replace_unsaved,"Resume may replace these in-memory staged edits and editor text")); }
                if ui.add_enabled(idle && self.can_resume() && !self.root.trim().is_empty(),egui::Button::new("Resume and revalidate stored staged edits")).clicked() {
                    self.replace_unsaved=false; self.invalidate(); let root=self.root.clone();
                    self.spawn(false,move || Workspace::open(Path::new(&root)).and_then(|w|w.load_draft()).map(Outcome::Resumed).map_err(|e|e.to_string()));
                }
                if ui.add_enabled(idle && !self.root.trim().is_empty(),egui::Button::new("Compare saved replacements with current files")).clicked() {
                    self.invalidate(); let root=self.root.clone();
                    self.spawn(false,move || Workspace::open(Path::new(&root)).and_then(|w|w.review_saved_draft()).map(Outcome::Recovery).map_err(|e|e.to_string()));
                }
                if let Some(review)=&self.recovery {
                    ui.label(format!("Recovery comparison of saved draft {} (no source writes or rebase)",review.id));
                    for (index,file) in review.files.iter().enumerate() {
                        if ui.add_enabled(idle,egui::Button::new(format!("{} — baseline matches: {}",file.edit.path,file.matches_baseline)).selected(self.recovery_index == Some(index))).clicked() { self.recovery_index=Some(index); self.accept_recovery=false; }
                    }
                    if let Some(file)=self.recovery_index.and_then(|index|review.files.get(index)) {
                        ui.label(format!("Saved before hash: {}",file.edit.before_hash.as_deref().unwrap_or("path expected absent")));
                        ui.label(format!("Current hash: {}",file.current.as_ref().map(|c|c.hash.as_str()).unwrap_or("path currently absent")));
                        ui.label("Current source"); let mut current=file.current.as_ref().map(|c|c.content.as_str()).unwrap_or("(path currently absent)");
                        ui.add(egui::TextEdit::multiline(&mut current).code_editor().desired_rows(5).desired_width(f32::INFINITY).interactive(false));
                        ui.label("Saved replacement (has not been merged with current source)"); let mut replacement=file.edit.replacement.as_str();
                        ui.add(egui::TextEdit::multiline(&mut replacement).code_editor().desired_rows(5).desired_width(f32::INFINITY).interactive(false));
                        ui.add_enabled(idle,egui::Checkbox::new(&mut self.accept_recovery,"Replace the editor text and use this displayed current file state as the repair baseline"));
                    }
                }
                if ui.add_enabled(idle && self.accept_recovery,egui::Button::new("Load selected replacement for manual repair")).clicked() { if let Err(error)=self.recover_file() { self.status=error; } }
                let mut remove=None;
                if let Some(info)=&self.saved_info {
                    ui.label(format!("Stored draft ID: {}",info.id));
                    for path in &info.paths { ui.monospace(path); }
                    ui.label("Remove only the stored draft by typing its exact ID. This leaves source and in-memory edits intact, and can remove a stale draft.");
                    ui.add_enabled(idle,egui::TextEdit::singleline(&mut self.delete_confirmation));
                    if ui.add_enabled(idle && self.delete_confirmation == info.id,egui::Button::new("Remove this stored draft")).clicked() { remove=Some(info.id.clone()); }
                }
                if let Some(id)=remove {
                    self.delete_confirmation.clear(); let root=self.root.clone();
                    self.spawn(true,move || { Workspace::open(Path::new(&root)).and_then(|w|w.clear_draft(&id)).map_err(|e|e.to_string())?; Ok(Outcome::Cleared(id)) });
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
            if ui.add_enabled(idle,egui::Button::new("Discard in-memory edits and review")).clicked() { self.discard_memory(); self.status = "Unapplied edit drafts discarded.".into(); }
            if let Some(pending) = &self.pending { ui.label(if pending.writing { "Completing the requested local write; do not close the application." } else { "Reading/validating locally…" }); }
            ui.label(&self.status);
        });
        if self.pending.is_some() {
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
            writing: false,
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
    #[test]
    fn resumed_edits_never_restore_apply_authorization_or_replace_without_acknowledgement() {
        let mut panel = loaded_panel();
        panel.stage().unwrap();
        assert!(!panel.can_resume());
        panel.replace_unsaved = true;
        assert!(panel.can_resume());
        panel.confirmation = "old-authorization".into();
        let edit = PatchEdit {
            path: "restored.rs".into(),
            before_hash: Some(mini_resident::content_hash(b"baseline")),
            replacement: "saved edit".into(),
        };
        panel.resume(SavedDraft {
            preview: PatchPreview {
                id: "saved-id".into(),
                edits: vec![edit],
                provenance: Default::default(),
            },
            originals: vec![ContextItem {
                path: "restored.rs".into(),
                content: "baseline".into(),
                hash: mini_resident::content_hash(b"baseline"),
            }],
        });
        assert_eq!(panel.edits.len(), 1);
        assert_eq!(panel.edits[0].edit.path, "restored.rs");
        assert_eq!(panel.edits[0].original.as_deref(), Some("baseline"));
        assert!(panel.preview.is_none() && panel.confirmation.is_empty());
        assert!(!panel.replace_unsaved && panel.can_resume());
        assert_eq!(panel.saved_info.as_ref().unwrap().id, "saved-id");
        panel.sync_root("other");
        assert!(panel.saved_info.is_none() && panel.delete_confirmation.is_empty());
    }
    #[test]
    fn editing_a_resumed_file_keeps_the_original_hash_and_requires_restaging() {
        let mut panel = loaded_panel();
        panel.stage().unwrap();
        panel.replacement = "unrelated unsaved text".into();
        panel.replacement_dirty = true;
        panel.edit_staged(0);
        assert!(panel.pending_editor.is_some());
        panel.resolve_editor(true);
        assert_eq!(panel.replacement, "after");
        panel.replacement = "revised draft".into();
        assert_eq!(panel.edits[0].edit.replacement, "after");
        panel.stage().unwrap();
        assert_eq!(
            panel.edits[0].edit.before_hash.as_deref(),
            Some(mini_resident::content_hash(b"before").as_str())
        );
        assert_eq!(panel.edits[0].edit.replacement, "revised draft");
        assert!(panel.preview.is_none() && panel.confirmation.is_empty());
    }
    #[test]
    fn close_guard_distinguishes_saved_staging_from_unsaved_editor_text() {
        let mut panel = loaded_panel();
        panel.stage().unwrap();
        assert!(panel.has_unsaved_work() && panel.request_close());
        panel.close_requested = false;
        panel.saved_edits = Some(panel.edits.iter().map(|e| e.edit.clone()).collect());
        assert!(!panel.has_unsaved_work() && !panel.request_close());
        panel.replacement_dirty = true;
        panel
            .loaded
            .as_mut()
            .unwrap()
            .original
            .as_mut()
            .unwrap()
            .hash = mini_resident::content_hash(b"new baseline");
        assert!(
            panel.editor_unstaged(),
            "accepted new baseline must be staged even if replacement text is unchanged"
        );
        panel
            .loaded
            .as_mut()
            .unwrap()
            .original
            .as_mut()
            .unwrap()
            .hash = mini_resident::content_hash(b"before");
        panel.replacement_dirty = true;
        panel.replacement = "new unstaged text".into();
        assert!(panel.editor_unstaged() && panel.request_close());
        panel.stage().unwrap();
        assert!(
            panel.has_unsaved_work(),
            "old saved checkpoint must not cover newly staged text"
        );
        panel.discard_memory();
        assert!(!panel.has_unsaved_work());
    }
    #[test]
    fn stale_repair_requires_acceptance_and_only_loads_an_unstaged_current_baseline() {
        use mini_resident::DraftFileReview;
        let old_hash = mini_resident::content_hash(b"old");
        let current_hash = mini_resident::content_hash(b"owners new version");
        let mut panel = Panel::default();
        panel.recovery = Some(DraftReview {
            id: "stored-id".into(),
            files: vec![DraftFileReview {
                edit: PatchEdit {
                    path: "lib.rs".into(),
                    before_hash: Some(old_hash),
                    replacement: "saved replacement".into(),
                },
                current: Some(ContextItem {
                    path: "lib.rs".into(),
                    content: "owners new version".into(),
                    hash: current_hash.clone(),
                }),
                matches_baseline: false,
            }],
        });
        panel.recovery_index = Some(0);
        assert!(panel.recover_file().is_err());
        panel.accept_recovery = true;
        panel.recover_file().unwrap();
        assert!(panel.edits.is_empty() && panel.preview.is_none() && panel.confirmation.is_empty());
        assert_eq!(panel.replacement, "saved replacement");
        assert!(!panel.create && panel.has_unsaved_work());
        panel.stage().unwrap();
        assert_eq!(
            panel.edits[0].edit.before_hash.as_deref(),
            Some(current_hash.as_str())
        );
        assert_eq!(
            panel.edits[0].original.as_deref(),
            Some("owners new version")
        );
        assert!(panel.recovery.is_none() && !panel.accept_recovery);
    }
    #[test]
    fn selecting_model_text_cannot_silently_replace_unstaged_editor_work() {
        let mut panel = loaded_panel();
        panel.replacement_dirty = true;
        panel.draft_reply("workspace", "new model suggestion");
        assert_eq!(panel.replacement, "after");
        assert!(panel.pending_editor.is_some() && panel.is_busy());
        panel.resolve_editor(false);
        assert_eq!(panel.replacement, "after");
        panel.draft_reply("workspace", "new model suggestion");
        panel.resolve_editor(true);
        assert_eq!(panel.replacement, "new model suggestion");
        assert!(panel.edits.is_empty() && panel.preview.is_none());
        assert!(panel.has_unsaved_work());
    }
}
