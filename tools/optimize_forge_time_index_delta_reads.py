#!/usr/bin/env python3
"""Read the bounded time-index delta with one file handle per operation."""

from __future__ import annotations

import re
from pathlib import Path


path = Path(__file__).resolve().parents[1] / "crates/mini-store/src/time_index.rs"
text = path.read_text(encoding="utf-8")
pattern = r'''    fn read_delta_keys\(&self, count: u64\) -> Result<Vec<String>> \{.*?\n    \}\n\n    fn truncate_delta'''
replacement = '''    fn read_delta_keys(&self, count: u64) -> Result<Vec<String>> {
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

    fn truncate_delta'''
updated, count = re.subn(pattern, replacement, text, count=1, flags=re.DOTALL)
if count != 1:
    raise SystemExit(f"expected one read_delta_keys implementation, found {count}")
path.write_text(updated, encoding="utf-8")
Path(__file__).unlink()
