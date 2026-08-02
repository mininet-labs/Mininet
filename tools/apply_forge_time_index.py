#!/usr/bin/env python3
"""One-shot integration patch for PR #287.

The large ordered-index implementation and integration tests are committed as
new files. This script makes the small, exact edits to existing mini-store
interfaces, runs the focused Rust checks in CI, then removes itself and its
workflow before the resulting commit is pushed.
"""

from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BACKEND = ROOT / "crates/mini-store/src/backend.rs"
STORE = ROOT / "crates/mini-store/src/store.rs"
LIB = ROOT / "crates/mini-store/src/lib.rs"
SELF = Path(__file__)
WORKFLOW = ROOT / ".github/workflows/forge-time-index-integrate.yml"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one target, found {count}: {old[:100]!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


def patch_lib() -> None:
    replace_once(
        LIB,
        "mod store;\n\npub use backend::{Backend, FsBackend, MemoryBackend};\npub use cache::{CacheTier, ViewConditions};\npub use store::{HeadState, Store};",
        "mod store;\nmod time_index;\n\npub use backend::{Backend, FsBackend, MemoryBackend};\npub use cache::{CacheTier, ViewConditions};\npub use store::{\n    HeadState, Store, TimeCursor, TimePage, MAX_TIME_PAGE_SIZE,\n};",
    )
    replace_once(
        LIB,
        "    /// The backend returned bytes that do not derive the requested id — a\n    /// corrupted or malicious backend (content-addressing violated).\n    Corrupt,\n}",
        "    /// The backend returned bytes that do not derive the requested id, or\n    /// a local reconstructible index failed structural validation.\n    Corrupt,\n    /// A caller-supplied page size or local bounded-index count exceeded its\n    /// declared ceiling.\n    LimitExceeded,\n    /// A continuation cursor did not belong to the requested ordered range.\n    InvalidCursor,\n}",
    )
    replace_once(
        LIB,
        "            StoreError::Corrupt => write!(f, \"backend bytes do not match requested id\"),\n",
        "            StoreError::Corrupt => write!(f, \"corrupt object or reconstructible index\"),\n            StoreError::LimitExceeded => write!(f, \"store limit exceeded\"),\n            StoreError::InvalidCursor => write!(f, \"invalid ordered-index cursor\"),\n",
    )


def patch_backend() -> None:
    replace_once(
        BACKEND,
        "    fn list_meta_prefix_last(&self, prefix: &str, limit: usize) -> Result<Vec<(String, Vec<u8>)>> {\n        let mut all = self.list_meta_prefix(prefix)?;\n        all.reverse();\n        all.truncate(limit);\n        Ok(all)\n    }\n}",
        "    fn list_meta_prefix_last(&self, prefix: &str, limit: usize) -> Result<Vec<(String, Vec<u8>)>> {\n        let mut all = self.list_meta_prefix(prefix)?;\n        all.reverse();\n        all.truncate(limit);\n        Ok(all)\n    }\n\n    /// The first `limit` metadata entries strictly after `after`, in key\n    /// order. The default is semantically correct but not bounded-I/O;\n    /// backends with an ordered index override it.\n    fn list_meta_prefix_page(\n        &self,\n        prefix: &str,\n        after: &str,\n        limit: usize,\n    ) -> Result<Vec<(String, Vec<u8>)>> {\n        let mut all = self.list_meta_prefix(prefix)?;\n        all.retain(|(key, _)| key.as_str() > after);\n        all.truncate(limit);\n        Ok(all)\n    }\n}",
    )

    replace_once(
        BACKEND,
        "        Ok(self\n            .meta\n            .range(prefix.to_string()..upper)\n            .rev()\n            .take(limit)\n            .map(|(k, v)| (k.clone(), v.clone()))\n            .collect())\n    }\n}\n\n/// Filesystem backend",
        "        Ok(self\n            .meta\n            .range(prefix.to_string()..upper)\n            .rev()\n            .take(limit)\n            .map(|(k, v)| (k.clone(), v.clone()))\n            .collect())\n    }\n\n    fn list_meta_prefix_page(\n        &self,\n        prefix: &str,\n        after: &str,\n        limit: usize,\n    ) -> Result<Vec<(String, Vec<u8>)>> {\n        use std::ops::Bound::{Excluded, Included};\n\n        let upper = format!(\"{prefix}\\u{7f}\");\n        let lower = if after < prefix {\n            Included(prefix.to_string())\n        } else {\n            Excluded(after.to_string())\n        };\n        Ok(self\n            .meta\n            .range((lower, Excluded(upper)))\n            .take(limit)\n            .map(|(key, value)| (key.clone(), value.clone()))\n            .collect())\n    }\n}\n\n/// Filesystem backend",
    )

    replace_once(
        BACKEND,
        "    pub fn open(root: &Path) -> Result<Self> {\n        fs::create_dir_all(root.join(\"blobs\"))?;\n        fs::create_dir_all(root.join(\"meta\"))?;\n        Ok(FsBackend {\n            root: root.to_path_buf(),\n        })\n    }\n",
        "    pub fn open(root: &Path) -> Result<Self> {\n        fs::create_dir_all(root.join(\"blobs\"))?;\n        fs::create_dir_all(root.join(\"meta\"))?;\n        Ok(FsBackend {\n            root: root.to_path_buf(),\n        })\n    }\n\n    /// Delete and deterministically reconstruct the local ordered time index\n    /// from authoritative `idx/time/` metadata rows. This is maintenance and\n    /// legacy migration work, not a page-bounded query.\n    pub fn rebuild_time_index(&self) -> Result<usize> {\n        crate::time_index::rebuild(&self.root)\n    }\n",
    )

    replace_once(
        BACKEND,
        "    fn put_meta(&mut self, key: &str, value: &[u8]) -> Result<()> {\n        Self::atomic_write(&self.meta_path(key)?, value)\n    }",
        "    fn put_meta(&mut self, key: &str, value: &[u8]) -> Result<()> {\n        let path = self.meta_path(key)?;\n        if key.starts_with(crate::time_index::TIME_PREFIX) {\n            let root = self.root.clone();\n            return crate::time_index::put_time_meta(&root, key, || {\n                Self::atomic_write(&path, value)\n            });\n        }\n        Self::atomic_write(&path, value)\n    }",
    )

    replace_once(
        BACKEND,
        "        out.sort_by(|a, b| a.0.cmp(&b.0));\n        Ok(out)\n    }\n}\n\n#[cfg(test)]",
        "        out.sort_by(|a, b| a.0.cmp(&b.0));\n        Ok(out)\n    }\n\n    fn list_meta_prefix_last(\n        &self,\n        prefix: &str,\n        limit: usize,\n    ) -> Result<Vec<(String, Vec<u8>)>> {\n        if prefix == crate::time_index::TIME_PREFIX {\n            return crate::time_index::last(&self.root, limit);\n        }\n        let mut all = self.list_meta_prefix(prefix)?;\n        all.reverse();\n        all.truncate(limit);\n        Ok(all)\n    }\n\n    fn list_meta_prefix_page(\n        &self,\n        prefix: &str,\n        after: &str,\n        limit: usize,\n    ) -> Result<Vec<(String, Vec<u8>)>> {\n        if prefix == crate::time_index::TIME_PREFIX {\n            return crate::time_index::page(&self.root, after, limit);\n        }\n        let mut all = self.list_meta_prefix(prefix)?;\n        all.retain(|(key, _)| key.as_str() > after);\n        all.truncate(limit);\n        Ok(all)\n    }\n}\n\n#[cfg(test)]",
    )


def patch_store() -> None:
    replace_once(
        STORE,
        "const MAX_SUBJECT_BYTES: usize = 64;\n\n/// Outcome of applying a head pointer.",
        "const MAX_SUBJECT_BYTES: usize = 64;\n\n/// Largest page accepted by [`Store::since_page`]. The backend may perform\n/// maintenance or one-time migration work, but steady-state returned work and\n/// allocation are bounded by this value.\npub const MAX_TIME_PAGE_SIZE: usize = 1024;\n\n/// Stable continuation cursor for chronological object pages. Ordering is the\n/// exact `idx/time/<timestamp>/<object-id>` order, so equal timestamps remain\n/// unambiguous across page boundaries.\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct TimeCursor {\n    pub timestamp_ms: u64,\n    pub object_id: ObjectId,\n}\n\nimpl TimeCursor {\n    pub fn new(timestamp_ms: u64, object_id: ObjectId) -> Self {\n        Self {\n            timestamp_ms,\n            object_id,\n        }\n    }\n\n    fn index_key(&self) -> String {\n        format!(\n            \"idx/time/{}/{}\",\n            time_key(self.timestamp_ms),\n            self.object_id.as_str()\n        )\n    }\n}\n\n/// One bounded chronological page. `next` is present only when another row\n/// exists after the returned page.\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct TimePage {\n    pub ids: Vec<ObjectId>,\n    pub next: Option<TimeCursor>,\n}\n\n/// Outcome of applying a head pointer.",
    )

    replace_once(
        STORE,
        "    pub fn since(&self, cursor_ms: u64) -> Result<Vec<ObjectId>> {\n        let mut out = Vec::new();\n        for (key, _) in self.backend.list_meta_prefix(\"idx/time/\")? {\n            let (ts, id_str) = parse_time_index_key(&key)?;\n            if ts < cursor_ms {\n                continue;\n            }\n            out.push(ObjectId::parse(id_str)?);\n        }\n        Ok(out)\n    }\n\n    /// The `limit` most-recently-timestamped objects, newest first",
        "    pub fn since(&self, cursor_ms: u64) -> Result<Vec<ObjectId>> {\n        let mut out = Vec::new();\n        for (key, _) in self.backend.list_meta_prefix(\"idx/time/\")? {\n            let (ts, id_str) = parse_time_index_key(&key)?;\n            if ts < cursor_ms {\n                continue;\n            }\n            out.push(ObjectId::parse(id_str)?);\n        }\n        Ok(out)\n    }\n\n    /// Return at most `limit` objects at or after `start_ms`, strictly after\n    /// `after` when a continuation cursor is supplied. The cursor binds both\n    /// timestamp and object id, preventing equal-timestamp omissions or\n    /// duplicates.\n    pub fn since_page(\n        &self,\n        start_ms: u64,\n        after: Option<&TimeCursor>,\n        limit: usize,\n    ) -> Result<TimePage> {\n        if limit > MAX_TIME_PAGE_SIZE {\n            return Err(StoreError::LimitExceeded);\n        }\n        if limit == 0 {\n            return Ok(TimePage {\n                ids: Vec::new(),\n                next: None,\n            });\n        }\n        if after.is_some_and(|cursor| cursor.timestamp_ms < start_ms) {\n            return Err(StoreError::InvalidCursor);\n        }\n        let after_key = after.map_or_else(\n            || format!(\"idx/time/{}/\", time_key(start_ms)),\n            TimeCursor::index_key,\n        );\n        let mut rows = self.backend.list_meta_prefix_page(\n            \"idx/time/\",\n            &after_key,\n            limit + 1,\n        )?;\n        let has_more = rows.len() > limit;\n        if has_more {\n            rows.truncate(limit);\n        }\n\n        let mut ids = Vec::with_capacity(rows.len());\n        let mut last_cursor = None;\n        for (key, _) in rows {\n            let (timestamp_ms, id_str) = parse_time_index_key(&key)?;\n            if timestamp_ms < start_ms {\n                return Err(StoreError::Corrupt);\n            }\n            let object_id = ObjectId::parse(id_str)?;\n            last_cursor = Some(TimeCursor::new(timestamp_ms, object_id.clone()));\n            ids.push(object_id);\n        }\n        Ok(TimePage {\n            ids,\n            next: if has_more { last_cursor } else { None },\n        })\n    }\n\n    /// The `limit` most-recently-timestamped objects, newest first",
    )

    replace_once(
        STORE,
        "fn parse_time_index_key(key: &str) -> Result<(u64, &str)> {\n    let rest = key.strip_prefix(\"idx/time/\").ok_or(StoreError::Corrupt)?;\n    let ts_str = rest.split('/').next().ok_or(StoreError::Corrupt)?;\n    let ts: u64 = ts_str.parse().map_err(|_| StoreError::Corrupt)?;\n    let id_str = rest.get(ts_str.len() + 1..).ok_or(StoreError::Corrupt)?;\n    Ok((ts, id_str))\n}",
        "fn parse_time_index_key(key: &str) -> Result<(u64, &str)> {\n    let rest = key.strip_prefix(\"idx/time/\").ok_or(StoreError::Corrupt)?;\n    let mut parts = rest.split('/');\n    let ts_str = parts.next().ok_or(StoreError::Corrupt)?;\n    let id_str = parts.next().ok_or(StoreError::Corrupt)?;\n    if parts.next().is_some()\n        || ts_str.len() != 20\n        || !ts_str.bytes().all(|byte| byte.is_ascii_digit())\n        || id_str.is_empty()\n    {\n        return Err(StoreError::Corrupt);\n    }\n    let ts: u64 = ts_str.parse().map_err(|_| StoreError::Corrupt)?;\n    Ok((ts, id_str))\n}",
    )


def main() -> None:
    patch_lib()
    patch_backend()
    patch_store()

    # The source file had one deliberately shadowed draft variable; remove it
    # before strict Clippy.
    time_index = ROOT / "crates/mini-store/src/time_index.rs"
    text = time_index.read_text(encoding="utf-8")
    text = text.replace(
        '        let key = "idx/time/00000000000000000001/z2DgV4mM8hVtw4d7xAM";\n',
        "",
        1,
    )
    text = text.replace("        let _ = key;\n", "", 1)
    time_index.write_text(text, encoding="utf-8")

    SELF.unlink()
    WORKFLOW.unlink()

    run("cargo", "fmt", "--all")
    run("cargo", "test", "-p", "mini-store", "--all-features")
    run(
        "cargo",
        "clippy",
        "-p",
        "mini-store",
        "--all-targets",
        "--all-features",
        "--",
        "-D",
        "warnings",
    )


if __name__ == "__main__":
    main()
