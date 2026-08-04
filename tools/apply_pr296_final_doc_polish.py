#!/usr/bin/env python3
"""Remove the last two wording inconsistencies from PR #296 documentation."""
from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:120]!r}")
    p.write_text(text.replace(old, new), encoding="utf-8")


replace_once(
    "crates/mini-transport-security/README.md",
    """- `build_verified_onion_route` accepts three live same-network verified
  endpoints and rejects visible endpoint, routing-key, root, or device reuse.
  The lower onion constructor also rejects using any relay routing key as the
  destination key, so no relay becomes the destination by caller mistake, then
  builds the `Entry -> Rendezvous -> Delivery` onion in `mini-relay`. The
  destination key itself remains caller-supplied rather than identity-verified.
  Permanent integration tests start with signed advertisements and local selection, then
""",
    """- `build_verified_onion_route` accepts three live same-network verified
  endpoints, rejects visible endpoint, routing-key, root, or device reuse, and
  delegates construction of the `Entry -> Rendezvous -> Delivery` onion to
  `mini-relay`. The lower constructor rejects using any relay routing key as the
  destination key, so no relay becomes the destination by caller mistake. The
  destination key itself remains caller-supplied rather than identity-verified.
  Permanent integration tests start with signed advertisements and local
  selection, then
""",
)
replace_once(
    "docs/planning/privacy-transport-runtime-convergence.md",
    """| Failed-attempt state atomicity | **PASS** | Freshness/replay values are cloned and committed only after full verification and successful exchange. | Crash-persistent replay state remains the host application's responsibility. |
""",
    """| Failed-attempt state atomicity | **PASS** | Freshness/replay values are cloned and committed only after full verification and successful exchange. | This proves in-process atomicity only; restart-surviving replay state remains unimplemented because the concrete caches have no persistence/import API. |
""",
)
print("PR 296 final documentation polish applied")
