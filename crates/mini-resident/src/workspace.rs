use crate::{
    content_hash, AiProvenance, ContextItem, ResidentError, Result, MAX_CONTEXT_BYTES,
    MAX_CONTEXT_FILES,
};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

const MAX_ENTRIES: usize = 20_000;
const MAX_DEPTH: usize = 32;
const MAX_SEARCH_BYTES: usize = 16 * 1024 * 1024;
const MAX_SEARCH_HITS: usize = 100;
const MAX_FILE_BYTES: usize = 256 * 1024;

fn blocked(segment: &str) -> bool {
    let s = segment.to_ascii_lowercase();
    matches!(
        s.as_str(),
        ".git"
            | ".config"
            | ".npmrc"
            | ".netrc"
            | ".pypirc"
            | ".mini"
            | ".mininet"
            | ".ssh"
            | ".gnupg"
            | "node_modules"
            | "target"
            | "vendor"
            | "credentials"
            | "secrets"
            | "identity"
            | "identities"
            | "wallet"
            | "wallets"
            | "resident-provider.json"
    ) || s.starts_with(".resident")
        || s.starts_with(".env")
        || s.contains("seed")
        || s.contains("secret")
        || s.contains("credential")
        || s.starts_with("id_rsa")
        || s.starts_with("id_ed25519")
        || s.contains("access_token")
        || s.contains("api_key")
        || s.contains("private_key")
        || s.contains("password")
        || s.ends_with(".pem")
        || s.ends_with(".key")
        || s.ends_with(".p12")
        || s == "config"
        || s == "config.json"
        || s == "config.toml"
}
pub fn validate_context_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > 1024
        || path.contains('\\')
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == ".." || blocked(s))
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(ResidentError(
            "context path must be a relative source path without metadata or secrets".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub bytes: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInspection {
    pub root: String,
    pub files: Vec<SourceFile>,
    pub excluded_entries: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSummary {
    pub path: String,
    pub bytes: u64,
    pub hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub path: String,
    pub line: usize,
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchEdit {
    pub path: String,
    pub before_hash: Option<String>,
    pub replacement: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchPreview {
    pub id: String,
    pub edits: Vec<PatchEdit>,
    pub provenance: AiProvenance,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchReceipt {
    pub preview_id: String,
    pub patched_paths: Vec<String>,
    pub provenance: AiProvenance,
}

impl Workspace {
    pub fn open(root: &Path) -> Result<Self> {
        // The grant is the exact owner-selected directory, not an ancestor of
        // a symlink that might disclose another workspace or identity store.
        let absolute = if root.is_absolute() {
            root.to_path_buf()
        } else {
            std::env::current_dir()?.join(root)
        };
        let mut part = PathBuf::new();
        for c in absolute.components() {
            if matches!(c, Component::Normal(name) if name.to_str().is_none_or(blocked)) {
                return Err(ResidentError(
                    "metadata and private configuration directories cannot be workspace grants"
                        .into(),
                ));
            }
            part.push(c);
            if fs::symlink_metadata(&part)?.file_type().is_symlink() {
                return Err(ResidentError(
                    "workspace roots cannot traverse symlinks".into(),
                ));
            }
        }
        let root = fs::canonicalize(absolute)?;
        if !root.is_dir() {
            return Err(ResidentError("workspace must be a directory".into()));
        }
        Ok(Self { root })
    }
    fn path(&self, relative: &str, allow_missing: bool) -> Result<PathBuf> {
        validate_context_path(relative)?;
        let mut full = self.root.clone();
        let components: Vec<_> = Path::new(relative).components().collect();
        for (i, c) in components.iter().enumerate() {
            full.push(c);
            match fs::symlink_metadata(&full) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err(ResidentError("workspace symlinks are excluded".into()))
                }
                Ok(meta) if i + 1 < components.len() && !meta.is_dir() => {
                    return Err(ResidentError("path parent is not a directory".into()))
                }
                Ok(_) => (),
                Err(e)
                    if allow_missing
                        && i + 1 == components.len()
                        && e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(full)
    }
    fn read(&self, relative: &str) -> Result<String> {
        let path = self.path(relative, false)?;
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES as u64 {
            return Err(ResidentError(
                "context requires a regular text file of at most 256 KiB".into(),
            ));
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(MAX_FILE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_FILE_BYTES || bytes.contains(&0) {
            return Err(ResidentError("binary or oversized context file".into()));
        }
        String::from_utf8(bytes).map_err(|_| ResidentError("context file is not UTF-8".into()))
    }
    /// A review-only snapshot. Only a genuinely absent final component is
    /// represented as None; missing parents and rejected paths remain errors.
    pub(crate) fn optional_context(&self, relative: &str) -> Result<Option<ContextItem>> {
        let path = self.path(relative, true)?;
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
            Ok(_) => self
                .context(&[relative.to_owned()])?
                .into_iter()
                .next()
                .map(Some)
                .ok_or_else(|| ResidentError("source review returned no context".into())),
        }
    }
    /// Hash an explicitly granted regular file for local hosting planning.
    /// Bytes are streamed locally and never attached to a model request.
    pub fn file_summary(&self, relative: &str) -> Result<FileSummary> {
        const LIMIT: u64 = 64 * 1024 * 1024;
        let path = self.path(relative, false)?;
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.len() > LIMIT {
            return Err(ResidentError(
                "hosting summary requires a regular file of at most 64 MiB".into(),
            ));
        }
        let mut file = fs::File::open(path)?;
        let mut bytes = 0u64;
        let mut hasher = blake3::Hasher::new();
        let mut buffer = [0u8; 16 * 1024];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            bytes += n as u64;
            if bytes > LIMIT {
                return Err(ResidentError(
                    "file grew beyond hosting summary limit".into(),
                ));
            }
            hasher.update(&buffer[..n]);
        }
        Ok(FileSummary {
            path: relative.into(),
            bytes,
            hash: hasher.finalize().to_hex().to_string(),
        })
    }

    pub fn context(&self, paths: &[String]) -> Result<Vec<ContextItem>> {
        if paths.len() > MAX_CONTEXT_FILES {
            return Err(ResidentError("at most 32 files may be attached".into()));
        }
        let mut bytes = 0;
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for path in paths {
            if !seen.insert(path) {
                return Err(ResidentError("duplicate context path".into()));
            }
            let content = self.read(path)?;
            bytes += content.len();
            if bytes > MAX_CONTEXT_BYTES {
                return Err(ResidentError("attached context exceeds 256 KiB".into()));
            }
            out.push(ContextItem {
                path: path.clone(),
                hash: content_hash(content.as_bytes()),
                content,
            });
        }
        Ok(out)
    }
    pub fn inspect(&self) -> Result<WorkspaceInspection> {
        let mut out = WorkspaceInspection {
            root: self.root.display().to_string(),
            files: vec![],
            excluded_entries: 0,
        };
        let mut remaining = MAX_ENTRIES;
        self.walk(&self.root, 0, &mut remaining, &mut out)?;
        out.files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }
    fn walk(
        &self,
        dir: &Path,
        depth: usize,
        budget: &mut usize,
        out: &mut WorkspaceInspection,
    ) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err(ResidentError("workspace exceeds 32-directory depth".into()));
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(dir)? {
            if *budget == 0 {
                return Err(ResidentError(
                    "workspace exceeds 20000-entry inspection budget".into(),
                ));
            }
            *budget -= 1;
            entries.push(entry?);
        }
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                out.excluded_entries += 1;
                continue;
            };
            let meta = fs::symlink_metadata(entry.path())?;
            if blocked(name) || meta.file_type().is_symlink() {
                out.excluded_entries += 1;
                continue;
            }
            if meta.is_dir() {
                self.walk(&entry.path(), depth + 1, budget, out)?;
            } else if meta.is_file() {
                let path = entry
                    .path()
                    .strip_prefix(&self.root)
                    .map_err(|e| ResidentError(e.to_string()))?
                    .to_string_lossy()
                    .replace('\\', "/");
                validate_context_path(&path)?;
                out.files.push(SourceFile {
                    path,
                    bytes: meta.len(),
                });
            } else {
                out.excluded_entries += 1;
            }
        }
        Ok(())
    }
    /// Literal, case-sensitive source search, never a shell or regexp program.
    /// Searches UTF-8 text files of at most 256 KiB; binary and larger files
    /// are excluded. Budget exhaustion is an error, never silent truncation.
    pub fn search(&self, query: &str) -> Result<Vec<SearchHit>> {
        if query.is_empty() || query.len() > 256 || query.contains('\n') {
            return Err(ResidentError(
                "search needs a literal query of 1–256 bytes".into(),
            ));
        }
        let inspection = self.inspect()?;
        let mut remaining = MAX_SEARCH_BYTES;
        let mut hits = Vec::new();
        for file in inspection.files {
            if file.bytes > MAX_FILE_BYTES as u64 {
                continue;
            }
            let content = match self.read(&file.path) {
                Ok(content) => content,
                Err(e)
                    if e.0 == "context file is not UTF-8"
                        || e.0 == "binary or oversized context file" =>
                {
                    continue
                }
                Err(e) => return Err(e),
            };
            remaining = remaining.checked_sub(content.len()).ok_or_else(|| {
                ResidentError(
                    "source search exceeds 16 MiB budget; select a smaller workspace".into(),
                )
            })?;
            for (line, text) in content.lines().enumerate() {
                if text.contains(query) {
                    if hits.len() == MAX_SEARCH_HITS {
                        return Err(ResidentError(
                            "more than 100 search hits; narrow the query".into(),
                        ));
                    }
                    hits.push(SearchHit {
                        path: file.path.clone(),
                        line: line + 1,
                        text: text.chars().take(500).collect(),
                    });
                }
            }
        }
        Ok(hits)
    }
    pub fn preview_patch(&self, edits: &[PatchEdit]) -> Result<PatchPreview> {
        if edits.is_empty() || edits.len() > 16 {
            return Err(ResidentError("patch requires 1–16 file edits".into()));
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut bytes = 0;
        for edit in edits {
            self.path(&edit.path, edit.before_hash.is_none())?;
            if !seen.insert(&edit.path) {
                return Err(ResidentError("duplicate patch path".into()));
            }
            bytes += edit.replacement.len();
            if bytes > MAX_CONTEXT_BYTES || edit.replacement.contains('\0') {
                return Err(ResidentError("patch exceeds text/byte limits".into()));
            }
            match &edit.before_hash {
                Some(hash) if *hash == content_hash(self.read(&edit.path)?.as_bytes()) => (),
                None if !self.root.join(&edit.path).try_exists()? => (),
                _ => {
                    return Err(ResidentError(format!(
                        "patch before-content mismatch: {}",
                        edit.path
                    )))
                }
            }
        }
        let binding = serde_json::to_vec(&(self.root.to_string_lossy(), edits))?;
        Ok(PatchPreview {
            id: content_hash(&binding),
            edits: edits.to_vec(),
            provenance: AiProvenance::default(),
        })
    }
    /// Only a direct owner UI/CLI action may call this function. Provider text
    /// never dispatches tools; matching the exact preview ID is mandatory.
    ///
    /// An OS lock serializes cooperating Resident writers. Each file publication
    /// is atomic where the filesystem supports rename/hard links; a multi-file
    /// edit is NOT crash-atomic. Durable recovery copies can remain after a
    /// crash or failed rollback and require owner inspection. Ordinary editors
    /// do not take this lock: detected changes fail closed, but this is not an
    /// atomic compare-and-swap against hostile directory or file replacement.
    pub fn apply_patch(
        &self,
        preview: &PatchPreview,
        owner_confirmation: &str,
    ) -> Result<PatchReceipt> {
        self.apply_patch_inner(preview, owner_confirmation, |_| {})
    }
    pub(crate) fn root_path(&self) -> &Path {
        &self.root
    }
    pub(crate) fn patch_lock(&self) -> Result<fs::File> {
        let lock = self.root.join(".resident-patch.lock");
        match fs::symlink_metadata(&lock) {
            Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
                return Err(ResidentError(
                    "Resident patch lock must be a regular file".into(),
                ))
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        mini_durable::try_lock_exclusive(&lock).map_err(|e| {
            ResidentError(format!(
                "Resident patch workspace busy or lock unavailable: {e}"
            ))
        })
    }
    fn apply_patch_inner(
        &self,
        preview: &PatchPreview,
        owner_confirmation: &str,
        mut before_publication: impl FnMut(usize),
    ) -> Result<PatchReceipt> {
        let _guard = self.patch_lock()?;
        let checked = self.preview_patch(&preview.edits)?;
        if preview != &checked || owner_confirmation != checked.id {
            return Err(ResidentError(
                "owner confirmation must match the exact current patch preview".into(),
            ));
        }
        static STAGE_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = STAGE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut staged = Vec::new();
        for (index, edit) in preview.edits.iter().enumerate() {
            let dest = self.path(&edit.path, edit.before_hash.is_none())?;
            let original = if edit.before_hash.is_some() {
                Some((
                    self.read(&edit.path)?.into_bytes(),
                    fs::metadata(&dest)?.permissions(),
                ))
            } else {
                None
            };
            staged.push(PatchStage::create(dest, edit, original, sequence, index)?);
        }
        self.preview_patch(&preview.edits)?;
        for index in 0..staged.len() {
            // Recheck immediately before each publication. The create path
            // additionally uses hard_link, which cannot overwrite a late file.
            let outcome = self
                .preview_patch(std::slice::from_ref(&preview.edits[index]))
                .and_then(|_| {
                    before_publication(index);
                    staged[index].publish().map_err(ResidentError::from)
                });
            if let Err(error) = outcome {
                let mut rollback_errors = Vec::new();
                for stage in staged[..=index].iter_mut().rev() {
                    if let Err(rollback) = stage.rollback() {
                        stage.keep_recovery = true;
                        rollback_errors
                            .push(format!("{}: {rollback}", stage.destination.display()));
                    }
                }
                let recovery: Vec<_> = staged
                    .iter()
                    .filter(|s| s.keep_recovery)
                    .filter_map(|s| s.original.as_ref())
                    .filter(|p| p.exists())
                    .map(|p| p.display().to_string())
                    .collect();
                return Err(ResidentError(format!("patch publication failed: {error}; rollback_errors={rollback_errors:?}; retained_recovery={recovery:?}. Inspect source state before retrying.")));
            }
        }
        Ok(PatchReceipt {
            preview_id: checked.id,
            patched_paths: preview.edits.iter().map(|e| e.path.clone()).collect(),
            provenance: AiProvenance::default(),
        })
    }
}

/// Every path here is an artifact created with create_new by this call. Drop
/// never removes a pre-existing artifact. Recovery files survive failed restore.
struct PatchStage {
    replacement: PathBuf,
    destination: PathBuf,
    original: Option<PathBuf>,
    replacement_hash: String,
    published: bool,
    keep_recovery: bool,
}
fn new_stage(path: &Path) -> std::io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
impl PatchStage {
    fn create(
        destination: PathBuf,
        edit: &PatchEdit,
        original: Option<(Vec<u8>, fs::Permissions)>,
        sequence: u64,
        index: usize,
    ) -> Result<Self> {
        let parent = destination
            .parent()
            .ok_or_else(|| ResidentError("patch requires a parent directory".into()))?;
        let name = format!(".resident-patch-{}-{sequence}-{index}", std::process::id());
        let replacement = parent.join(format!("{name}.new"));
        let mut file = new_stage(&replacement)?;
        let mut stage = Self {
            replacement,
            destination: destination.clone(),
            original: None,
            replacement_hash: content_hash(edit.replacement.as_bytes()),
            published: false,
            keep_recovery: false,
        };
        file.write_all(edit.replacement.as_bytes())?;
        // Staging is private (0600), but a new source file must be published with
        // ordinary readable permissions, not the staging mode.
        #[cfg(unix)]
        if original.is_none() {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o644))?;
        }
        if let Some((bytes, permissions)) = original {
            file.set_permissions(permissions.clone())?;
            let backup = parent.join(format!("{name}.old"));
            let mut recovery = new_stage(&backup)?;
            stage.original = Some(backup);
            recovery.write_all(&bytes)?;
            recovery.set_permissions(permissions)?;
            recovery.sync_all()?;
            drop(recovery);
            mini_durable::persist_staged_file(stage.original.as_ref().unwrap())?;
        }
        file.sync_all()?;
        drop(file);
        // Persist private recovery/staging names before changing an original.
        mini_durable::persist_staged_file(&stage.replacement)?;
        Ok(stage)
    }
    fn publish(&mut self) -> std::io::Result<()> {
        // Publication may commit before a barrier error. Conservatively attempt
        // hash-checked rollback on any error; never overwrite an intervening edit.
        self.published = true;
        if self.original.is_some() {
            mini_durable::publish_staged_file(&self.replacement, &self.destination)
        } else {
            let result =
                mini_durable::publish_staged_file_new(&self.replacement, &self.destination);
            // An AlreadyExists failure with the stage still in place is a definite
            // pre-publication collision: the destination is another editor's file
            // and must never be rolled back (deleted) by this stage.
            if result.as_ref().is_err_and(|error| {
                error.kind() == std::io::ErrorKind::AlreadyExists && self.replacement.exists()
            }) {
                self.published = false;
            }
            result
        }
    }
    fn rollback(&mut self) -> std::io::Result<()> {
        if !self.published {
            return Ok(());
        }
        // Do not clobber an ordinary editor's intervening replacement during
        // best-effort rollback. Retain original bytes for manual recovery.
        let metadata = fs::symlink_metadata(&self.destination)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(std::io::Error::other("destination changed during rollback"));
        }
        let mut bytes = Vec::new();
        fs::File::open(&self.destination)?
            .take(MAX_FILE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if content_hash(&bytes) != self.replacement_hash {
            return Err(std::io::Error::other(
                "destination content changed during rollback",
            ));
        }
        if let Some(original) = &self.original {
            mini_durable::publish_staged_file(original, &self.destination)
        } else {
            mini_durable::remove_file(&self.destination)
        }
    }
}
impl Drop for PatchStage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.replacement);
        if !self.keep_recovery {
            if let Some(original) = &self.original {
                let _ = fs::remove_file(original);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_context_and_patch_are_bounded_and_hash_checked() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("code.rs"), "old").unwrap();
        fs::write(dir.path().join(".env"), "credential").unwrap();
        let w = Workspace::open(dir.path()).unwrap();
        assert_eq!(w.inspect().unwrap().files.len(), 1);
        assert!(w.context(&[".env".into()]).is_err());
        assert!(w.context(&["../code.rs".into()]).is_err());
        assert_eq!(w.search("old").unwrap().len(), 1);
        let context = w.context(&["code.rs".into()]).unwrap();
        let edits = [PatchEdit {
            path: "code.rs".into(),
            before_hash: Some(context[0].hash.clone()),
            replacement: "new".into(),
        }];
        let preview = w.preview_patch(&edits).unwrap();
        assert!(w.apply_patch(&preview, "yes").is_err());
        fs::write(dir.path().join("code.rs"), "concurrent edit").unwrap();
        assert!(w.apply_patch(&preview, &preview.id).is_err());
        fs::write(dir.path().join("code.rs"), "old").unwrap();
        w.apply_patch(&preview, &preview.id).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("code.rs")).unwrap(),
            "new"
        );
        assert!(w.apply_patch(&preview, &preview.id).is_err());
    }
    #[test]
    fn grants_reject_protected_ancestors_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            ".SSH",
            ".GIT",
            ".MININET",
            "Wallet",
            "IDENTITY",
            "Credentials",
            "Secrets",
        ] {
            let nested = dir.path().join(name).join("code");
            fs::create_dir_all(&nested).unwrap();
            assert!(Workspace::open(&nested).is_err(), "{name}");
        }
    }

    #[test]
    fn hosting_summary_streams_binary_without_model_context() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = [0u8, 1, 255, 4];
        fs::write(dir.path().join("artifact.bin"), bytes).unwrap();
        let w = Workspace::open(dir.path()).unwrap();
        let summary = w.file_summary("artifact.bin").unwrap();
        assert_eq!(summary.bytes, 4);
        assert_eq!(summary.hash, content_hash(&bytes));
        assert!(w.context(&["artifact.bin".into()]).is_err());
        fs::write(dir.path().join("private.key"), bytes).unwrap();
        assert!(w.file_summary("private.key").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_never_grant_outside_context_or_patch() {
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("private"), "private").unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        let w = Workspace::open(dir.path()).unwrap();
        assert!(w.inspect().unwrap().files.is_empty());
        assert!(w.context(&["link/private".into()]).is_err());
        assert!(w
            .preview_patch(&[PatchEdit {
                path: "link/new.rs".into(),
                before_hash: None,
                replacement: "new".into()
            }])
            .is_err());
    }

    #[test]
    fn late_new_file_collision_never_overwrites_and_rolls_back_prior_edits() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("existing.rs"), "original").unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        let preview = workspace
            .preview_patch(&[
                PatchEdit {
                    path: "existing.rs".into(),
                    before_hash: Some(content_hash(b"original")),
                    replacement: "replacement".into(),
                },
                PatchEdit {
                    path: "new.rs".into(),
                    before_hash: None,
                    replacement: "planned new file".into(),
                },
            ])
            .unwrap();
        let result = workspace.apply_patch_inner(&preview, &preview.id, |index| {
            // Simulate an ordinary editor creating the path in the narrow
            // interval after validation and before publication.
            if index == 1 {
                fs::write(dir.path().join("new.rs"), "late owner content").unwrap();
            }
        });
        assert!(result.is_err());
        assert_eq!(
            fs::read(dir.path().join("new.rs")).unwrap(),
            b"late owner content"
        );
        assert_eq!(
            fs::read(dir.path().join("existing.rs")).unwrap(),
            b"original"
        );
        assert!(fs::read_dir(dir.path()).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".old")));
        // Explicitly previewing a different, still-absent name succeeds.
        let create = workspace
            .preview_patch(&[PatchEdit {
                path: "created.rs".into(),
                before_hash: None,
                replacement: "created".into(),
            }])
            .unwrap();
        workspace.apply_patch(&create, &create.id).unwrap();
        assert_eq!(fs::read(dir.path().join("created.rs")).unwrap(), b"created");
    }

    #[cfg(unix)]
    #[test]
    fn new_source_files_are_published_with_ordinary_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        let preview = workspace
            .preview_patch(&[PatchEdit {
                path: "fresh.rs".into(),
                before_hash: None,
                replacement: "fn main() {}".into(),
            }])
            .unwrap();
        workspace.apply_patch(&preview, &preview.id).unwrap();
        let mode = fs::metadata(dir.path().join("fresh.rs"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o644);
    }

    #[test]
    fn identical_late_new_file_is_not_deleted_by_rollback() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("existing.rs"), "original").unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        let preview = workspace
            .preview_patch(&[
                PatchEdit {
                    path: "existing.rs".into(),
                    before_hash: Some(content_hash(b"original")),
                    replacement: "replacement".into(),
                },
                PatchEdit {
                    path: "new.rs".into(),
                    before_hash: None,
                    replacement: "planned new file".into(),
                },
            ])
            .unwrap();
        let result = workspace.apply_patch_inner(&preview, &preview.id, |index| {
            // Another editor independently creates byte-identical content.
            if index == 1 {
                fs::write(dir.path().join("new.rs"), "planned new file").unwrap();
            }
        });
        assert!(result.is_err());
        assert_eq!(
            fs::read(dir.path().join("new.rs")).unwrap(),
            b"planned new file",
            "an independently created identical file must survive rollback"
        );
        assert_eq!(
            fs::read(dir.path().join("existing.rs")).unwrap(),
            b"original"
        );
    }

    #[test]
    fn patch_lock_serializes_writers_and_recovery_artifacts_are_never_context() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("code.rs"), "old").unwrap();
        fs::write(
            dir.path().join(".resident-patch-recovery.old"),
            "recovery bytes",
        )
        .unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        let preview = workspace
            .preview_patch(&[PatchEdit {
                path: "code.rs".into(),
                before_hash: Some(content_hash(b"old")),
                replacement: "new".into(),
            }])
            .unwrap();
        let guard =
            mini_durable::try_lock_exclusive(&dir.path().join(".resident-patch.lock")).unwrap();
        let second = workspace.clone();
        let pending = preview.clone();
        let error =
            std::thread::spawn(move || second.apply_patch(&pending, &pending.id).unwrap_err())
                .join()
                .unwrap();
        assert!(error.0.contains("busy or lock unavailable"));
        assert_eq!(fs::read(dir.path().join("code.rs")).unwrap(), b"old");
        assert_eq!(workspace.inspect().unwrap().files.len(), 1);
        assert!(workspace
            .context(&[".resident-patch-recovery.old".into()])
            .is_err());
        drop(guard);
        workspace.apply_patch(&preview, &preview.id).unwrap();
        assert_eq!(fs::read(dir.path().join("code.rs")).unwrap(), b"new");
    }

    #[cfg(unix)]
    #[test]
    fn executable_permissions_survive_publication_and_atomic_rollback() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.sh");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o751)).unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        let preview = workspace
            .preview_patch(&[PatchEdit {
                path: "run.sh".into(),
                before_hash: Some(content_hash(b"old")),
                replacement: "new".into(),
            }])
            .unwrap();
        workspace.apply_patch(&preview, &preview.id).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o751
        );
        let rollback = workspace
            .preview_patch(&[
                PatchEdit {
                    path: "run.sh".into(),
                    before_hash: Some(content_hash(b"new")),
                    replacement: "again".into(),
                },
                PatchEdit {
                    path: "collision.rs".into(),
                    before_hash: None,
                    replacement: "planned".into(),
                },
            ])
            .unwrap();
        assert!(workspace
            .apply_patch_inner(&rollback, &rollback.id, |index| {
                if index == 1 {
                    fs::write(dir.path().join("collision.rs"), "late").unwrap();
                }
            })
            .is_err());
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o751
        );
    }

    #[test]
    fn failed_rollback_retains_original_without_clobbering_later_owner_edit() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("first.rs"), "original").unwrap();
        let workspace = Workspace::open(dir.path()).unwrap();
        let preview = workspace
            .preview_patch(&[
                PatchEdit {
                    path: "first.rs".into(),
                    before_hash: Some(content_hash(b"original")),
                    replacement: "patch".into(),
                },
                PatchEdit {
                    path: "second.rs".into(),
                    before_hash: None,
                    replacement: "new".into(),
                },
            ])
            .unwrap();
        let result = workspace
            .apply_patch_inner(&preview, &preview.id, |index| {
                if index == 1 {
                    fs::write(dir.path().join("first.rs"), "later owner edit").unwrap();
                    fs::write(dir.path().join("second.rs"), "late collision").unwrap();
                }
            })
            .unwrap_err();
        assert!(result
            .0
            .contains("destination content changed during rollback"));
        assert_eq!(
            fs::read(dir.path().join("first.rs")).unwrap(),
            b"later owner edit"
        );
        let backups: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|s| s == "old"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(&backups[0]).unwrap(), b"original");
        assert_eq!(workspace.inspect().unwrap().files.len(), 2);
    }
}
