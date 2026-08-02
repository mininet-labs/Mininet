#!/usr/bin/env python3
"""Bound every steady-state time-index file read before allocation."""

from __future__ import annotations

import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TIME_INDEX = ROOT / "crates/mini-store/src/time_index.rs"
BACKEND = ROOT / "crates/mini-store/src/backend.rs"
PLANNING = ROOT / "docs/planning/forge-bounded-fs-index-pages.md"
SELF = Path(__file__)
WORKFLOW = ROOT / ".github/workflows/forge-time-index-input-bounds.yml"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one target, found {count}: {old[:120]!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def regex_once(path: Path, pattern: str, replacement: str) -> None:
    text = path.read_text(encoding="utf-8")
    updated, count = re.subn(pattern, replacement, text, count=1, flags=re.DOTALL)
    if count != 1:
        raise SystemExit(f"{path}: expected one regex target, found {count}: {pattern[:120]!r}")
    path.write_text(updated, encoding="utf-8")


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


def patch_time_index() -> None:
    replace_once(
        TIME_INDEX,
        "const MAX_INDEX_DIRECTORY_ENTRIES: usize = 16;\n",
        "const MAX_INDEX_DIRECTORY_ENTRIES: usize = 16;\n"
        "const MAX_PENDING_BYTES: u64 = (8 + 2 + MAX_KEY_BYTES + 8) as u64;\n"
        "const MAX_TIME_METADATA_VALUE_BYTES: u64 = 4 * 1024;\n",
    )

    replace_once(
        TIME_INDEX,
        "        let marker = read_regular(&self.index_root.join(MARKER_FILE), \"time-index marker\")?;\n"
        "        if marker.as_deref() != Some(MARKER) {\n",
        "        let marker = match read_regular_limited(\n"
        "            &self.index_root.join(MARKER_FILE),\n"
        "            \"time-index marker\",\n"
        "            MARKER.len() as u64,\n"
        "        ) {\n"
        "            Ok(marker) => marker,\n"
        "            Err(StoreError::Corrupt) | Err(StoreError::LimitExceeded) => {\n"
        "                self.rebuild()?;\n"
        "                return Ok(());\n"
        "            }\n"
        "            Err(error) => return Err(error),\n"
        "        };\n"
        "        if marker.as_deref() != Some(MARKER) {\n",
    )

    replace_once(
        TIME_INDEX,
        "        let pending = read_regular(\n"
        "            &self.index_root.join(PENDING_FILE),\n"
        "            \"time-index pending journal\",\n"
        "        )?;\n",
        "        let pending = match read_regular_limited(\n"
        "            &self.index_root.join(PENDING_FILE),\n"
        "            \"time-index pending journal\",\n"
        "            MAX_PENDING_BYTES,\n"
        "        ) {\n"
        "            Ok(pending) => pending,\n"
        "            Err(StoreError::Corrupt) | Err(StoreError::LimitExceeded) => {\n"
        "                self.rebuild()?;\n"
        "                return Ok(());\n"
        "            }\n"
        "            Err(error) => return Err(error),\n"
        "        };\n",
    )

    replace_once(
        TIME_INDEX,
        "        let bytes = read_regular(&self.index_root.join(MANIFEST_FILE), \"time-index manifest\")?\n"
        "            .ok_or(StoreError::Corrupt)?;\n",
        "        let bytes = read_regular_limited(\n"
        "            &self.index_root.join(MANIFEST_FILE),\n"
        "            \"time-index manifest\",\n"
        "            MANIFEST_BYTES as u64,\n"
        "        )?\n"
        "        .ok_or(StoreError::Corrupt)?;\n",
    )

    regex_once(
        TIME_INDEX,
        r"fn read_regular\(path: &Path, label: &str\) -> Result<Option<Vec<u8>>> \{.*?\n\}\n\nfn remove_regular_if_present",
        '''fn read_regular_limited(
    path: &Path,
    label: &str,
    max_bytes: u64,
) -> Result<Option<Vec<u8>>> {
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

fn remove_regular_if_present''',
    )

    replace_once(
        TIME_INDEX,
        "        Ok(_) => Ok(Some(fs::read(current)?)),\n",
        "        Ok(metadata) => {\n"
        "            if metadata.len() > MAX_TIME_METADATA_VALUE_BYTES {\n"
        "                return Err(StoreError::LimitExceeded);\n"
        "            }\n"
        "            let capacity = usize::try_from(metadata.len())\n"
        "                .map_err(|_| StoreError::LimitExceeded)?;\n"
        "            let mut value = Vec::with_capacity(capacity);\n"
        "            File::open(current)?\n"
        "                .take(MAX_TIME_METADATA_VALUE_BYTES + 1)\n"
        "                .read_to_end(&mut value)?;\n"
        "            if value.len() as u64 > MAX_TIME_METADATA_VALUE_BYTES {\n"
        "                return Err(StoreError::LimitExceeded);\n"
        "            }\n"
        "            Ok(Some(value))\n"
        "        }\n",
    )

    text = TIME_INDEX.read_text(encoding="utf-8")
    tests = r'''

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
'''
    end = text.rfind("\n}\n")
    if end < 0 or "an_oversized_pending_journal_is_rebuilt" in text:
        raise SystemExit("could not append bounded-read tests exactly once")
    TIME_INDEX.write_text(text[:end] + tests + text[end:], encoding="utf-8")


def patch_memory_backend() -> None:
    replace_once(
        BACKEND,
        "        let upper = format!(\"{prefix}\\u{7f}\");\n"
        "        let lower = if after < prefix {\n",
        "        let upper = format!(\"{prefix}\\u{7f}\");\n"
        "        if after >= upper.as_str() {\n"
        "            return Err(StoreError::InvalidCursor);\n"
        "        }\n"
        "        let lower = if after < prefix {\n",
    )

    text = BACKEND.read_text(encoding="utf-8")
    test = r'''

    #[test]
    fn memory_backend_page_rejects_a_cursor_outside_the_prefix_range() {
        let backend = MemoryBackend::new();
        assert_eq!(
            backend.list_meta_prefix_page("idx/time/", "zzzz", 1),
            Err(StoreError::InvalidCursor)
        );
    }
'''
    end = text.rfind("\n}\n")
    if end < 0 or "memory_backend_page_rejects_a_cursor" in text:
        raise SystemExit("could not append memory cursor test exactly once")
    BACKEND.write_text(text[:end] + test + text[end:], encoding="utf-8")


def update_planning() -> None:
    replace_once(
        PLANNING,
        "- Hostile cursors, zero limits, excessive limits, and symlinked index paths fail\n"
        "  safely.\n",
        "- Hostile cursors, zero limits, excessive limits, and symlinked index paths fail\n"
        "  safely.\n"
        "- Marker, manifest, journal, and authoritative time-row values are size-checked\n"
        "  before allocation; oversized local files rebuild or fail closed.\n",
    )
    replace_once(
        PLANNING,
        "- Steady-state page work is bounded by page size, a 1,024-record delta, and\n"
        "  logarithmic fixed-record base seeks. The index directory itself is capped and\n"
        "  unknown entries fail closed.\n",
        "- Steady-state page work is bounded by page size, a 1,024-record delta, and\n"
        "  logarithmic fixed-record base seeks. The index directory and every control-file\n"
        "  or metadata-value allocation are capped; unknown entries fail closed.\n",
    )


def main() -> None:
    patch_time_index()
    patch_memory_backend()
    update_planning()

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
    run("python3", "tools/mininet_nav.py", "build")


if __name__ == "__main__":
    main()
