//! Explicit owner-local draft persistence. A saved draft conveys no approval;
//! load revalidates source and every apply still needs a new exact confirmation.
use crate::{
    content_hash, validate_context_path, AiProvenance, ContextItem, PatchEdit, PatchPreview,
    ResidentError, Result, Workspace, MAX_CONTEXT_BYTES,
};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const NAME: &str = ".resident-draft.json";
const MAX_DRAFT_BYTES: usize = 2 * 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedDraft {
    pub preview: PatchPreview,
    /// Freshly read source, not deserialized or trusted draft-file content.
    pub originals: Vec<ContextItem>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftInfo {
    pub id: String,
    pub paths: Vec<String>,
}
/// Recovery-only source comparison. This is not an apply-ready PatchPreview
/// and conveys no approval, rebasing or source modification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftReview {
    pub id: String,
    pub files: Vec<DraftFileReview>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftFileReview {
    pub edit: PatchEdit,
    pub current: Option<ContextItem>,
    pub matches_baseline: bool,
}
enum SaveCondition<'a> {
    Overwrite,
    Create,
    Replace(&'a str),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u8,
    root: String,
    preview: PatchPreview,
}

fn check_file(path: &Path) -> Result<Option<fs::Metadata>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_DRAFT_BYTES as u64
    {
        return Err(ResidentError(
            "saved draft must be a bounded regular file, never a symlink".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.permissions().mode() & 0o077 != 0 || metadata.nlink() != 1 {
            return Err(ResidentError(
                "saved draft must have private Unix permissions and no hard links".into(),
            ));
        }
    }
    Ok(Some(metadata))
}
fn canonical_root(workspace: &Workspace) -> Result<&str> {
    workspace
        .root_path()
        .to_str()
        .ok_or_else(|| ResidentError("saved drafts require a UTF-8 workspace path".into()))
}
fn validate_document(workspace: &Workspace, document: &Document) -> Result<()> {
    if document.version != 1
        || document.root != canonical_root(workspace)?
        || document.preview.provenance != AiProvenance::default()
    {
        return Err(ResidentError(
            "saved draft version, workspace root or AI provenance differs".into(),
        ));
    }
    let edits = &document.preview.edits;
    if edits.is_empty() || edits.len() > 16 {
        return Err(ResidentError("saved draft requires 1–16 edits".into()));
    }
    let mut paths = std::collections::BTreeSet::new();
    let mut bytes = 0usize;
    for edit in edits {
        validate_context_path(&edit.path)?;
        bytes = bytes
            .checked_add(edit.replacement.len())
            .ok_or_else(|| ResidentError("saved draft byte overflow".into()))?;
        if !paths.insert(&edit.path)
            || bytes > MAX_CONTEXT_BYTES
            || edit.replacement.contains('\0')
            || edit.before_hash.as_ref().is_some_and(|hash| {
                hash.len() != 64
                    || !hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
        {
            return Err(ResidentError(
                "saved draft has invalid edits, hashes or byte limits".into(),
            ));
        }
    }
    let binding = serde_json::to_vec(&(workspace.root_path().to_string_lossy(), edits))?;
    if document.preview.id != content_hash(&binding) {
        return Err(ResidentError(
            "saved draft preview content hash differs".into(),
        ));
    }
    Ok(())
}
fn read_document(workspace: &Workspace) -> Result<Document> {
    let path = workspace.root_path().join(NAME);
    if check_file(&path)?.is_none() {
        return Err(ResidentError(
            "no saved Resident draft exists in this workspace".into(),
        ));
    }
    let mut bytes = Vec::new();
    let file = File::open(&path)?;
    // Check the opened handle too, so an unexpected replaced/oversized file
    // fails before parsing. The owner controls this directory; these checks
    // do not promise atomic protection against hostile directory replacement.
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_DRAFT_BYTES as u64 {
        return Err(ResidentError(
            "saved draft file changed while opening".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.permissions().mode() & 0o077 != 0 || metadata.nlink() != 1 {
            return Err(ResidentError(
                "opened saved draft is not a private single-link file".into(),
            ));
        }
    }
    file.take(MAX_DRAFT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_DRAFT_BYTES {
        return Err(ResidentError("saved draft exceeds 2 MiB".into()));
    }
    let document: Document = serde_json::from_slice(&bytes)?;
    validate_document(workspace, &document)?;
    Ok(document)
}
fn fresh_draft(workspace: &Workspace, preview: PatchPreview) -> Result<SavedDraft> {
    let checked = workspace.preview_patch(&preview.edits)?;
    if preview != checked {
        return Err(ResidentError(
            "saved draft differs from the exact current patch preview".into(),
        ));
    }
    let paths: Vec<_> = preview
        .edits
        .iter()
        .filter(|edit| edit.before_hash.is_some())
        .map(|edit| edit.path.clone())
        .collect();
    // Up to sixteen independently bounded originals may exceed the model's
    // aggregate context allowance. They remain local review text, never an
    // implicit model attachment.
    let mut originals = Vec::new();
    for path in &paths {
        originals.extend(workspace.context(std::slice::from_ref(path))?);
    }
    for original in &originals {
        if !preview.edits.iter().any(|edit| {
            edit.path == original.path
                && edit.before_hash.as_deref() == Some(original.hash.as_str())
        }) {
            return Err(ResidentError(
                "source changed while resuming saved draft".into(),
            ));
        }
    }
    if workspace.preview_patch(&preview.edits)? != preview {
        return Err(ResidentError(
            "source changed while reading draft review context".into(),
        ));
    }
    Ok(SavedDraft { preview, originals })
}
struct PrivateStage(PathBuf);
impl Drop for PrivateStage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn write_document(workspace: &Workspace, bytes: &[u8]) -> Result<()> {
    let destination = workspace.root_path().join(NAME);
    let replacing = check_file(&destination)?.is_some();
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let stage_path = workspace.root_path().join(format!(
        ".resident-draft-{}-{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&stage_path)?;
    let stage = PrivateStage(stage_path);
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    // Recheck owner-controlled target shape immediately before publication.
    // No permissive-mode temporary file is ever created: atomic_replace's
    // default creation mode is unsuitable for private source drafts.
    check_file(&destination)?;
    // Publish through mini-durable's own commit primitives rather than a raw
    // rename/hard_link followed by a direct sync_parent: directory fsync is
    // Unix-only (see mini-durable's lib.rs), and these helpers already carry
    // the Windows write-through equivalent plus the Unix directory barrier.
    if replacing {
        mini_durable::publish_staged_file(&stage.0, &destination)?;
    } else {
        mini_durable::publish_staged_file_new(&stage.0, &destination)?;
    }
    Ok(())
}
impl Workspace {
    /// Save an explicitly selected current draft. This writes no source file
    /// and persists no owner confirmation. Existing saved draft replacement is
    /// atomic where supported; errors after publication may have committed.
    pub fn save_draft(&self, edits: &[PatchEdit]) -> Result<SavedDraft> {
        self.save_draft_inner(edits, SaveCondition::Overwrite)
    }
    /// Compare-and-save under the shared patch writer lock. None means create
    /// only; Some(id) replaces only that exact structurally valid stored draft.
    /// The source still must match the newly submitted edits. This coordinates
    /// cooperating writers; it does not serialize arbitrary filesystem editors.
    pub fn save_draft_if(&self, edits: &[PatchEdit], expected: Option<&str>) -> Result<SavedDraft> {
        self.save_draft_inner(
            edits,
            match expected {
                None => SaveCondition::Create,
                Some(id) => SaveCondition::Replace(id),
            },
        )
    }
    fn save_draft_inner(
        &self,
        edits: &[PatchEdit],
        condition: SaveCondition<'_>,
    ) -> Result<SavedDraft> {
        let _guard = self.patch_lock()?;
        match condition {
            SaveCondition::Overwrite => (),
            SaveCondition::Create => {
                if check_file(&self.root_path().join(NAME))?.is_some() {
                    return Err(ResidentError(
                        "saved draft already exists; inspect its exact ID before replacing it"
                            .into(),
                    ));
                }
            }
            SaveCondition::Replace(expected) => {
                if read_document(self)?.preview.id != expected {
                    return Err(ResidentError(
                        "saved draft changed; inspect it again before replacement".into(),
                    ));
                }
            }
        }
        let saved = fresh_draft(self, self.preview_patch(edits)?)?;
        let document = Document {
            version: 1,
            root: canonical_root(self)?.into(),
            preview: saved.preview.clone(),
        };
        let bytes = serde_json::to_vec(&document)?;
        if bytes.len() > MAX_DRAFT_BYTES {
            return Err(ResidentError("saved draft exceeds 2 MiB".into()));
        }
        write_document(self, &bytes)?;
        Ok(saved)
    }
    /// Resume only if the exact workspace, preview and source before hashes
    /// still match. Stale or tampered files remain untouched for owner review.
    pub fn load_draft(&self) -> Result<SavedDraft> {
        let _guard = self.patch_lock()?;
        fresh_draft(self, read_document(self)?.preview)
    }
    /// Recover saved replacement text with current local source snapshots.
    /// Baseline mismatches are reported, never rebased. This performs no source
    /// writes and returns no apply-ready preview or persisted confirmation.
    pub fn review_saved_draft(&self) -> Result<DraftReview> {
        let _guard = self.patch_lock()?;
        let document = read_document(self)?;
        let mut files = Vec::with_capacity(document.preview.edits.len());
        for edit in document.preview.edits {
            let current = self.optional_context(&edit.path)?;
            let matches_baseline = match (&edit.before_hash, &current) {
                (None, None) => true,
                (Some(expected), Some(current)) => expected == &current.hash,
                _ => false,
            };
            files.push(DraftFileReview {
                edit,
                current,
                matches_baseline,
            });
        }
        Ok(DraftReview {
            id: document.preview.id,
            files,
        })
    }
    /// Inspect structurally verified draft metadata without implying that the
    /// current source still matches. This enables exact-ID stale-draft removal.
    pub fn draft_info(&self) -> Result<DraftInfo> {
        let _guard = self.patch_lock()?;
        let document = read_document(self)?;
        Ok(DraftInfo {
            id: document.preview.id,
            paths: document
                .preview
                .edits
                .into_iter()
                .map(|edit| edit.path)
                .collect(),
        })
    }
    /// Delete only the exact stored draft ID. Source may be stale; the stored
    /// root, schema, provenance and content-derived ID must still be valid.
    pub fn clear_draft(&self, expected_id: &str) -> Result<()> {
        let _guard = self.patch_lock()?;
        let document = read_document(self)?;
        if expected_id != document.preview.id {
            return Err(ResidentError(
                "draft removal requires its exact inspected ID".into(),
            ));
        }
        let path = self.root_path().join(NAME);
        // mini_durable::remove_file durably removes on both platforms; a raw
        // fs::remove_file plus sync_parent is Unix-only (directory fsync is
        // unsupported on Windows, see mini-durable's lib.rs).
        mini_durable::remove_file(&path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn edits() -> Vec<PatchEdit> {
        vec![
            PatchEdit {
                path: "code.rs".into(),
                before_hash: Some(content_hash(b"original")),
                replacement: "proposed".into(),
            },
            PatchEdit {
                path: "new.rs".into(),
                before_hash: None,
                replacement: "new source".into(),
            },
        ]
    }
    #[test]
    fn explicit_draft_roundtrip_keeps_source_unmodified_and_originals_fresh() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("code.rs"), "original").unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        let saved = workspace.save_draft(&edits()).unwrap();
        assert_eq!(workspace.load_draft().unwrap(), saved);
        assert_eq!(saved.originals.len(), 1);
        assert_eq!(saved.originals[0].content, "original");
        assert_eq!(saved.originals[0].hash, content_hash(b"original"));
        assert_eq!(fs::read(dir.path().join("code.rs")).unwrap(), b"original");
        assert!(!dir.path().join("new.rs").exists());
        assert_eq!(workspace.inspect().unwrap().files.len(), 1);
        let document: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.path().join(NAME)).unwrap()).unwrap();
        assert!(document.get("confirmation").is_none() && document.get("originals").is_none());
        // Existing local review text is per-file bounded, independently of the
        // model attachment's aggregate context allowance.
        let large = "x".repeat(140 * 1024);
        fs::write(dir.path().join("a.rs"), &large).unwrap();
        fs::write(dir.path().join("b.rs"), &large).unwrap();
        let large_edits: Vec<_> = ["a.rs", "b.rs"]
            .into_iter()
            .map(|path| PatchEdit {
                path: path.into(),
                before_hash: Some(content_hash(large.as_bytes())),
                replacement: "small".into(),
            })
            .collect();
        let saved = workspace.save_draft(&large_edits).unwrap();
        assert!(
            saved
                .originals
                .iter()
                .map(|item| item.content.len())
                .sum::<usize>()
                > MAX_CONTEXT_BYTES
        );
        assert_eq!(workspace.load_draft().unwrap(), saved);
    }
    #[test]
    fn stale_load_and_save_preserve_draft_and_exact_metadata_can_delete_it() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("code.rs"), "original").unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        let saved = workspace.save_draft(&edits()).unwrap();
        let bytes = fs::read(dir.path().join(NAME)).unwrap();
        fs::write(dir.path().join("code.rs"), "owner edited after saving").unwrap();
        assert!(workspace.load_draft().is_err());
        assert!(workspace.save_draft(&edits()).is_err());
        assert_eq!(fs::read(dir.path().join(NAME)).unwrap(), bytes);
        assert_eq!(
            workspace.draft_info().unwrap(),
            DraftInfo {
                id: saved.preview.id.clone(),
                paths: vec!["code.rs".into(), "new.rs".into()]
            }
        );
        assert!(workspace.clear_draft("yes").is_err());
        assert_eq!(fs::read(dir.path().join(NAME)).unwrap(), bytes);
        workspace.clear_draft(&saved.preview.id).unwrap();
        assert!(!dir.path().join(NAME).exists());
        assert_eq!(
            fs::read(dir.path().join("code.rs")).unwrap(),
            b"owner edited after saving"
        );
    }
    #[test]
    fn foreign_and_tampered_drafts_are_never_resumed_or_removed_as_valid() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        for dir in [&first, &second] {
            fs::write(dir.path().join("code.rs"), "original").unwrap();
        }
        let workspace = Workspace::open(first.path()).unwrap();
        let saved = workspace.save_draft(&edits()).unwrap();
        let path = first.path().join(NAME);
        let bytes = fs::read(&path).unwrap();
        fs::copy(&path, second.path().join(NAME)).unwrap();
        let foreign = Workspace::open(second.path()).unwrap();
        assert!(foreign.load_draft().is_err());
        assert!(foreign.draft_info().is_err());
        assert!(foreign.clear_draft(&saved.preview.id).is_err());
        for mutation in ["text", "provenance", "unknown", "version"] {
            let mut document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            match mutation {
                "text" => document["preview"]["edits"][0]["replacement"] = "tampered".into(),
                "provenance" => document["preview"]["provenance"]["human_approval"] = true.into(),
                "unknown" => document["confirmation"] = saved.preview.id.clone().into(),
                _ => document["version"] = 2.into(),
            }
            fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
            let tampered = fs::read(&path).unwrap();
            assert!(workspace.load_draft().is_err(), "accepted {mutation}");
            assert!(
                workspace.draft_info().is_err(),
                "inspected {mutation} as valid"
            );
            assert!(workspace.clear_draft(&saved.preview.id).is_err());
            assert_eq!(fs::read(&path).unwrap(), tampered);
        }
    }
    #[cfg(unix)]
    #[test]
    fn draft_files_are_private_and_symlinks_and_hardlinks_are_refused() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("code.rs"), "original").unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        let saved = workspace.save_draft(&edits()).unwrap();
        let path = dir.path().join(NAME);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        workspace.save_draft(&edits()).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let bytes = fs::read(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(workspace.load_draft().is_err());
        assert!(workspace.save_draft(&edits()).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_file(&path).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("private-draft");
        fs::write(&target, &bytes).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&target, &path).unwrap();
        assert!(workspace.save_draft(&edits()).is_err());
        assert!(workspace.load_draft().is_err());
        assert!(workspace.clear_draft(&saved.preview.id).is_err());
        assert_eq!(fs::read(&target).unwrap(), bytes);
        fs::remove_file(&path).unwrap();
        fs::hard_link(&target, &path).unwrap();
        assert!(workspace.load_draft().is_err());
        assert!(workspace.save_draft(&edits()).is_err());
        assert_eq!(fs::read(&target).unwrap(), bytes);
    }
    #[test]
    fn draft_operations_share_the_patch_writer_lock() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("code.rs"), "original").unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        let saved = workspace.save_draft(&edits()).unwrap();
        let bytes = fs::read(dir.path().join(NAME)).unwrap();
        let _guard = workspace.patch_lock().unwrap();
        assert!(workspace.save_draft(&edits()).is_err());
        assert!(workspace.load_draft().is_err());
        assert!(workspace
            .save_draft_if(&edits(), Some(&saved.preview.id))
            .is_err());
        assert!(workspace.review_saved_draft().is_err());
        assert!(workspace.draft_info().is_err());
        assert!(workspace.clear_draft(&saved.preview.id).is_err());
        assert_eq!(fs::read(dir.path().join(NAME)).unwrap(), bytes);
    }
    #[test]
    fn compare_and_save_refuses_stale_or_missing_expectations_and_serializes_races() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("code.rs"), "original").unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        assert!(workspace
            .save_draft_if(&edits(), Some("missing-id"))
            .is_err());
        assert!(!dir.path().join(NAME).exists());
        let first = workspace.save_draft_if(&edits(), None).unwrap();
        assert!(workspace.save_draft_if(&edits(), None).is_err());
        let mut replacement = edits();
        replacement[0].replacement = "second draft".into();
        let second = workspace
            .save_draft_if(&replacement, Some(&first.preview.id))
            .unwrap();
        let bytes = fs::read(dir.path().join(NAME)).unwrap();
        assert!(workspace
            .save_draft_if(&edits(), Some(&first.preview.id))
            .is_err());
        assert_eq!(fs::read(dir.path().join(NAME)).unwrap(), bytes);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let mut tasks = Vec::new();
        for text in ["concurrent one", "concurrent two"] {
            let (workspace, expected, barrier) = (
                workspace.clone(),
                second.preview.id.clone(),
                barrier.clone(),
            );
            let mut replacement = edits();
            replacement[0].replacement = text.into();
            tasks.push(std::thread::spawn(move || {
                barrier.wait();
                workspace.save_draft_if(&replacement, Some(&expected))
            }));
        }
        let outcomes: Vec<_> = tasks.into_iter().map(|task| task.join().unwrap()).collect();
        assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
        let winner = outcomes
            .into_iter()
            .find_map(std::result::Result::ok)
            .unwrap();
        assert_eq!(workspace.load_draft().unwrap(), winner);
        assert_eq!(fs::read(dir.path().join("code.rs")).unwrap(), b"original");
        // Explicit legacy callers retain their intentionally unconditional save.
        assert_eq!(
            workspace.save_draft(&edits()).unwrap().preview,
            first.preview
        );
    }

    #[test]
    fn stale_review_preserves_replacements_and_reports_current_missing_and_new_collisions() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("code.rs"), "original").unwrap();
        fs::write(dir.path().join("removed.rs"), "old removed source").unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        let changes = vec![
            edits()[0].clone(),
            PatchEdit {
                path: "removed.rs".into(),
                before_hash: Some(content_hash(b"old removed source")),
                replacement: "replacement for removed".into(),
            },
            PatchEdit {
                path: "collision.rs".into(),
                before_hash: None,
                replacement: "planned creation".into(),
            },
            PatchEdit {
                path: "absent.rs".into(),
                before_hash: None,
                replacement: "still absent".into(),
            },
        ];
        let saved = workspace.save_draft_if(&changes, None).unwrap();
        let stored = fs::read(dir.path().join(NAME)).unwrap();
        fs::write(dir.path().join("code.rs"), "owner revision").unwrap();
        fs::remove_file(dir.path().join("removed.rs")).unwrap();
        fs::write(dir.path().join("collision.rs"), "owner created this").unwrap();
        let review = workspace.review_saved_draft().unwrap();
        assert_eq!(review.id, saved.preview.id);
        assert_eq!(
            review
                .files
                .iter()
                .map(|file| file.edit.clone())
                .collect::<Vec<_>>(),
            changes
        );
        assert!(!review.files[0].matches_baseline);
        assert_eq!(
            review.files[0].current.as_ref().unwrap().content,
            "owner revision"
        );
        assert_eq!(
            review.files[0].current.as_ref().unwrap().hash,
            content_hash(b"owner revision")
        );
        assert!(review.files[1].current.is_none());
        assert!(!review.files[1].matches_baseline);
        assert_eq!(
            review.files[2].current.as_ref().unwrap().content,
            "owner created this"
        );
        assert!(!review.files[2].matches_baseline);
        assert!(review.files[3].current.is_none() && review.files[3].matches_baseline);
        assert!(workspace.load_draft().is_err());
        assert!(workspace
            .apply_patch(&saved.preview, &saved.preview.id)
            .is_err());
        assert_eq!(fs::read(dir.path().join(NAME)).unwrap(), stored);
        assert_eq!(
            fs::read(dir.path().join("code.rs")).unwrap(),
            b"owner revision"
        );
        assert!(!dir.path().join("absent.rs").exists());
    }

    #[test]
    fn draft_review_does_not_treat_missing_parents_or_nonregular_source_as_missing_final_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/code.rs"), "original").unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        workspace
            .save_draft(&[PatchEdit {
                path: "sub/code.rs".into(),
                before_hash: Some(content_hash(b"original")),
                replacement: "proposal".into(),
            }])
            .unwrap();
        fs::remove_file(dir.path().join("sub/code.rs")).unwrap();
        fs::remove_dir(dir.path().join("sub")).unwrap();
        assert!(workspace.review_saved_draft().is_err());
        fs::create_dir_all(dir.path().join("sub/code.rs")).unwrap();
        assert!(workspace.review_saved_draft().is_err());
        fs::remove_dir(dir.path().join("sub/code.rs")).unwrap();
        fs::write(dir.path().join("sub/code.rs"), vec![b'x'; 256 * 1024 + 1]).unwrap();
        assert!(workspace.review_saved_draft().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn stale_review_refuses_a_source_replaced_with_an_outside_symlink() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("code.rs"), "original").unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        workspace.save_draft(&edits()).unwrap();
        let stored = fs::read(dir.path().join(NAME)).unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("other"), "not granted").unwrap();
        fs::remove_file(dir.path().join("code.rs")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("other"), dir.path().join("code.rs"))
            .unwrap();
        assert!(workspace
            .review_saved_draft()
            .unwrap_err()
            .0
            .contains("symlinks"));
        assert_eq!(fs::read(dir.path().join(NAME)).unwrap(), stored);
        assert_eq!(
            fs::read(outside.path().join("other")).unwrap(),
            b"not granted"
        );
    }
}
