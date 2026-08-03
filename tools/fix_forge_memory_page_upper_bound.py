#!/usr/bin/env python3
"""Keep MemoryBackend page semantics total when the cursor is above a prefix."""

from pathlib import Path


path = Path(__file__).resolve().parents[1] / "crates/mini-store/src/backend.rs"
text = path.read_text(encoding="utf-8")
old = '''        if after >= upper.as_str() {
            return Err(StoreError::InvalidCursor);
        }
'''
new = '''        if after >= upper.as_str() {
            return Ok(Vec::new());
        }
'''
if text.count(old) != 1:
    raise SystemExit(f"expected one upper-bound guard, found {text.count(old)}")
text = text.replace(old, new, 1)
old_test = '''    #[test]
    fn memory_backend_page_rejects_a_cursor_outside_the_prefix_range() {
        let backend = MemoryBackend::new();
        assert_eq!(
            backend.list_meta_prefix_page("idx/time/", "zzzz", 1),
            Err(StoreError::InvalidCursor)
        );
    }
'''
new_test = '''    #[test]
    fn memory_backend_page_above_the_prefix_range_is_empty() {
        let backend = MemoryBackend::new();
        assert!(backend
            .list_meta_prefix_page("idx/time/", "zzzz", 1)
            .unwrap()
            .is_empty());
    }
'''
if text.count(old_test) != 1:
    raise SystemExit(f"expected one upper-bound test, found {text.count(old_test)}")
path.write_text(text.replace(old_test, new_test, 1), encoding="utf-8")
Path(__file__).unlink()
