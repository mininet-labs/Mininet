//! Local ordered side index for `FsBackend`'s `idx/time/` metadata rows.
//!
//! The metadata files remain authoritative. This module is a delete-and-rebuild
//! acceleration structure: one immutable sorted base plus a strictly bounded
//! append delta. Queries binary-search the base, inspect at most one bounded
//! delta, and verify every returned key against its authoritative metadata row.
//! A manifest detects a missing base, while a one-entry write-ahead journal
//! recovers a metadata write interrupted before the side index was updated.
//! No remote index, daemon, database, or authority is introduced.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use mini_objects::ObjectId;

use crate::{Result, StoreError};

pub(crate) const TIME_PREFIX: &str = "idx/time/";

const INDEX_DIR: &str = "ordered/time-v1";
const MARKER_FILE: &str = "version";
const LOCK_FILE: &str = "lock";
const PENDING_FILE: &str = "pending";
const MANIFEST_FILE: &str = "manifest";
const DELTA_FILE: &str = "delta";
const MARKER: &[u8] = b"mini-store-time-index-v1\n";

const BASE_MAGIC: &[u8; 8] = b"MNTIDX01";
const BASE_VERSION: u16 = 1;
const BASE_HEADER_BYTES: u64 = 20;
const MAX_KEY_BYTES: usize = 192;
const RECORD_BYTES: usize = 8 + 2 + MAX_KEY_BYTES + 8;

/// Every steady-state query may inspect this many unsorted recent writes in
/// addition to a logarithmic base seek and the requested page. Reaching the
/// bound triggers a local compaction; migration/compaction is intentionally
/// not claimed to be page-bounded work.
const MAX_DELTA_RECORDS: u64 = 1_024;
const MAX_FORWARD_QUERY_ROWS: usize = crate::MAX_TIME_PAGE_SIZE + 1;
const MAX_INDEX_DIRECTORY_ENTRIES: usize = 16;
const MAX_PENDING_BYTES: u64 = (8 + 2 + MAX_KEY_BYTES + 8) as u64;
const MAX_TIME_METADATA_VALUE_BYTES: u64 = 4 * 1024;

const MANIFEST_MAGIC: &[u8; 8] = b"MNTMAN01";
const MANIFEST_VERSION: u16 = 1;
const MANIFEST_BYTES: usize = 44;

const PENDING_MAGIC: &[u8; 8] = b"MNTPND01";

type MetadataRow = (String, Vec<u8>);
type QueryRows = (Vec<MetadataRow>, bool);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Manifest {
    generation: u64,
    base_count: u64,
    delta_count: u64,
}

#[derive(Debug)]
struct BaseReader {
    file: File,
    count: u64,
}

impl BaseReader {
    fn open(path: &Path, expected_count: u64) -> Result<Self> {
        let mut file = open_regular(path, "time-index base")?;
        let count = read_base_header(&mut file)?;
        if count != expected_count {
            return Err(StoreError::Corrupt);
        }
        Ok(Self { file, count })
    }

    fn read_key(&mut self, index: u64) -> Result<String> {
        if index >= self.count {
            return Err(StoreError::Corrupt);
        }
        let offset = BASE_HEADER_BYTES
            .checked_add(
                index
                    .checked_mul(RECORD_BYTES as u64)
                    .ok_or(StoreError::Corrupt)?,
            )
            .ok_or(StoreError::Corrupt)?;
        self.file.seek(SeekFrom::Start(offset))?;
        let mut record = [0u8; RECORD_BYTES];
        self.file.read_exact(&mut record)?;
        decode_record(&record, index)
    }

    fn first_greater_than(&mut self, after: &str) -> Result<u64> {
        let mut low = 0u64;
        let mut high = self.count;
        while low < high {
            let mid = low + (high - low) / 2;
            let key = self.read_key(mid)?;
            if key.as_str() <= after {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        Ok(low)
    }

    fn contains(&mut self, needle: &str) -> Result<bool> {
        let index = self.first_greater_than(needle)?;
        if index == 0 {
            return Ok(false);
        }
        Ok(self.read_key(index - 1)? == needle)
    }
}

#[derive(Debug)]
struct BaseWriter {
    file: File,
    temp_path: PathBuf,
    final_path: PathBuf,
    count: u64,
    last_key: Option<String>,
}

impl BaseWriter {
    fn new(index_root: &Path, generation: u64) -> Result<Self> {
        let final_path = base_path(index_root, generation);
        let temp_path = index_root.join(format!("base-{generation:020}.tmp"));
        remove_regular_if_present(&temp_path, "time-index base temporary")?;
        remove_regular_if_present(&final_path, "orphaned time-index base")?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&temp_path)?;
        write_base_header(&mut file, 0)?;
        Ok(Self {
            file,
            temp_path,
            final_path,
            count: 0,
            last_key: None,
        })
    }

    fn push(&mut self, key: &str) -> Result<()> {
        validate_time_key(key)?;
        if self
            .last_key
            .as_deref()
            .is_some_and(|previous| previous >= key)
        {
            return Err(StoreError::Corrupt);
        }
        let record = encode_record(key, self.count)?;
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&record)?;
        self.last_key = Some(key.to_string());
        self.count = self.count.checked_add(1).ok_or(StoreError::LimitExceeded)?;
        Ok(())
    }

    fn finish(mut self) -> Result<(PathBuf, u64)> {
        self.file.seek(SeekFrom::Start(0))?;
        write_base_header(&mut self.file, self.count)?;
        self.file.sync_all()?;
        drop(self.file);
        fs::rename(&self.temp_path, &self.final_path)?;
        sync_parent_directory(&self.final_path)?;
        Ok((self.final_path, self.count))
    }
}

#[derive(Debug)]
struct LockedIndex<'a> {
    root: &'a Path,
    index_root: PathBuf,
}

impl<'a> LockedIndex<'a> {
    fn prepare(&self) -> Result<()> {
        let marker = match read_regular_limited(
            &self.index_root.join(MARKER_FILE),
            "time-index marker",
            MARKER.len() as u64,
        ) {
            Ok(marker) => marker,
            Err(StoreError::Corrupt) | Err(StoreError::LimitExceeded) => {
                self.rebuild()?;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        if marker.as_deref() != Some(MARKER) {
            self.rebuild()?;
            return Ok(());
        }

        let pending = match read_regular_limited(
            &self.index_root.join(PENDING_FILE),
            "time-index pending journal",
            MAX_PENDING_BYTES,
        ) {
            Ok(pending) => pending,
            Err(StoreError::Corrupt) | Err(StoreError::LimitExceeded) => {
                self.rebuild()?;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        let manifest = match self.read_manifest() {
            Ok(manifest) => manifest,
            Err(StoreError::Corrupt) | Err(StoreError::LimitExceeded) => {
                self.rebuild()?;
                return Ok(());
            }
            Err(error) => return Err(error),
        };

        if let Err(error) = self.validate_base(&manifest) {
            return match error {
                StoreError::Corrupt | StoreError::LimitExceeded => self.rebuild().map(|_| ()),
                other => Err(other),
            };
        }

        let actual_delta = match self.delta_record_count() {
            Ok(count) => count,
            Err(StoreError::Corrupt) | Err(StoreError::LimitExceeded) => {
                self.rebuild()?;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        let allowed_extra = u64::from(pending.is_some());
        if actual_delta < manifest.delta_count
            || actual_delta > manifest.delta_count.saturating_add(allowed_extra)
            || manifest.delta_count > MAX_DELTA_RECORDS
        {
            self.rebuild()?;
            return Ok(());
        }

        if let Some(bytes) = pending {
            let key = match decode_pending(&bytes) {
                Ok(key) => key,
                Err(StoreError::Corrupt) | Err(StoreError::LimitExceeded) => {
                    self.rebuild()?;
                    return Ok(());
                }
                Err(error) => return Err(error),
            };
            if let Err(error) = self.recover_pending(&key, manifest, actual_delta) {
                match error {
                    StoreError::Corrupt | StoreError::LimitExceeded => {
                        self.rebuild()?;
                        return Ok(());
                    }
                    other => return Err(other),
                }
            }
        } else if actual_delta != manifest.delta_count {
            self.rebuild()?;
        }

        let manifest = self.read_manifest()?;
        self.validate_base(&manifest)?;
        if self.delta_record_count()? != manifest.delta_count {
            self.rebuild()?;
            return Ok(());
        }
        if manifest.delta_count >= MAX_DELTA_RECORDS {
            self.compact(&manifest)?;
        }
        self.cleanup_orphan_bases(self.read_manifest()?.generation)
    }

    fn read_manifest(&self) -> Result<Manifest> {
        let bytes = read_regular_limited(
            &self.index_root.join(MANIFEST_FILE),
            "time-index manifest",
            MANIFEST_BYTES as u64,
        )?
        .ok_or(StoreError::Corrupt)?;
        decode_manifest(&bytes)
    }

    fn write_manifest(&self, manifest: &Manifest) -> Result<()> {
        atomic_write(
            &self.index_root.join(MANIFEST_FILE),
            &encode_manifest(manifest),
        )
    }

    fn validate_base(&self, manifest: &Manifest) -> Result<()> {
        let _ = BaseReader::open(
            &base_path(&self.index_root, manifest.generation),
            manifest.base_count,
        )?;
        Ok(())
    }

    fn write_pending(&self, key: &str) -> Result<()> {
        atomic_write(&self.index_root.join(PENDING_FILE), &encode_pending(key)?)
    }

    fn clear_pending(&self) -> Result<()> {
        remove_regular_if_present(
            &self.index_root.join(PENDING_FILE),
            "time-index pending journal",
        )
    }

    fn recover_pending(&self, key: &str, mut manifest: Manifest, actual_delta: u64) -> Result<()> {
        let metadata_exists = read_meta_value(self.root, key)?.is_some();

        if actual_delta == manifest.delta_count.saturating_add(1) {
            let extra = self.read_delta_key(manifest.delta_count)?;
            if extra != key {
                self.rebuild()?;
                return Ok(());
            }
            if metadata_exists {
                manifest.delta_count = actual_delta;
                self.write_manifest(&manifest)?;
            } else {
                self.truncate_delta(manifest.delta_count)?;
            }
        } else if actual_delta == manifest.delta_count {
            if metadata_exists && !self.contains_key(&manifest, key)? {
                self.append_delta(&mut manifest, key)?;
            }
        } else {
            self.rebuild()?;
            return Ok(());
        }

        self.clear_pending()?;
        let manifest = self.read_manifest()?;
        if manifest.delta_count >= MAX_DELTA_RECORDS {
            self.compact(&manifest)?;
        }
        Ok(())
    }

    fn contains_key(&self, manifest: &Manifest, key: &str) -> Result<bool> {
        let mut base = BaseReader::open(
            &base_path(&self.index_root, manifest.generation),
            manifest.base_count,
        )?;
        if base.contains(key)? {
            return Ok(true);
        }
        Ok(self
            .read_delta_keys(manifest.delta_count)?
            .iter()
            .any(|candidate| candidate == key))
    }

    fn append_delta(&self, manifest: &mut Manifest, key: &str) -> Result<()> {
        if manifest.delta_count >= MAX_DELTA_RECORDS {
            self.compact(manifest)?;
            *manifest = self.read_manifest()?;
        }
        let delta_path = self.index_root.join(DELTA_FILE);
        let expected_len = manifest
            .delta_count
            .checked_mul(RECORD_BYTES as u64)
            .ok_or(StoreError::LimitExceeded)?;
        reject_symlink_or_non_file_if_present(&delta_path, "time-index delta")?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&delta_path)?;
        if file.metadata()?.len() != expected_len {
            return Err(StoreError::Corrupt);
        }
        file.write_all(&encode_record(key, manifest.delta_count)?)?;
        file.sync_all()?;

        manifest.delta_count = manifest
            .delta_count
            .checked_add(1)
            .ok_or(StoreError::LimitExceeded)?;
        self.write_manifest(manifest)
    }

    fn delta_record_count(&self) -> Result<u64> {
        let path = self.index_root.join(DELTA_FILE);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(StoreError::Io("time-index delta is a symlink".to_string()))
            }
            Ok(metadata) if !metadata.is_file() => {
                return Err(StoreError::Io(
                    "time-index delta is not a regular file".to_string(),
                ))
            }
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(StoreError::Corrupt)
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.len() % RECORD_BYTES as u64 != 0 {
            return Err(StoreError::Corrupt);
        }
        let count = metadata.len() / RECORD_BYTES as u64;
        if count > MAX_DELTA_RECORDS.saturating_add(1) {
            return Err(StoreError::LimitExceeded);
        }
        Ok(count)
    }

    fn read_delta_key(&self, index: u64) -> Result<String> {
        let path = self.index_root.join(DELTA_FILE);
        let mut file = open_regular(&path, "time-index delta")?;
        let offset = index
            .checked_mul(RECORD_BYTES as u64)
            .ok_or(StoreError::Corrupt)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut record = [0u8; RECORD_BYTES];
        file.read_exact(&mut record)?;
        decode_record(&record, index)
    }

    fn read_delta_keys(&self, count: u64) -> Result<Vec<String>> {
        if count > MAX_DELTA_RECORDS || self.delta_record_count()? != count {
            return Err(StoreError::Corrupt);
        }
        let capacity = usize::try_from(count).map_err(|_| StoreError::LimitExceeded)?;
        let mut file = open_regular(&self.index_root.join(DELTA_FILE), "time-index delta")?;
        let mut keys = Vec::with_capacity(capacity);
        for index in 0..count {
            let mut record = [0u8; RECORD_BYTES];
            file.read_exact(&mut record)?;
            keys.push(decode_record(&record, index)?);
        }
        Ok(keys)
    }

    fn truncate_delta(&self, count: u64) -> Result<()> {
        let path = self.index_root.join(DELTA_FILE);
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        file.set_len(
            count
                .checked_mul(RECORD_BYTES as u64)
                .ok_or(StoreError::LimitExceeded)?,
        )?;
        file.sync_all()?;
        Ok(())
    }

    fn compact(&self, manifest: &Manifest) -> Result<()> {
        if manifest.delta_count == 0 {
            return Ok(());
        }
        let mut delta = self.read_delta_keys(manifest.delta_count)?;
        delta.sort();
        delta.dedup();

        let next_generation = manifest
            .generation
            .checked_add(1)
            .ok_or(StoreError::LimitExceeded)?;
        let mut writer = BaseWriter::new(&self.index_root, next_generation)?;
        let mut base = BaseReader::open(
            &base_path(&self.index_root, manifest.generation),
            manifest.base_count,
        )?;

        let mut base_index = 0u64;
        let mut delta_index = 0usize;
        while base_index < base.count || delta_index < delta.len() {
            let base_key = if base_index < base.count {
                Some(base.read_key(base_index)?)
            } else {
                None
            };
            let delta_key = delta.get(delta_index);
            match (base_key.as_deref(), delta_key) {
                (Some(left), Some(right)) if left < right.as_str() => {
                    writer.push(left)?;
                    base_index += 1;
                }
                (Some(left), Some(right)) if left == right.as_str() => {
                    writer.push(left)?;
                    base_index += 1;
                    delta_index += 1;
                }
                (Some(_), Some(right)) => {
                    writer.push(right)?;
                    delta_index += 1;
                }
                (Some(left), None) => {
                    writer.push(left)?;
                    base_index += 1;
                }
                (None, Some(right)) => {
                    writer.push(right)?;
                    delta_index += 1;
                }
                (None, None) => break,
            }
        }

        let (_, base_count) = writer.finish()?;
        let next = Manifest {
            generation: next_generation,
            base_count,
            delta_count: 0,
        };

        // If power is lost between these writes, the old manifest and empty
        // delta disagree and the next open rebuilds from authoritative rows.
        // After the manifest switch the new base is complete; the old base is
        // only an orphan and never participates in a query.
        atomic_write(&self.index_root.join(DELTA_FILE), b"")?;
        self.write_manifest(&next)?;
        remove_regular_if_present(
            &base_path(&self.index_root, manifest.generation),
            "superseded time-index base",
        )?;
        self.cleanup_orphan_bases(next_generation)
    }

    fn query_forward(&self, manifest: &Manifest, after: &str, limit: usize) -> Result<QueryRows> {
        validate_after_key(after)?;
        let mut delta = self.read_delta_keys(manifest.delta_count)?;
        delta.retain(|key| key.as_str() > after);
        delta.sort();
        delta.dedup();

        let extra = delta.len();
        // `candidates` already contains `extra` delta rows. A total budget of
        // `limit + extra` therefore reads at most `limit` base rows; the extra
        // slots cover keys duplicated between base and delta before dedup.
        let base_budget = limit.checked_add(extra).ok_or(StoreError::LimitExceeded)?;
        let mut candidates = delta;
        let mut base = BaseReader::open(
            &base_path(&self.index_root, manifest.generation),
            manifest.base_count,
        )?;
        let mut index = base.first_greater_than(after)?;
        while index < base.count && candidates.len() < base_budget {
            candidates.push(base.read_key(index)?);
            index += 1;
        }
        candidates.sort();
        candidates.dedup();
        self.resolve_candidates(candidates.into_iter(), limit)
    }

    fn query_reverse(&self, manifest: &Manifest, limit: usize) -> Result<QueryRows> {
        let mut delta = self.read_delta_keys(manifest.delta_count)?;
        delta.sort_by(|left, right| right.cmp(left));
        delta.dedup();

        let extra = delta.len();
        // Same accounting as the forward scan: the delta rows already occupy
        // `extra` candidate slots, so only `limit` base rows are read.
        let base_budget = limit.checked_add(extra).ok_or(StoreError::LimitExceeded)?;
        let mut candidates = delta;
        let mut base = BaseReader::open(
            &base_path(&self.index_root, manifest.generation),
            manifest.base_count,
        )?;
        let mut remaining = base.count;
        while remaining > 0 && candidates.len() < base_budget {
            remaining -= 1;
            candidates.push(base.read_key(remaining)?);
        }
        candidates.sort_by(|left, right| right.cmp(left));
        candidates.dedup();
        self.resolve_candidates(candidates.into_iter(), limit)
    }

    fn resolve_candidates(
        &self,
        candidates: impl Iterator<Item = String>,
        limit: usize,
    ) -> Result<QueryRows> {
        let mut rows = Vec::with_capacity(limit);
        let mut stale = false;
        for key in candidates {
            if rows.len() >= limit {
                break;
            }
            match read_meta_value(self.root, &key)? {
                Some(value) => rows.push((key, value)),
                None => stale = true,
            }
        }
        Ok((rows, stale))
    }

    fn rebuild(&self) -> Result<usize> {
        self.clear_base_files()?;
        let generation = 1;
        let mut writer = BaseWriter::new(&self.index_root, generation)?;
        visit_legacy_time_keys(self.root, |key| writer.push(key))?;
        let count = usize::try_from(writer.count).map_err(|_| StoreError::LimitExceeded)?;
        let (_, base_count) = writer.finish()?;

        atomic_write(&self.index_root.join(DELTA_FILE), b"")?;
        self.write_manifest(&Manifest {
            generation,
            base_count,
            delta_count: 0,
        })?;
        atomic_write(&self.index_root.join(MARKER_FILE), MARKER)?;
        self.clear_pending()?;
        Ok(count)
    }

    fn clear_base_files(&self) -> Result<()> {
        let permanent = [
            MARKER_FILE,
            LOCK_FILE,
            PENDING_FILE,
            MANIFEST_FILE,
            DELTA_FILE,
        ];
        let mut entries = 0usize;
        for entry in fs::read_dir(&self.index_root)? {
            entries = entries.checked_add(1).ok_or(StoreError::LimitExceeded)?;
            if entries > MAX_INDEX_DIRECTORY_ENTRIES {
                return Err(StoreError::LimitExceeded);
            }

            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(StoreError::Io(
                    "symlink in time-index directory".to_string(),
                ));
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("base-") && (name.ends_with(".idx") || name.ends_with(".tmp")) {
                if !file_type.is_file() {
                    return Err(StoreError::Io("non-file time-index base entry".to_string()));
                }
                fs::remove_file(path)?;
                continue;
            }
            if name.ends_with(".tmp-write") {
                if !file_type.is_file() {
                    return Err(StoreError::Io(
                        "non-file time-index atomic temporary".to_string(),
                    ));
                }
                fs::remove_file(path)?;
                continue;
            }
            if permanent.contains(&name.as_ref()) {
                if !file_type.is_file() {
                    return Err(StoreError::Io(
                        "non-file permanent time-index entry".to_string(),
                    ));
                }
                continue;
            }
            return Err(StoreError::Io(format!(
                "unknown entry in time-index directory: {name}"
            )));
        }
        Ok(())
    }

    fn cleanup_orphan_bases(&self, current_generation: u64) -> Result<()> {
        let current_name = format!("base-{current_generation:020}.idx");
        let permanent = [
            MARKER_FILE,
            LOCK_FILE,
            PENDING_FILE,
            MANIFEST_FILE,
            DELTA_FILE,
        ];
        let mut entries = 0usize;
        let mut base_files = 0usize;
        for entry in fs::read_dir(&self.index_root)? {
            entries = entries.checked_add(1).ok_or(StoreError::LimitExceeded)?;
            if entries > MAX_INDEX_DIRECTORY_ENTRIES {
                return Err(StoreError::LimitExceeded);
            }

            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(StoreError::Io(
                    "symlink in time-index directory".to_string(),
                ));
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();

            if name.starts_with("base-") && name.ends_with(".tmp") {
                if !file_type.is_file() {
                    return Err(StoreError::Io(
                        "non-file time-index base temporary".to_string(),
                    ));
                }
                fs::remove_file(path)?;
                continue;
            }
            if name.starts_with("base-") && name.ends_with(".idx") {
                base_files += 1;
                if base_files > 8 {
                    return Err(StoreError::LimitExceeded);
                }
                if !file_type.is_file() {
                    return Err(StoreError::Io("non-file time-index base".to_string()));
                }
                if name != current_name {
                    fs::remove_file(path)?;
                }
                continue;
            }
            if name.ends_with(".tmp-write") {
                if !file_type.is_file() {
                    return Err(StoreError::Io(
                        "non-file time-index atomic temporary".to_string(),
                    ));
                }
                fs::remove_file(path)?;
                continue;
            }
            if permanent.contains(&name.as_ref()) {
                if !file_type.is_file() {
                    return Err(StoreError::Io(
                        "non-file permanent time-index entry".to_string(),
                    ));
                }
                continue;
            }
            return Err(StoreError::Io(format!(
                "unknown entry in time-index directory: {name}"
            )));
        }
        Ok(())
    }
}

pub(crate) fn put_time_meta<F>(root: &Path, key: &str, write_metadata: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    validate_time_key(key)?;
    with_locked(root, |index| {
        index.write_pending(key)?;
        write_metadata()?;

        let mut manifest = index.read_manifest()?;
        let indexed = match index.contains_key(&manifest, key) {
            Ok(indexed) => indexed,
            Err(StoreError::Corrupt) | Err(StoreError::LimitExceeded) => {
                // The metadata row is already authoritative. Rebuild the
                // disposable acceleration index rather than leaving a
                // permanent pending journal that wedges later writes.
                index.rebuild()?;
                manifest = index.read_manifest()?;
                index.contains_key(&manifest, key)?
            }
            Err(error) => return Err(error),
        };
        if !indexed {
            index.append_delta(&mut manifest, key)?;
        }
        index.clear_pending()?;

        let manifest = index.read_manifest()?;
        if manifest.delta_count >= MAX_DELTA_RECORDS {
            index.compact(&manifest)?;
        }
        Ok(())
    })
}

pub(crate) fn last(root: &Path, limit: usize) -> Result<Vec<(String, Vec<u8>)>> {
    if limit > crate::MAX_TIME_PAGE_SIZE {
        return Err(StoreError::LimitExceeded);
    }
    if limit == 0 {
        return Ok(Vec::new());
    }
    with_locked(root, |index| {
        let manifest = index.read_manifest()?;
        let (rows, stale) = match index.query_reverse(&manifest, limit) {
            Ok(result) => result,
            Err(StoreError::Corrupt) | Err(StoreError::LimitExceeded) => {
                index.rebuild()?;
                let manifest = index.read_manifest()?;
                index.query_reverse(&manifest, limit)?
            }
            Err(error) => return Err(error),
        };
        if stale {
            index.rebuild()?;
            let manifest = index.read_manifest()?;
            return index.query_reverse(&manifest, limit).map(|result| result.0);
        }
        Ok(rows)
    })
}

pub(crate) fn page(root: &Path, after: &str, limit: usize) -> Result<Vec<(String, Vec<u8>)>> {
    if limit > MAX_FORWARD_QUERY_ROWS {
        return Err(StoreError::LimitExceeded);
    }
    if limit == 0 {
        return Ok(Vec::new());
    }
    with_locked(root, |index| {
        let manifest = index.read_manifest()?;
        let (rows, stale) = match index.query_forward(&manifest, after, limit) {
            Ok(result) => result,
            Err(StoreError::Corrupt) | Err(StoreError::LimitExceeded) => {
                index.rebuild()?;
                let manifest = index.read_manifest()?;
                index.query_forward(&manifest, after, limit)?
            }
            Err(error) => return Err(error),
        };
        if stale {
            index.rebuild()?;
            let manifest = index.read_manifest()?;
            return index
                .query_forward(&manifest, after, limit)
                .map(|result| result.0);
        }
        Ok(rows)
    })
}

pub(crate) fn rebuild(root: &Path) -> Result<usize> {
    with_locked(root, |index| index.rebuild())
}

fn with_locked<T>(root: &Path, operation: impl FnOnce(&LockedIndex<'_>) -> Result<T>) -> Result<T> {
    let index_root = ensure_layout(root)?;
    let lock_path = index_root.join(LOCK_FILE);
    reject_symlink_or_non_file_if_present(&lock_path, "time-index lock")?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    #[allow(clippy::incompatible_msrv)]
    lock.lock()?;

    let index = LockedIndex { root, index_root };
    index.prepare()?;
    operation(&index)
}

fn ensure_layout(root: &Path) -> Result<PathBuf> {
    ensure_existing_or_create_directory(root, "store root")?;
    let ordered = root.join("ordered");
    ensure_existing_or_create_directory(&ordered, "ordered-index directory")?;
    let index_root = root.join(INDEX_DIR);
    ensure_existing_or_create_directory(&index_root, "time-index directory")?;
    Ok(index_root)
}

fn ensure_existing_or_create_directory(path: &Path, label: &str) -> Result<()> {
    let validate_existing = || -> Result<()> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                Err(StoreError::Io(format!("{label} is a symlink")))
            }
            Ok(metadata) if !metadata.is_dir() => {
                Err(StoreError::Io(format!("{label} is not a directory")))
            }
            Ok(_) => Ok(()),
            Err(error) => Err(error.into()),
        }
    };

    match fs::symlink_metadata(path) {
        Ok(_) => validate_existing(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match fs::create_dir(path) {
                Ok(()) => Ok(()),
                // Independent processes may both observe the directory as
                // absent before one creates it. Revalidate the winner rather
                // than turning that safe race into a write failure.
                Err(create_error) if create_error.kind() == std::io::ErrorKind::AlreadyExists => {
                    validate_existing()
                }
                Err(create_error) => Err(create_error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

fn reject_symlink_or_non_file_if_present(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(StoreError::Io(format!("{label} is a symlink")))
        }
        Ok(metadata) if !metadata.is_file() => {
            Err(StoreError::Io(format!("{label} is not a regular file")))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn open_regular(path: &Path, label: &str) -> Result<File> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(StoreError::Io(format!("{label} is a symlink")))
        }
        Ok(metadata) if !metadata.is_file() => {
            Err(StoreError::Io(format!("{label} is not a regular file")))
        }
        Ok(_) => Ok(File::open(path)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(StoreError::Corrupt),
        Err(error) => Err(error.into()),
    }
}

fn read_regular_limited(path: &Path, label: &str, max_bytes: u64) -> Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(StoreError::Io(format!("{label} is a symlink")))
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(StoreError::Io(format!("{label} is not a regular file")))
        }
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.len() > max_bytes {
        return Err(StoreError::LimitExceeded);
    }

    let capacity = usize::try_from(metadata.len()).map_err(|_| StoreError::LimitExceeded)?;
    let mut bytes = Vec::with_capacity(capacity);
    File::open(path)?
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(StoreError::LimitExceeded);
    }
    Ok(Some(bytes))
}

fn remove_regular_if_present(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(StoreError::Io(format!("{label} is a symlink")))
        }
        Ok(metadata) if !metadata.is_file() => {
            Err(StoreError::Io(format!("{label} is not a regular file")))
        }
        Ok(_) => {
            fs::remove_file(path)?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Make an already-completed rename durable in the containing directory
/// where the standard library exposes a directory file descriptor. The side
/// index remains reconstructible authority-free state, but returning success
/// should not knowingly leave Unix rename persistence to a later unrelated
/// write.
fn sync_parent_directory(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| StoreError::Io("time-index path has no containing directory".to_string()))?;
    #[cfg(unix)]
    {
        File::open(parent)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        // `std` does not expose a portable directory-fsync operation on every
        // target. Those platforms retain the reconstructible-index fallback;
        // the limitation is recorded in D-0430 rather than hidden.
        let _ = parent;
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_extension("tmp-write");
    remove_regular_if_present(&temp, "time-index atomic temporary file")?;
    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(temp, path)?;
    sync_parent_directory(path)?;
    Ok(())
}

fn base_path(index_root: &Path, generation: u64) -> PathBuf {
    index_root.join(format!("base-{generation:020}.idx"))
}

fn write_base_header(file: &mut File, count: u64) -> Result<()> {
    file.write_all(BASE_MAGIC)?;
    file.write_all(&BASE_VERSION.to_be_bytes())?;
    file.write_all(&[0, 0])?;
    file.write_all(&count.to_be_bytes())?;
    Ok(())
}

fn read_base_header(file: &mut File) -> Result<u64> {
    file.seek(SeekFrom::Start(0))?;
    let mut header = [0u8; BASE_HEADER_BYTES as usize];
    file.read_exact(&mut header).map_err(|error| {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            StoreError::Corrupt
        } else {
            error.into()
        }
    })?;
    if &header[..8] != BASE_MAGIC
        || u16::from_be_bytes([header[8], header[9]]) != BASE_VERSION
        || header[10] != 0
        || header[11] != 0
    {
        return Err(StoreError::Corrupt);
    }
    let mut raw_count = [0u8; 8];
    raw_count.copy_from_slice(&header[12..20]);
    let count = u64::from_be_bytes(raw_count);
    let expected = BASE_HEADER_BYTES
        .checked_add(
            count
                .checked_mul(RECORD_BYTES as u64)
                .ok_or(StoreError::Corrupt)?,
        )
        .ok_or(StoreError::Corrupt)?;
    if file.metadata()?.len() != expected {
        return Err(StoreError::Corrupt);
    }
    Ok(count)
}

fn encode_manifest(manifest: &Manifest) -> [u8; MANIFEST_BYTES] {
    let mut bytes = [0u8; MANIFEST_BYTES];
    bytes[..8].copy_from_slice(MANIFEST_MAGIC);
    bytes[8..10].copy_from_slice(&MANIFEST_VERSION.to_be_bytes());
    bytes[12..20].copy_from_slice(&manifest.generation.to_be_bytes());
    bytes[20..28].copy_from_slice(&manifest.base_count.to_be_bytes());
    bytes[28..36].copy_from_slice(&manifest.delta_count.to_be_bytes());
    let checksum = bytes_checksum(&bytes[..36]);
    bytes[36..44].copy_from_slice(&checksum.to_be_bytes());
    bytes
}

fn decode_manifest(bytes: &[u8]) -> Result<Manifest> {
    if bytes.len() != MANIFEST_BYTES
        || &bytes[..8] != MANIFEST_MAGIC
        || u16::from_be_bytes([bytes[8], bytes[9]]) != MANIFEST_VERSION
        || bytes[10] != 0
        || bytes[11] != 0
    {
        return Err(StoreError::Corrupt);
    }
    let mut raw_checksum = [0u8; 8];
    raw_checksum.copy_from_slice(&bytes[36..44]);
    if u64::from_be_bytes(raw_checksum) != bytes_checksum(&bytes[..36]) {
        return Err(StoreError::Corrupt);
    }
    let generation = read_u64(&bytes[12..20]);
    let base_count = read_u64(&bytes[20..28]);
    let delta_count = read_u64(&bytes[28..36]);
    if generation == 0 || delta_count > MAX_DELTA_RECORDS {
        return Err(StoreError::Corrupt);
    }
    Ok(Manifest {
        generation,
        base_count,
        delta_count,
    })
}

fn read_u64(bytes: &[u8]) -> u64 {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(bytes);
    u64::from_be_bytes(raw)
}

fn encode_record(key: &str, ordinal: u64) -> Result<[u8; RECORD_BYTES]> {
    validate_time_key(key)?;
    let key_bytes = key.as_bytes();
    if key_bytes.len() > MAX_KEY_BYTES {
        return Err(StoreError::LimitExceeded);
    }
    let mut record = [0u8; RECORD_BYTES];
    record[..8].copy_from_slice(&ordinal.to_be_bytes());
    record[8..10].copy_from_slice(&(key_bytes.len() as u16).to_be_bytes());
    record[10..10 + key_bytes.len()].copy_from_slice(key_bytes);
    let checksum = record_checksum(ordinal, key_bytes);
    record[10 + MAX_KEY_BYTES..].copy_from_slice(&checksum.to_be_bytes());
    Ok(record)
}

fn decode_record(record: &[u8; RECORD_BYTES], expected_ordinal: u64) -> Result<String> {
    let ordinal = read_u64(&record[..8]);
    if ordinal != expected_ordinal {
        return Err(StoreError::Corrupt);
    }
    let length = u16::from_be_bytes([record[8], record[9]]) as usize;
    if length == 0 || length > MAX_KEY_BYTES {
        return Err(StoreError::Corrupt);
    }
    if record[10 + length..10 + MAX_KEY_BYTES]
        .iter()
        .any(|byte| *byte != 0)
    {
        return Err(StoreError::Corrupt);
    }
    let key_bytes = &record[10..10 + length];
    if read_u64(&record[10 + MAX_KEY_BYTES..]) != record_checksum(ordinal, key_bytes) {
        return Err(StoreError::Corrupt);
    }
    let key = String::from_utf8(key_bytes.to_vec()).map_err(|_| StoreError::Corrupt)?;
    validate_time_key(&key)?;
    Ok(key)
}

fn record_checksum(ordinal: u64, key: &[u8]) -> u64 {
    let mut bytes = ordinal.to_be_bytes().to_vec();
    bytes.extend_from_slice(key);
    bytes_checksum(&bytes)
}

fn bytes_checksum(bytes: &[u8]) -> u64 {
    // Non-authoritative corruption detector only. Metadata rows and object
    // content addresses remain the source of truth; this is not a signature or
    // a new cryptographic construction.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn encode_pending(key: &str) -> Result<Vec<u8>> {
    validate_time_key(key)?;
    if key.len() > MAX_KEY_BYTES {
        return Err(StoreError::LimitExceeded);
    }
    let mut bytes = Vec::with_capacity(8 + 2 + key.len() + 8);
    bytes.extend_from_slice(PENDING_MAGIC);
    bytes.extend_from_slice(&(key.len() as u16).to_be_bytes());
    bytes.extend_from_slice(key.as_bytes());
    bytes.extend_from_slice(&record_checksum(0, key.as_bytes()).to_be_bytes());
    Ok(bytes)
}

fn decode_pending(bytes: &[u8]) -> Result<String> {
    if bytes.len() < 18 || &bytes[..8] != PENDING_MAGIC {
        return Err(StoreError::Corrupt);
    }
    let length = u16::from_be_bytes([bytes[8], bytes[9]]) as usize;
    if length == 0 || length > MAX_KEY_BYTES || bytes.len() != 8 + 2 + length + 8 {
        return Err(StoreError::Corrupt);
    }
    let key_bytes = &bytes[10..10 + length];
    if read_u64(&bytes[10 + length..]) != record_checksum(0, key_bytes) {
        return Err(StoreError::Corrupt);
    }
    let key = String::from_utf8(key_bytes.to_vec()).map_err(|_| StoreError::Corrupt)?;
    validate_time_key(&key)?;
    Ok(key)
}

fn validate_time_key(key: &str) -> Result<()> {
    let rest = key.strip_prefix(TIME_PREFIX).ok_or(StoreError::Corrupt)?;
    let mut parts = rest.split('/');
    let timestamp = parts.next().ok_or(StoreError::Corrupt)?;
    let object_id = parts.next().ok_or(StoreError::Corrupt)?;
    if parts.next().is_some()
        || timestamp.len() != 20
        || !timestamp.bytes().all(|byte| byte.is_ascii_digit())
        || timestamp.parse::<u64>().is_err()
        || object_id.is_empty()
    {
        return Err(StoreError::Corrupt);
    }
    ObjectId::parse(object_id)
        .map(|_| ())
        .map_err(|_| StoreError::Corrupt)
}

fn validate_after_key(key: &str) -> Result<()> {
    if key.ends_with('/') {
        let timestamp = key
            .strip_prefix(TIME_PREFIX)
            .and_then(|rest| rest.strip_suffix('/'))
            .ok_or(StoreError::InvalidCursor)?;
        if timestamp.len() != 20
            || !timestamp.bytes().all(|byte| byte.is_ascii_digit())
            || timestamp.parse::<u64>().is_err()
        {
            return Err(StoreError::InvalidCursor);
        }
        Ok(())
    } else {
        validate_time_key(key).map_err(|_| StoreError::InvalidCursor)
    }
}

fn read_meta_value(root: &Path, key: &str) -> Result<Option<Vec<u8>>> {
    validate_time_key(key)?;
    let base = root.join("meta");
    let mut current = base.clone();
    let segments: Vec<&str> = key.split('/').collect();
    for segment in &segments[..segments.len() - 1] {
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(StoreError::Io(
                    "symlink in time-index metadata path".to_string(),
                ))
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(StoreError::Io(
                    "non-directory in time-index metadata path".to_string(),
                ))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        }
    }
    current.push(segments[segments.len() - 1]);
    match fs::symlink_metadata(&current) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(StoreError::Io(
            "symlinked time-index metadata row".to_string(),
        )),
        Ok(metadata) if !metadata.is_file() => Err(StoreError::Io(
            "non-file time-index metadata row".to_string(),
        )),
        Ok(metadata) => {
            if metadata.len() > MAX_TIME_METADATA_VALUE_BYTES {
                return Err(StoreError::LimitExceeded);
            }
            let capacity =
                usize::try_from(metadata.len()).map_err(|_| StoreError::LimitExceeded)?;
            let mut value = Vec::with_capacity(capacity);
            File::open(current)?
                .take(MAX_TIME_METADATA_VALUE_BYTES + 1)
                .read_to_end(&mut value)?;
            if value.len() as u64 > MAX_TIME_METADATA_VALUE_BYTES {
                return Err(StoreError::LimitExceeded);
            }
            Ok(Some(value))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn visit_legacy_time_keys(root: &Path, mut visitor: impl FnMut(&str) -> Result<()>) -> Result<()> {
    let time_root = root.join("meta").join("idx").join("time");
    match fs::symlink_metadata(&time_root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(StoreError::Io(
                "symlink in legacy time-index root".to_string(),
            ))
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(StoreError::Io(
                "legacy time-index root is not a directory".to_string(),
            ))
        }
        Ok(_) => {}
    }

    let mut timestamps = Vec::new();
    for entry in fs::read_dir(&time_root)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() || !file_type.is_dir() {
            return Err(StoreError::Io(
                "unsafe entry in legacy time-index root".to_string(),
            ));
        }
        let timestamp = entry
            .file_name()
            .into_string()
            .map_err(|_| StoreError::Corrupt)?;
        if timestamp.len() != 20
            || !timestamp.bytes().all(|byte| byte.is_ascii_digit())
            || timestamp.parse::<u64>().is_err()
        {
            return Err(StoreError::Corrupt);
        }
        timestamps.push((timestamp, entry.path()));
    }
    timestamps.sort_by(|left, right| left.0.cmp(&right.0));

    for (timestamp, directory) in timestamps {
        let mut object_ids = Vec::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(StoreError::Io(
                    "symlink in legacy time-index timestamp directory".to_string(),
                ));
            }
            if path.extension().is_some_and(|extension| extension == "tmp") {
                continue;
            }
            if !file_type.is_file() {
                return Err(StoreError::Io(
                    "non-file in legacy time-index timestamp directory".to_string(),
                ));
            }
            let object_id = entry
                .file_name()
                .into_string()
                .map_err(|_| StoreError::Corrupt)?;
            ObjectId::parse(&object_id).map_err(|_| StoreError::Corrupt)?;
            object_ids.push(object_id);
        }
        object_ids.sort();
        object_ids.dedup();
        for object_id in object_ids {
            let key = format!("{TIME_PREFIX}{timestamp}/{object_id}");
            visitor(&key)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mini-store-time-index-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn valid_key(timestamp: u64, seed: u8) -> String {
        let controller =
            did_mini::Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let object = mini_objects::ObjectBuilder::new(mini_objects::ObjectType::POST)
            .timestamp_ms(timestamp)
            .sign(&controller.did(), &controller)
            .unwrap();
        format!("idx/time/{timestamp:020}/{}", object.id().as_str())
    }

    #[test]
    fn record_and_manifest_round_trip_detect_corruption() {
        let key = valid_key(1, 1);
        let record = encode_record(&key, 7).unwrap();
        assert_eq!(decode_record(&record, 7).unwrap(), key);
        assert!(decode_record(&record, 6).is_err());
        let mut damaged = record;
        damaged[11] ^= 1;
        assert!(decode_record(&damaged, 7).is_err());

        let manifest = Manifest {
            generation: 9,
            base_count: 100,
            delta_count: 3,
        };
        let encoded = encode_manifest(&manifest);
        assert_eq!(decode_manifest(&encoded).unwrap(), manifest);
        let mut damaged_manifest = encoded;
        damaged_manifest[20] ^= 1;
        assert!(decode_manifest(&damaged_manifest).is_err());
    }

    #[test]
    fn pending_journal_round_trips() {
        let key = valid_key(2, 3);
        assert_eq!(decode_pending(&encode_pending(&key).unwrap()).unwrap(), key);
    }

    #[test]
    fn a_persisted_metadata_row_is_recovered_from_the_journal() {
        let root = temp_root("journal");
        fs::create_dir_all(root.join("blobs")).unwrap();
        fs::create_dir_all(root.join("meta/idx/time/00000000000000000003")).unwrap();
        let key = valid_key(3, 5);

        with_locked(&root, |index| {
            index.write_pending(&key)?;
            fs::write(root.join("meta").join(&key), b"")?;
            Ok(())
        })
        .unwrap();

        let rows = last(&root, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, key);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_missing_manifested_base_forces_rebuild() {
        let root = temp_root("missing-base");
        fs::create_dir_all(root.join("blobs")).unwrap();
        fs::create_dir_all(root.join("meta/idx/time/00000000000000000004")).unwrap();
        let key = valid_key(4, 7);
        fs::write(root.join("meta").join(&key), b"").unwrap();
        assert_eq!(rebuild(&root).unwrap(), 1);

        let manifest = with_locked(&root, |index| index.read_manifest()).unwrap();
        fs::remove_file(base_path(&root.join(INDEX_DIR), manifest.generation)).unwrap();
        let rows = last(&root, 2).unwrap();
        assert_eq!(rows[0].0, key);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_journaled_delta_append_is_committed_exactly_once() {
        use std::io::Write as _;

        let root = temp_root("journal-delta");
        fs::create_dir_all(root.join("blobs")).unwrap();
        fs::create_dir_all(root.join("meta")).unwrap();
        let key = valid_key(8, 9);

        with_locked(&root, |index| {
            index.write_pending(&key)?;
            let metadata_path = root.join("meta").join(&key);
            fs::create_dir_all(metadata_path.parent().unwrap())?;
            fs::write(&metadata_path, b"")?;

            // Simulate loss of power after the delta record is durable but
            // before the manifest count is advanced.
            let manifest = index.read_manifest()?;
            let delta_path = index.index_root.join(DELTA_FILE);
            let mut delta = OpenOptions::new().append(true).open(delta_path)?;
            delta.write_all(&encode_record(&key, manifest.delta_count)?)?;
            delta.sync_all()?;
            Ok(())
        })
        .unwrap();

        let first = last(&root, 10).unwrap();
        let second = last(&root, 10).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].0, key);
        let manifest = with_locked(&root, |index| index.read_manifest()).unwrap();
        assert_eq!(manifest.base_count + manifest.delta_count, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn manual_compaction_preserves_sorted_unique_rows() {
        let root = temp_root("compact");
        fs::create_dir_all(root.join("blobs")).unwrap();
        fs::create_dir_all(root.join("meta")).unwrap();
        let keys = [valid_key(30, 11), valid_key(10, 13), valid_key(30, 15)];

        with_locked(&root, |index| {
            let mut manifest = index.read_manifest()?;
            for key in &keys {
                let path = root.join("meta").join(key);
                fs::create_dir_all(path.parent().unwrap())?;
                fs::write(path, b"")?;
                index.append_delta(&mut manifest, key)?;
            }
            index.compact(&manifest)?;
            let compacted = index.read_manifest()?;
            assert_eq!(compacted.delta_count, 0);
            assert_eq!(compacted.base_count, 3);
            let (rows, stale) =
                index.query_forward(&compacted, "idx/time/00000000000000000000/", 10)?;
            assert!(!stale);
            let mut expected = keys.to_vec();
            expected.sort();
            assert_eq!(
                rows.into_iter().map(|(key, _)| key).collect::<Vec<_>>(),
                expected
            );
            Ok(())
        })
        .unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn compaction_collapses_a_key_present_in_both_base_and_delta() {
        let root = temp_root("compact-existing-duplicate");
        fs::create_dir_all(root.join("blobs")).unwrap();
        fs::create_dir_all(root.join("meta")).unwrap();
        let existing = valid_key(20, 21);
        let delta_only = valid_key(30, 23);

        let existing_path = root.join("meta").join(&existing);
        fs::create_dir_all(existing_path.parent().unwrap()).unwrap();
        fs::write(existing_path, b"").unwrap();
        assert_eq!(rebuild(&root).unwrap(), 1);

        with_locked(&root, |index| {
            let mut manifest = index.read_manifest()?;
            assert_eq!(manifest.base_count, 1);
            assert_eq!(manifest.delta_count, 0);

            // Exercise the defensive equality branch directly. Normal writes
            // call `contains_key` first, but recovery/corruption hardening must
            // still collapse a key that appears in both sorted inputs.
            index.append_delta(&mut manifest, &existing)?;
            let delta_path = root.join("meta").join(&delta_only);
            fs::create_dir_all(delta_path.parent().unwrap())?;
            fs::write(delta_path, b"")?;
            index.append_delta(&mut manifest, &delta_only)?;

            index.compact(&manifest)?;
            let compacted = index.read_manifest()?;
            assert_eq!(compacted.delta_count, 0);
            assert_eq!(compacted.base_count, 2);
            let (rows, stale) =
                index.query_forward(&compacted, "idx/time/00000000000000000000/", 10)?;
            assert!(!stale);
            assert_eq!(
                rows.into_iter().map(|(key, _)| key).collect::<Vec<_>>(),
                vec![existing.clone(), delta_only.clone()]
            );
            Ok(())
        })
        .unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_oversized_pending_journal_is_rebuilt_without_unbounded_allocation() {
        let root = temp_root("oversized-pending");
        fs::create_dir_all(root.join("blobs")).unwrap();
        fs::create_dir_all(root.join("meta")).unwrap();
        let key = valid_key(12, 17);
        let metadata_path = root.join("meta").join(&key);
        fs::create_dir_all(metadata_path.parent().unwrap()).unwrap();
        fs::write(metadata_path, b"").unwrap();
        assert_eq!(rebuild(&root).unwrap(), 1);

        fs::write(
            root.join(INDEX_DIR).join(PENDING_FILE),
            vec![0x41; MAX_PENDING_BYTES as usize + 1],
        )
        .unwrap();
        let rows = last(&root, 2).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, key);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_oversized_authoritative_time_value_fails_closed() {
        let root = temp_root("oversized-value");
        fs::create_dir_all(root.join("blobs")).unwrap();
        fs::create_dir_all(root.join("meta")).unwrap();
        let key = valid_key(13, 19);
        let metadata_path = root.join("meta").join(&key);
        fs::create_dir_all(metadata_path.parent().unwrap()).unwrap();
        fs::write(
            metadata_path,
            vec![0x42; MAX_TIME_METADATA_VALUE_BYTES as usize + 1],
        )
        .unwrap();
        assert_eq!(rebuild(&root).unwrap(), 1);
        assert_eq!(last(&root, 2), Err(StoreError::LimitExceeded));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_unknown_side_index_entry_fails_closed_during_rebuild() {
        let root = temp_root("unknown-entry");
        fs::create_dir_all(root.join("blobs")).unwrap();
        fs::create_dir_all(root.join("meta")).unwrap();
        assert_eq!(rebuild(&root).unwrap(), 0);
        fs::write(root.join(INDEX_DIR).join("unexpected"), b"hostile").unwrap();
        assert!(matches!(rebuild(&root), Err(StoreError::Io(_))));
        let _ = fs::remove_dir_all(root);
    }
}
