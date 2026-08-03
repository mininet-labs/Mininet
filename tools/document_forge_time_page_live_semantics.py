#!/usr/bin/env python3
"""State the exact live-pagination boundary for author-timestamp pages."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
STORE = ROOT / "crates/mini-store/src/store.rs"
PLANNING = ROOT / "docs/planning/forge-bounded-fs-index-pages.md"
DECISIONS = ROOT / "docs/DECISION_LOG.md"
STATUS = ROOT / "docs/STATUS.md"
SPINE = ROOT / "docs/design/self-hosted-forge-spine.md"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one target, found {count}: {old[:120]!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


replace_once(
    STORE,
    "/// Stable continuation cursor for chronological object pages. Ordering is the\n"
    "/// exact `idx/time/<timestamp>/<object-id>` order, so equal timestamps remain\n"
    "/// unambiguous across page boundaries.\n",
    "/// Stable continuation cursor for chronological object pages over a fixed\n"
    "/// local index view. Ordering is the exact\n"
    "/// `idx/time/<timestamp>/<object-id>` order, so equal timestamps remain\n"
    "/// unambiguous across page boundaries. This is a display/browsing cursor, not\n"
    "/// a lossless synchronization frontier: an object arriving later with an\n"
    "/// author-claimed timestamp before the cursor will sort before it.\n",
)

replace_once(
    STORE,
    "    /// Return at most `limit` objects at or after `start_ms`, strictly after\n"
    "    /// `after` when a continuation cursor is supplied. The cursor binds both\n"
    "    /// timestamp and object id, preventing equal-timestamp omissions or\n"
    "    /// duplicates.\n",
    "    /// Return at most `limit` objects at or after `start_ms`, strictly after\n"
    "    /// `after` when a continuation cursor is supplied. For a fixed index view,\n"
    "    /// binding timestamp and object id prevents equal-timestamp omissions or\n"
    "    /// duplicates. This does not create snapshot or sync semantics: concurrent\n"
    "    /// late/backdated arrivals may sort before an already-issued cursor and must\n"
    "    /// be discovered by the normal content/want-list sync path, not this page\n"
    "    /// cursor.\n",
)

replace_once(
    PLANNING,
    "- Timestamps are author claims used for deterministic display ordering, never\n"
    "  freshness, arrival, consensus, or trust evidence.\n",
    "- Timestamps are author claims used for deterministic display ordering, never\n"
    "  freshness, arrival, consensus, or trust evidence. `since_page` is stable for\n"
    "  a fixed local index view, not a lossless live-sync frontier: a later-arriving\n"
    "  object with an older claimed timestamp can sort before the current cursor.\n"
    "  Lossless discovery remains the content/want-list synchronization layer.\n",
)

replace_once(
    DECISIONS,
    "an unbounded compatibility API; only `idx/time` is accelerated; author timestamps\n"
    "are not freshness evidence; parent-directory fsync, cross-index transactionality,\n"
    "and physical weakest-device latency remain unmeasured.\n",
    "an unbounded compatibility API; only `idx/time` is accelerated; author timestamps\n"
    "are not freshness evidence; and a later/backdated arrival can sort before an\n"
    "already-issued page cursor, so this is browsing rather than a lossless sync\n"
    "frontier. Parent-directory fsync, cross-index transactionality, and physical\n"
    "weakest-device latency remain unmeasured.\n",
)

replace_once(
    STATUS,
    "  timestamps are ordering hints, not freshness; physical weakest-device and\n"
    "  parent-directory-fsync behavior remain follow-up. No remote index, daemon,\n",
    "  timestamps are ordering hints, not freshness; a later/backdated arrival can\n"
    "  sort before an issued cursor, so pages are not a lossless sync frontier;\n"
    "  physical weakest-device and parent-directory-fsync behavior remain follow-up.\n"
    "  No remote index, daemon,\n",
)

replace_once(
    SPINE,
    "and author/type/link compound pagination is still open. No hosted index or\n"
    "mandatory daemon is introduced.\n",
    "and author/type/link compound pagination is still open. Pages are stable over a\n"
    "fixed view but are not a lossless sync frontier: later/backdated arrivals can\n"
    "sort before an issued author-timestamp cursor. No hosted index or mandatory\n"
    "daemon is introduced.\n",
)

Path(__file__).unlink()
