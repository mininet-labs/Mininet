#!/usr/bin/env python3
"""
mininet_nav.py - zero-dependency repository navigator for Mininet.

The goal is to make an offline zip of the repo self-describing and searchable
without GitHub, a language server, ripgrep, a database, or any external service.

Common commands:
  python3 tools/mininet_nav.py build
  python3 tools/mininet_nav.py map
  python3 tools/mininet_nav.py search "governed release"
  python3 tools/mininet_nav.py symbols "verify"
  python3 tools/mininet_nav.py files "mini-forge"
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from collections import Counter
from pathlib import Path
from typing import Any, Dict, Iterable, List, Sequence, Tuple

TEXT_SUFFIXES = {
    ".rs", ".md", ".toml", ".yml", ".yaml", ".json", ".jsonl",
    ".txt", ".gitignore", ".lock", ".sh", ".py",
}

EXCLUDED_DIRS = {
    ".git", "target", ".idea", ".vscode", "node_modules", "__pycache__",
}

GENERATED_DIR = Path("docs/_generated")
INDEX_JSON = GENERATED_DIR / "REPO_INDEX.json"
INDEX_JSONL = GENERATED_DIR / "REPO_INDEX.jsonl"
MAP_MD = GENERATED_DIR / "REPO_MAP.md"

STOPWORDS = {
    "the", "and", "for", "with", "that", "this", "from", "into", "mini",
    "mininet", "crate", "crates", "docs", "src", "test", "tests", "readme",
    "pub", "fn", "let", "mut", "use", "mod", "impl", "where", "which",
    "have", "has", "are", "was", "were", "not", "all", "any", "can",
    "should", "must", "will", "when", "then", "than", "only", "over",
    "under", "before", "after", "without", "within", "between", "their",
}

TOPIC_HINTS = {
    "identity": ["did-mini", "kel", "delegation", "controller", "pre-rotation"],
    "crypto": ["mini-crypto", "x25519", "hkdf", "aead", "multihash"],
    "channel": ["mini-bearer", "channel", "handshake", "bearer"],
    "presence": ["mini-presence", "attestation", "rtt", "replay", "range"],
    "reward": ["mini-reward", "accrual", "maturation", "vesting"],
    "keystone": ["mini-keystone", "demo", "end-to-end", "presence"],
    "objects": ["mini-objects", "signed object", "links", "envelope"],
    "store": ["mini-store", "content-addressed", "head", "want"],
    "sync": ["mini-sync", "pull", "ingest", "carrier", "reconcile"],
    "media": ["mini-media", "chunk", "manifest", "assemble"],
    "social": ["mini-social", "profile", "follow", "feed"],
    "forge": ["mini-forge", "pull request", "approval", "merge", "release"],
    "governance": ["policy", "maintainer", "quorum", "human", "vote"],
    "release": ["verify_governed_release", "attestation", "artifact", "timelock"],
    "roadmap": ["roadmap", "beta status", "decision log", "invariant"],
}

RUST_SYMBOL_RE = re.compile(
    r"^\s*(?:pub\s+(?:\([^)]*\)\s+)?)?"
    r"(?:(?:async|const|unsafe)\s+)*"
    r"(struct|enum|trait|fn|mod|const|type)\s+([A-Za-z_][A-Za-z0-9_]*)",
)

IMPL_RE = re.compile(r"^\s*impl(?:<[^>]+>)?\s+([^\s{]+)")
HEADING_RE = re.compile(r"^(#{1,6})\s+(.+?)\s*$")
WORD_RE = re.compile(r"[A-Za-z][A-Za-z0-9_\-]{2,}")


def repo_root(start: Path | None = None) -> Path:
    here = (start or Path.cwd()).resolve()
    for candidate in [here, *here.parents]:
        if (candidate / "Cargo.toml").exists() and (candidate / "README.md").exists():
            return candidate
    return here


def is_text_file(path: Path) -> bool:
    if path.name in {"LICENSE", "README", "CONTRIBUTING"}:
        return True
    if path.suffix in TEXT_SUFFIXES:
        return True
    return False


def iter_files(root: Path, include_generated: bool = False) -> Iterable[Path]:
    for base, dirs, files in os.walk(root):
        rel_base = Path(base).relative_to(root)
        dirs[:] = [d for d in dirs if d not in EXCLUDED_DIRS]
        if not include_generated and rel_base == GENERATED_DIR:
            dirs[:] = []
            continue
        for name in sorted(files):
            path = Path(base) / name
            rel = path.relative_to(root)
            if not include_generated and rel.parts[:2] == ("docs", "_generated"):
                continue
            if is_text_file(path):
                yield path


def read_text(path: Path) -> str:
    try:
        data = path.read_bytes()
    except OSError:
        return ""
    if b"\x00" in data:
        return ""
    return data.decode("utf-8", errors="replace")


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def extract_headings(text: str) -> List[Dict[str, Any]]:
    headings: List[Dict[str, Any]] = []
    for lineno, line in enumerate(text.splitlines(), start=1):
        m = HEADING_RE.match(line)
        if m:
            headings.append({"line": lineno, "level": len(m.group(1)), "text": m.group(2).strip()})
    return headings


def extract_rust_symbols(text: str) -> List[Dict[str, Any]]:
    out: List[Dict[str, Any]] = []
    for lineno, line in enumerate(text.splitlines(), start=1):
        m = RUST_SYMBOL_RE.match(line)
        if m:
            out.append({"line": lineno, "kind": m.group(1), "name": m.group(2)})
            continue
        im = IMPL_RE.match(line)
        if im:
            out.append({"line": lineno, "kind": "impl", "name": im.group(1).strip()})
    return out[:200]


def title_for(path: Path, text: str, headings: Sequence[Dict[str, Any]]) -> str:
    if headings:
        return str(headings[0]["text"])
    for line in text.splitlines():
        stripped = line.strip()
        if stripped and not stripped.startswith(("//", "#", "/*", "*")):
            return stripped[:120]
    return path.name


def keywords_for(rel: str, text: str, headings: Sequence[Dict[str, Any]], symbols: Sequence[Dict[str, Any]]) -> List[str]:
    seed = rel + "\n" + "\n".join(str(h["text"]) for h in headings) + "\n" + "\n".join(str(s["name"]) for s in symbols)
    words = [w.lower().strip("-_") for w in WORD_RE.findall(seed + "\n" + text[:5000])]
    counts = Counter(w for w in words if w and w not in STOPWORDS and len(w) <= 48)
    return [w for w, _ in counts.most_common(24)]


def classify(path: Path) -> str:
    parts = path.parts
    if len(parts) >= 3 and parts[0] == "crates":
        return "crate:" + parts[1]
    if parts[0] == "docs":
        return "docs"
    if parts[0] == "tools":
        return "tools"
    if parts[0] == ".github":
        return "ci"
    return "root"


def build_index(root: Path) -> Dict[str, Any]:
    files: List[Dict[str, Any]] = []
    crate_names: Dict[str, Dict[str, Any]] = {}
    for path in sorted(iter_files(root), key=lambda p: str(p.relative_to(root))):
        rel = path.relative_to(root).as_posix()
        text = read_text(path)
        headings = extract_headings(text)
        symbols = extract_rust_symbols(text) if path.suffix == ".rs" else []
        entry = {
            "path": rel,
            "group": classify(Path(rel)),
            "bytes": path.stat().st_size,
            "lines": text.count("\n") + (1 if text else 0),
            "sha256": sha256_file(path),
            "title": title_for(path, text, headings),
            "headings": headings[:80],
            "symbols": symbols,
            "keywords": keywords_for(rel, text, headings, symbols),
        }
        files.append(entry)
        if rel.endswith("Cargo.toml") and "/" in rel:
            parent = rel.split("/")[-2]
            crate_names[parent] = {"manifest": rel, "group": classify(Path(rel))}

    index = {
        "format": "mininet.repo_index.v1",
        "purpose": "Offline navigation/search metadata. Regenerate with tools/mininet_nav.py build.",
        "root_files": [f["path"] for f in files if f["group"] == "root"],
        "counts": {
            "files": len(files),
            "crates": len(crate_names),
            "rust_symbols": sum(len(f["symbols"]) for f in files),
            "headings": sum(len(f["headings"]) for f in files),
        },
        "crates": crate_names,
        "topics": TOPIC_HINTS,
        "files": files,
    }
    return index


def write_index(root: Path, index: Dict[str, Any]) -> None:
    gen = root / GENERATED_DIR
    gen.mkdir(parents=True, exist_ok=True)
    (root / INDEX_JSON).write_text(json.dumps(index, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    with (root / INDEX_JSONL).open("w", encoding="utf-8") as fh:
        for entry in index["files"]:
            fh.write(json.dumps(entry, sort_keys=True) + "\n")
    (root / MAP_MD).write_text(render_map(index), encoding="utf-8")


def render_map(index: Dict[str, Any]) -> str:
    lines: List[str] = []
    lines.append("# Generated repository map")
    lines.append("")
    lines.append("Generated by `python3 tools/mininet_nav.py build`. Do not edit by hand.")
    lines.append("")
    c = index["counts"]
    lines.append(f"- Files indexed: {c['files']}")
    lines.append(f"- Crates: {c['crates']}")
    lines.append(f"- Rust symbols: {c['rust_symbols']}")
    lines.append(f"- Markdown headings: {c['headings']}")
    lines.append("")
    lines.append("## Crates")
    lines.append("")
    for name in sorted(index["crates"]):
        readme = f"crates/{name}/README.md"
        title = file_title(index, readme)
        lines.append(f"- `{name}` - {title} (`{readme}`)")
    lines.append("")
    lines.append("## Docs")
    lines.append("")
    for entry in index["files"]:
        if entry["group"] == "docs" and entry["path"].endswith(".md"):
            lines.append(f"- `{entry['path']}` - {entry['title']}")
    lines.append("")
    lines.append("## Top symbols by file")
    lines.append("")
    for entry in index["files"]:
        if entry["symbols"]:
            syms = ", ".join(f"{s['kind']} {s['name']}" for s in entry["symbols"][:12])
            lines.append(f"- `{entry['path']}`: {syms}")
    lines.append("")
    lines.append("## Topic hints")
    lines.append("")
    for topic, hints in sorted(index["topics"].items()):
        lines.append(f"- `{topic}`: " + ", ".join(f"`{h}`" for h in hints))
    lines.append("")
    return "\n".join(lines)


def file_title(index: Dict[str, Any], path: str) -> str:
    for entry in index["files"]:
        if entry["path"] == path:
            return entry["title"]
    return ""


def load_or_build(root: Path, rebuild: bool = False) -> Dict[str, Any]:
    idx_path = root / INDEX_JSON
    if not rebuild and idx_path.exists():
        return json.loads(idx_path.read_text(encoding="utf-8"))
    index = build_index(root)
    write_index(root, index)
    return index


def normalize_terms(query: str) -> List[str]:
    expanded = [query]
    lower = query.lower().strip()
    if lower in TOPIC_HINTS:
        expanded.extend(TOPIC_HINTS[lower])
    joined = " ".join(expanded)
    return [w.lower() for w in WORD_RE.findall(joined) if w.lower() not in STOPWORDS]


def score_entry(entry: Dict[str, Any], terms: Sequence[str]) -> int:
    hay = " ".join([
        entry.get("path", ""), entry.get("title", ""),
        " ".join(entry.get("keywords", [])),
        " ".join(str(h.get("text", "")) for h in entry.get("headings", [])),
        " ".join(str(s.get("name", "")) for s in entry.get("symbols", [])),
    ]).lower()
    score = 0
    for term in terms:
        if term in hay:
            score += 4
        if term in entry.get("path", "").lower():
            score += 5
    return score


def line_matches(text: str, terms: Sequence[str]) -> List[Tuple[int, str]]:
    matches: List[Tuple[int, str]] = []
    for lineno, line in enumerate(text.splitlines(), start=1):
        low = line.lower()
        if all(t in low for t in terms) or (terms and any(t in low for t in terms)):
            matches.append((lineno, line.rstrip()))
        if len(matches) >= 8:
            break
    return matches


def search(root: Path, index: Dict[str, Any], query: str, limit: int) -> List[Dict[str, Any]]:
    terms = normalize_terms(query)
    results: List[Dict[str, Any]] = []
    for entry in index["files"]:
        path = root / entry["path"]
        if not path.exists():
            continue
        text = read_text(path)
        matches = line_matches(text, terms)
        score = score_entry(entry, terms) + (10 * len(matches))
        if score > 0 or matches:
            results.append({"score": score, "entry": entry, "matches": matches})
    results.sort(key=lambda r: (-r["score"], r["entry"]["path"]))
    return results[:limit]


def print_results(results: Sequence[Dict[str, Any]], json_output: bool = False) -> None:
    if json_output:
        safe = []
        for r in results:
            safe.append({
                "score": r["score"],
                "path": r["entry"]["path"],
                "title": r["entry"].get("title", ""),
                "matches": [{"line": n, "text": t} for n, t in r["matches"]],
            })
        print(json.dumps(safe, indent=2, sort_keys=True))
        return
    for r in results:
        entry = r["entry"]
        print(f"{entry['path']}  score={r['score']}  {entry.get('title','')}")
        for lineno, line in r["matches"][:4]:
            print(f"  {lineno}: {line[:180]}")
        print()


def symbols(index: Dict[str, Any], query: str, limit: int) -> List[Dict[str, Any]]:
    q = query.lower()
    out: List[Dict[str, Any]] = []
    for entry in index["files"]:
        for sym in entry.get("symbols", []):
            name = str(sym["name"])
            if q in name.lower() or q in str(sym["kind"]).lower():
                out.append({"path": entry["path"], "line": sym["line"], "kind": sym["kind"], "name": name})
    out.sort(key=lambda x: (x["path"], x["line"]))
    return out[:limit]


def print_symbols(items: Sequence[Dict[str, Any]], json_output: bool = False) -> None:
    if json_output:
        print(json.dumps(list(items), indent=2, sort_keys=True))
        return
    for item in items:
        print(f"{item['path']}:{item['line']}  {item['kind']} {item['name']}")


def files(index: Dict[str, Any], query: str | None, limit: int) -> List[Dict[str, Any]]:
    entries = index["files"]
    if query:
        terms = normalize_terms(query)
        scored = [(score_entry(e, terms), e) for e in entries]
        scored = [(s, e) for s, e in scored if s > 0]
        scored.sort(key=lambda x: (-x[0], x[1]["path"]))
        return [e for _, e in scored[:limit]]
    return entries[:limit]


def print_files(items: Sequence[Dict[str, Any]], json_output: bool = False) -> None:
    if json_output:
        print(json.dumps(list(items), indent=2, sort_keys=True))
        return
    for e in items:
        print(f"{e['path']}  {e['lines']} lines  {e['title']}")


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Offline Mininet repo navigator/search tool")
    parser.add_argument("--root", default=None, help="repository root, default: auto-detect")
    parser.add_argument("--rebuild", action="store_true", help="rebuild the checked-in index before running")
    parser.add_argument("--json", action="store_true", help="print machine-readable JSON")
    sub = parser.add_subparsers(dest="cmd", required=True)

    sub.add_parser("build", help="rebuild docs/_generated navigation files")
    sub.add_parser("map", help="print the generated repository map")

    p_search = sub.add_parser("search", help="search files using the local index")
    p_search.add_argument("query")
    p_search.add_argument("--limit", type=int, default=10)

    p_symbols = sub.add_parser("symbols", help="search Rust symbols")
    p_symbols.add_argument("query")
    p_symbols.add_argument("--limit", type=int, default=50)

    p_files = sub.add_parser("files", help="list indexed files, optionally by topic/query")
    p_files.add_argument("query", nargs="?")
    p_files.add_argument("--limit", type=int, default=80)

    args = parser.parse_args(argv)
    root = repo_root(Path(args.root)) if args.root else repo_root()

    if args.cmd == "build":
        index = build_index(root)
        write_index(root, index)
        if args.json:
            print(json.dumps(index["counts"], indent=2, sort_keys=True))
        else:
            print(f"wrote {INDEX_JSON}, {INDEX_JSONL}, {MAP_MD}")
            print(json.dumps(index["counts"], indent=2, sort_keys=True))
        return 0

    index = load_or_build(root, rebuild=args.rebuild)
    if args.cmd == "map":
        map_path = root / MAP_MD
        if not map_path.exists() or args.rebuild:
            write_index(root, index)
        print(map_path.read_text(encoding="utf-8"))
        return 0
    if args.cmd == "search":
        print_results(search(root, index, args.query, args.limit), json_output=args.json)
        return 0
    if args.cmd == "symbols":
        print_symbols(symbols(index, args.query, args.limit), json_output=args.json)
        return 0
    if args.cmd == "files":
        print_files(files(index, args.query, args.limit), json_output=args.json)
        return 0
    parser.error("unknown command")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
