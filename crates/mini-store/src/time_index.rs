//! Local ordered side index for `FsBackend`'s `idx/time/` metadata rows.
//!
//! The metadata files remain authoritative. This module is a delete-and-rebuild
//! acceleration structure: immutable sorted runs, bounded runs per level, and a
//! one-entry write-ahead journal make steady-state page queries logarithmic in
//! history while keeping crash recovery local and deterministic. No remote
//! index, daemon, database, or authority is introduced.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use mini_objects::ObjectId;

use crate::{Result, StoreError};

pub(crate) const TIME_PREFIX: &str = "idx/time/";

const INDEX_DIR: &str = "ordered/time-v1";
const RUNS_DIR: &str = "runs";
const MARKER_FILE: &str = "version";
const LOCK_FILE: &str = "lock";
const PENDING_FILE: &str = "pending";
const GENERATION_FILE: &str = "generation";
const MARKER: &[u8] = b"mini-store-time-index-v1\n";

const RUN_MAGIC: &[u8; 8] = b"MNTIDX01";
const RUN_VERSION: u16 = 1;
const RUN_HEADER_BYTES: u64 = 20;
const MAX_KEY_BYTES: usize = 192;
const RECORD_BYTES: usize = 8 + 2 + MAX_KEY_BYTES + 8;
const MAX_LEVELS: u8 = 16;
const MAX_RUNS_PER_LEVEL: usize = 4;
const MAX_RUN_FILES: usize = MAX_LEVELS as usize * MAX_RUNS_PER_LEVEL * 2;

const PENDING_MAGIC: &[u8; 8] = b"MNTPND01";

#[derive(Debug, Clone)]
struct RunMeta {
    path: PathBuf,
    level: u8,
    count: u64,
}

#[derive(Debug)]
struct RunReader {
    file: File,
    count: u64,
}

impl RunReader {
    fn open(meta: &RunMeta) -> Result<Self> {
        let mut file = File::open(&meta.path)?;
        let (level, count) = read_run_header(&mut file)?;
        if level != meta.level || count != meta.count {
            return Err(StoreError::Corrupt);
        }
        Ok(Self { file, count })
    }

    fn read_key(&mut self, index: u64) -> Result<String> {
        if index >= self.count {
            return Err(StoreError::Corrupt);
        }
        let offset = RUN_HEADER_BYTES
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
}

#[derive(Debug)]
struct RunWriter {
    file: File,
    temp_path: PathBuf,
    final_path: PathBuf,
    count: u64,
    last_key: Option<String>,
}

impl RunWriter {
    fn new(index_root: &Path, generation: u64) -> Result<Self> {
        let runs = index_root.join(RUNS_DIR);
        let final_path = runs.join(format!("run-{generation:020}.run"));
        let temp_path = runs.join(format!("run-{generation:020}.tmp"));
        remove_regular_if_present(&temp_path, "time-index temporary run")?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&temp_path)?;
        write_run_header(&mut file, 0, 0)?;
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

    fn finish(mut self, level: u8) -> Result<Option<RunMeta>> {
        if level >= MAX_LEVELS {
            return Err(StoreError::LimitExceeded);
        }
        if self.count == 0 {
            drop(self.file);
            remove_regular_if_present(&self.temp_path, "empty time-index run")?;
            return Ok(None);
        }
        self.file.seek(SeekFrom::Start(0))?;
        write_run_header(&mut self.file, level, self.count)?;
        self.file.sync_all()?;
        drop(self.file);
        fs::rename(&self.temp_path, &self.final_path)?;
        Ok(Some(RunMeta {
            path: self.final_path,
            level,
            count: self.count,
        }))
    }
}

#[derive(Debug)]
struct LockedIndex<'a> {
    root: &'a Path,
    index_root: PathBuf,
}

impl<'a> LockedIndex<'a> {
    fn ensure_initialized(&self) -> Result<()> {
        match read_regular(&self.index_root.join(MARKER_FILE), "time-index marker")? {
            Some(bytes) if bytes == MARKER => match self.list_runs() {
                Ok(_) => Ok(()),
                Err(StoreError::Corrupt) | Err(StoreError::LimitExceeded) => {
                    self.rebuild().map(|_| ())
                }
                Err(error) => Err(error),
            },
            Some(_) | None => self.rebuild().map(|_| ()),
        }
    }

    fn recover_pending(&self) -> Result<()> {
        let pending_path = self.index_root.join(PENDING_FILE);
        let Some(bytes) = read_regular(&pending_path, "time-index pending journal")? else {
            return Ok(());
        };
        let key = match decode_pending(&bytes) {
            Ok(key) => key,
            Err(StoreError::Corrupt) => {
                self.rebuild()?;
                remove_regular_if_present(&pending_path, "corrupt time-index journal")?;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        if read_meta_value(self.root, &key)?.is_some() {
            self.write_single_run(&key)?;
            self.compact()?;
        }
        remove_regular_if_present(&pending_path, "time-index pending journal")?;
        Ok(())
    }

    fn write_pending(&self, key: &str) -> Result<()> {
        atomic_write(
            &self.index_root.join(PENDING_FILE),
            &encode_pending(key)?,
        )
    }

    fn clear_pending(&self) -> Result<()> {
        remove_regular_if_present(
            &self.index_root.join(PENDING_FILE),
            "time-index pending journal",
        )
    }

    fn next_generation(&self) -> Result<u64> {
        let path = self.index_root.join(GENERATION_FILE);
        let current = match read_regular(&path, "time-index generation")? {
            None => 0,
            Some(bytes) if bytes.len() == 8 => {
                let mut raw = [0u8; 8];
                raw.copy_from_slice(&bytes);
                u64::from_be_bytes(raw)
            }
            Some(_) => return Err(StoreError::Corrupt),
        };
        let next = current.checked_add(1).ok_or(StoreError::LimitExceeded)?;
        atomic_write(&path, &next.to_be_bytes())?;
        Ok(next)
    }

    fn write_single_run(&self, key: &str) -> Result<()> {
        let generation = self.next_generation()?;
        let mut writer = RunWriter::new(&self.index_root, generation)?;
        writer.push(key)?;
        let _ = writer.finish(0)?;
        Ok(())
    }

    fn list_runs(&self) -> Result<Vec<RunMeta>> {
        let runs_path = self.index_root.join(RUNS_DIR);
        let mut runs = Vec::new();
        for entry in fs::read_dir(&runs_path)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(StoreError::Io(
                    "symlink in ordered time-index runs".to_string(),
                ));
            }
            if !file_type.is_file() {
                return Err(StoreError::Io(
                    "non-file in ordered time-index runs".to_string(),
                ));
            }
            match path.extension().and_then(|extension| extension.to_str()) {
                Some("tmp") => {
                    fs::remove_file(path)?;
                    continue;
                }
                Some("run") => {}
                _ => {
                    return Err(StoreError::Io(
                        "unknown file in ordered time-index runs".to_string(),
                    ))
                }
            }
            let mut file = File::open(&path)?;
            let (level, count) = read_run_header(&mut file)?;
            runs.push(RunMeta { path, level, count });
            if runs.len() > MAX_RUN_FILES {
                return Err(StoreError::LimitExceeded);
            }
        }
        runs.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(runs)
    }

    fn compact(&self) -> Result<()> {
        loop {
            let runs = match self.list_runs() {
                Ok(runs) => runs,
                Err(StoreError::LimitExceeded) => {
                    self.rebuild()?;
                    return Ok(());
                }
                Err(error) => return Err(error),
            };
            let mut selected: Option<(u8, Vec<RunMeta>)> = None;
            for level in 0..MAX_LEVELS {
                let at_level: Vec<RunMeta> = runs
                    .iter()
                    .filter(|run| run.level == level)
                    .cloned()
                    .collect();
                if at_level.len() > MAX_RUNS_PER_LEVEL {
                    if level + 1 >= MAX_LEVELS {
                        return Err(StoreError::LimitExceeded);
                    }
                    selected = Some((level + 1, at_level));
                    break;
                }
            }
            let Some((target_level, inputs)) = selected else {
                return Ok(());
            };
            self.merge_runs(&inputs, target_level)?;
            for input in inputs {
                remove_regular_if_present(&input.path, "compacted time-index run")?;
            }
        }
    }

    fn merge_runs(&self, inputs: &[RunMeta], level: u8) -> Result<()> {
        let generation = self.next_generation()?;
        let mut writer = RunWriter::new(&self.index_root, generation)?;
        let mut readers: Vec<RunReader> = inputs
            .iter()
            .map(RunReader::open)
            .collect::<Result<_>>()?;
        let mut heap: BinaryHeap<Reverse<(String, usize, u64)>> = BinaryHeap::new();
        for (run_index, reader) in readers.iter_mut().enumerate() {
            if reader.count > 0 {
                heap.push(Reverse((reader.read_key(0)?, run_index, 0)));
            }
        }
        let mut last: Option<String> = None;
        while let Some(Reverse((key, run_index, record_index))) = heap.pop() {
            if last.as_deref() != Some(key.as_str()) {
                writer.push(&key)?;
                last = Some(key.clone());
            }
            let next = record_index + 1;
            if next < readers[run_index].count {
                let next_key = readers[run_index].read_key(next)?;
                heap.push(Reverse((next_key, run_index, next)));
            }
        }
        let _ = writer.finish(level)?;
        Ok(())
    }

    fn rebuild(&self) -> Result<usize> {
        self.clear_runs()?;
        atomic_write(&self.index_root.join(GENERATION_FILE), &0u64.to_be_bytes())?;

        let generation = self.next_generation()?;
        let mut writer = RunWriter::new(&self.index_root, generation)?;
        visit_legacy_time_keys(self.root, |key| writer.push(key))?;
        let count = usize::try_from(writer.count).map_err(|_| StoreError::LimitExceeded)?;
        let level = level_for_count(writer.count);
        let _ = writer.finish(level)?;
        atomic_write(&self.index_root.join(MARKER_FILE), MARKER)?;
        self.clear_pending()?;
        Ok(count)
    }

    fn clear_runs(&self) -> Result<()> {
        let runs_path = self.index_root.join(RUNS_DIR);
        for entry in fs::read_dir(runs_path)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() || !file_type.is_file() {
                return Err(StoreError::Io(
                    "unsafe entry in ordered time-index runs".to_string(),
                ));
            }
            fs::remove_file(entry.path())?;
        }
        Ok(())
    }

    fn query_forward(&self, after: &str, limit: usize) -> Result<(Vec<(String, Vec<u8>)>, bool)> {
        validate_after_key(after)?;
        let runs = self.list_runs()?;
        let mut readers: Vec<RunReader> = runs
            .iter()
            .map(RunReader::open)
            .collect::<Result<_>>()?;
        let mut heap: BinaryHeap<Reverse<(String, usize, u64)>> = BinaryHeap::new();
        for (run_index, reader) in readers.iter_mut().enumerate() {
            let record_index = reader.first_greater_than(after)?;
            if record_index < reader.count {
                let key = reader.read_key(record_index)?;
                heap.push(Reverse((key, run_index, record_index)));
            }
        }
        let mut rows = Vec::with_capacity(limit);
        let mut last: Option<String> = None;
        let mut stale = false;
        while rows.len() < limit {
            let Some(Reverse((key, run_index, record_index))) = heap.pop() else {
                break;
            };
            if last.as_deref() != Some(key.as_str()) {
                match read_meta_value(self.root, &key)? {
                    Some(value) => rows.push((key.clone(), value)),
                    None => stale = true,
                }
                last = Some(key.clone());
            }
            let next = record_index + 1;
            if next < readers[run_index].count {
                let next_key = readers[run_index].read_key(next)?;
                heap.push(Reverse((next_key, run_index, next)));
            }
        }
        Ok((rows, stale))
    }

    fn query_reverse(&self, limit: usize) -> Result<(Vec<(String, Vec<u8>)>, bool)> {
        let runs = self.list_runs()?;
        let mut readers: Vec<RunReader> = runs
            .iter()
            .map(RunReader::open)
            .collect::<Result<_>>()?;
        let mut heap: BinaryHeap<(String, usize, u64)> = BinaryHeap::new();
        for (run_index, reader) in readers.iter_mut().enumerate() {
            if reader.count > 0 {
                let record_index = reader.count - 1;
                heap.push((reader.read_key(record_index)?, run_index, record_index));
            }
        }
        let mut rows = Vec::with_capacity(limit);
        let mut last: Option<String> = None;
        let mut stale = false;
        while rows.len() < limit {
            let Some((key, run_index, record_index)) = heap.pop() else {
                break;
            };
            if last.as_deref() != Some(key.as_str()) {
                match read_meta_value(self.root, &key)? {
                    Some(value) => rows.push((key.clone(), value)),
                    None => stale = true,
                }
                last = Some(key.clone());
            }
            if record_index > 0 {
                let previous = record_index - 1;
                let previous_key = readers[run_index].read_key(previous)?;
                heap.push((previous_key, run_index, previous));
            }
        }
        Ok((rows, stale))
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
        index.write_single_run(key)?;
        index.compact()?;
        index.clear_pending()
    })
}

pub(crate) fn last(root: &Path, limit: usize) -> Result<Vec<(String, Vec<u8>)>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    with_locked(root, |index| {
        let (rows, stale) = match index.query_reverse(limit) {
            Ok(result) => result,
            Err(StoreError::Corrupt) | Err(StoreError::LimitExceeded) => {
                index.rebuild()?;
                index.query_reverse(limit)?
            }
            Err(error) => return Err(error),
        };
        if stale {
            index.rebuild()?;
            return index.query_reverse(limit).map(|result| result.0);
        }
        Ok(rows)
    })
}

pub(crate) fn page(
    root: &Path,
    after: &str,
    limit: usize,
) -> Result<Vec<(String, Vec<u8>)>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    with_locked(root, |index| {
        let (rows, stale) = match index.query_forward(after, limit) {
            Ok(result) => result,
            Err(StoreError::Corrupt) | Err(StoreError::LimitExceeded) => {
                index.rebuild()?;
                index.query_forward(after, limit)?
            }
            Err(error) => return Err(error),
        };
        if stale {
            index.rebuild()?;
            return index.query_forward(after, limit).map(|result| result.0);
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
    index.ensure_initialized()?;
    index.recover_pending()?;
    index.compact()?;
    operation(&index)
}

fn ensure_layout(root: &Path) -> Result<PathBuf> {
    ensure_existing_or_create_directory(root, "store root")?;
    let ordered = root.join("ordered");
    ensure_existing_or_create_directory(&ordered, "ordered-index directory")?;
    let index_root = root.join(INDEX_DIR);
    ensure_existing_or_create_directory(&index_root, "time-index directory")?;
    ensure_existing_or_create_directory(&index_root.join(RUNS_DIR), "time-index runs")?;
    Ok(index_root)
}

fn ensure_existing_or_create_directory(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(StoreError::Io(format!("{label} is a symlink")))
        }
        Ok(metadata) if !metadata.is_dir() => {
            Err(StoreError::Io(format!("{label} is not a directory")))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path)?;
            Ok(())
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

fn read_regular(path: &Path, label: &str) -> Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(StoreError::Io(format!("{label} is a symlink")))
        }
        Ok(metadata) if !metadata.is_file() => {
            Err(StoreError::Io(format!("{label} is not a regular file")))
        }
        Ok(_) => Ok(Some(fs::read(path)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
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
    Ok(())
}

fn write_run_header(file: &mut File, level: u8, count: u64) -> Result<()> {
    file.write_all(RUN_MAGIC)?;
    file.write_all(&RUN_VERSION.to_be_bytes())?;
    file.write_all(&[level, 0])?;
    file.write_all(&count.to_be_bytes())?;
    Ok(())
}

fn read_run_header(file: &mut File) -> Result<(u8, u64)> {
    file.seek(SeekFrom::Start(0))?;
    let mut header = [0u8; RUN_HEADER_BYTES as usize];
    file.read_exact(&mut header).map_err(|error| {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            StoreError::Corrupt
        } else {
            error.into()
        }
    })?;
    if &header[..8] != RUN_MAGIC || u16::from_be_bytes([header[8], header[9]]) != RUN_VERSION {
        return Err(StoreError::Corrupt);
    }
    let level = header[10];
    if level >= MAX_LEVELS || header[11] != 0 {
        return Err(StoreError::Corrupt);
    }
    let mut raw_count = [0u8; 8];
    raw_count.copy_from_slice(&header[12..20]);
    let count = u64::from_be_bytes(raw_count);
    let expected = RUN_HEADER_BYTES
        .checked_add(
            count
                .checked_mul(RECORD_BYTES as u64)
                .ok_or(StoreError::Corrupt)?,
        )
        .ok_or(StoreError::Corrupt)?;
    if file.metadata()?.len() != expected {
        return Err(StoreError::Corrupt);
    }
    Ok((level, count))
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
    let mut raw_ordinal = [0u8; 8];
    raw_ordinal.copy_from_slice(&record[..8]);
    let ordinal = u64::from_be_bytes(raw_ordinal);
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
    let mut raw_checksum = [0u8; 8];
    raw_checksum.copy_from_slice(&record[10 + MAX_KEY_BYTES..]);
    if u64::from_be_bytes(raw_checksum) != record_checksum(ordinal, key_bytes) {
        return Err(StoreError::Corrupt);
    }
    let key = String::from_utf8(key_bytes.to_vec()).map_err(|_| StoreError::Corrupt)?;
    validate_time_key(&key)?;
    Ok(key)
}

fn record_checksum(ordinal: u64, key: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in ordinal.to_be_bytes().iter().chain(key.iter()) {
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
    let mut raw_checksum = [0u8; 8];
    raw_checksum.copy_from_slice(&bytes[10 + length..]);
    if u64::from_be_bytes(raw_checksum) != record_checksum(0, key_bytes) {
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
    ObjectId::parse(object_id).map_err(StoreError::Object)?;
    Ok(())
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

fn level_for_count(count: u64) -> u8 {
    if count <= 1 {
        return 0;
    }
    let mut level = 0u8;
    let mut capacity = 1u64;
    while level + 1 < MAX_LEVELS && count > capacity {
        capacity = capacity.saturating_mul(MAX_RUNS_PER_LEVEL as u64 + 1);
        level += 1;
    }
    level
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
        Ok(_) => Ok(Some(fs::read(current)?)),
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
            ObjectId::parse(&object_id).map_err(StoreError::Object)?;
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

    #[test]
    fn record_round_trip_detects_reordering_and_corruption() {
        let key = "idx/time/00000000000000000001/z2DgV4mM8hVtw4d7xAM";
        // Use a real structurally valid id from a tiny object rather than
        // depending on a handwritten multibase vector.
        let root = did_mini::Controller::incept_single_from_seeds(&[1; 32], &[2; 32]).unwrap();
        let object = mini_objects::ObjectBuilder::new(mini_objects::ObjectType::POST)
            .sign(&root.did(), &root)
            .unwrap();
        let key = format!("idx/time/00000000000000000001/{}", object.id().as_str());
        let record = encode_record(&key, 7).unwrap();
        assert_eq!(decode_record(&record, 7).unwrap(), key);
        assert!(decode_record(&record, 6).is_err());

        let mut damaged = record;
        damaged[11] ^= 1;
        assert!(decode_record(&damaged, 7).is_err());
        let _ = key;
    }

    #[test]
    fn pending_journal_round_trips() {
        let root = did_mini::Controller::incept_single_from_seeds(&[3; 32], &[4; 32]).unwrap();
        let object = mini_objects::ObjectBuilder::new(mini_objects::ObjectType::POST)
            .sign(&root.did(), &root)
            .unwrap();
        let key = format!("idx/time/00000000000000000002/{}", object.id().as_str());
        assert_eq!(decode_pending(&encode_pending(&key).unwrap()).unwrap(), key);
    }

    #[test]
    fn a_persisted_metadata_row_is_recovered_from_the_journal() {
        let root = temp_root("journal");
        fs::create_dir_all(root.join("blobs")).unwrap();
        fs::create_dir_all(root.join("meta/idx/time/00000000000000000003")).unwrap();
        let controller = did_mini::Controller::incept_single_from_seeds(&[5; 32], &[6; 32]).unwrap();
        let object = mini_objects::ObjectBuilder::new(mini_objects::ObjectType::POST)
            .sign(&controller.did(), &controller)
            .unwrap();
        let key = format!("idx/time/00000000000000000003/{}", object.id().as_str());

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
}
