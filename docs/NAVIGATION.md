# Navigating this repository offline

This repo is meant to be understood without GitHub search, a hosted wiki, a
language server, or tribal knowledge — the same property that lets it eventually
be built *from inside Mininet*. `tools/mininet_nav.py` is the lens. It is
Python-3.8+ stdlib only and derives its answers from the checked-out tree.

## Commands

```sh
python3 tools/mininet_nav.py build
python3 tools/mininet_nav.py map
python3 tools/mininet_nav.py search "governed release" --limit 10
python3 tools/mininet_nav.py symbols verify --limit 20
python3 tools/mininet_nav.py files mini-forge --limit 20
```

Useful flags:

```sh
python3 tools/mininet_nav.py --json search IdentityOracle
python3 tools/mininet_nav.py --rebuild files presence
python3 tools/mininet_nav.py --root /path/to/repo map
```

## What `build` generates

- `docs/_generated/REPO_INDEX.json` — machine-readable whole-repo index.
- `docs/_generated/REPO_INDEX.jsonl` — one JSON record per indexed file, easy for
  small offline scripts.
- `docs/_generated/REPO_MAP.md` — human-readable map of crates, docs, symbols,
  and topic hints.

The index records each text file's path, group, byte/line counts, SHA-256 digest,
first title/heading, Markdown headings, Rust symbols, and search keywords. It is
a generated lens over the repository, not an authority over the code.

## When to regenerate

Run `build` whenever crates, docs, public symbols, or dependencies change — ideally
in the same commit. A future CI step should fail if `python3 tools/mininet_nav.py
build` changes `docs/_generated/*`, keeping the repo self-describing by
construction.
