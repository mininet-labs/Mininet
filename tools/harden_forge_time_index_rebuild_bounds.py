#!/usr/bin/env python3
"""Bound and validate the side-index directory during rebuilds."""

from __future__ import annotations

import re
from pathlib import Path


path = Path(__file__).resolve().parents[1] / "crates/mini-store/src/time_index.rs"
text = path.read_text(encoding="utf-8")
pattern = r'''    fn clear_base_files\(&self\) -> Result<\(\)> \{.*?\n    \}\n\n    fn cleanup_orphan_bases'''
replacement = '''    fn clear_base_files(&self) -> Result<()> {
        let permanent = [
            MARKER_FILE,
            LOCK_FILE,
            PENDING_FILE,
            MANIFEST_FILE,
            DELTA_FILE,
        ];
        let mut entries = 0usize;
        for entry in fs::read_dir(&self.index_root)? {
            entries = entries
                .checked_add(1)
                .ok_or(StoreError::LimitExceeded)?;
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
            if name.starts_with("base-")
                && (name.ends_with(".idx") || name.ends_with(".tmp"))
            {
                if !file_type.is_file() {
                    return Err(StoreError::Io(
                        "non-file time-index base entry".to_string(),
                    ));
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

    fn cleanup_orphan_bases'''
updated, count = re.subn(pattern, replacement, text, count=1, flags=re.DOTALL)
if count != 1:
    raise SystemExit(f"expected one clear_base_files implementation, found {count}")

test = r'''

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
'''
end = updated.rfind("\n}\n")
if end < 0 or "an_unknown_side_index_entry_fails_closed" in updated:
    raise SystemExit("could not append rebuild-directory test exactly once")
path.write_text(updated[:end] + test + updated[end:], encoding="utf-8")
Path(__file__).unlink()
