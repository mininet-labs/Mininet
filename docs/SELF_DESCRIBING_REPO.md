# The self-describing repository

A network whose promise is "no owner, no off switch, built from inside itself"
cannot depend on an outside service to explain its own code. So the repository
carries its own map, derivable from nothing but the tree.

## The contract

- **Source of truth is the code and the Constitution.** `docs/INVARIANTS.md`
  mirrors SPEC-00 §12; if any generated file, README, or comment disagrees with
  the source or the Constitution, the source wins and the other is in error.
- **The index is a lens, never an authority.** `tools/mininet_nav.py` reads the
  tree and emits `docs/_generated/REPO_INDEX.json`, `REPO_INDEX.jsonl`, and `REPO_MAP.md`. It adds no
  facts; it only reflects what is already there (crate purposes from `//!`
  headers/headings, Rust symbols from source files, topic keywords, file digests,
  and cited `SPEC-`/`D-` references).
- **Regenerate, don't edit.** The generated files are reproducible outputs. Hand
  edits are lost on the next `build` run; that is intentional.

## Why this matters for self-governance

The forge (`mini-forge`, SPEC-11) is the mechanism for building Mininet from
inside Mininet: code lives as content-addressed objects, changes flow through
PR → approval → governed merge → attested release, and no balance ever buys
merge or release authority. For that loop to be real, a stranger with only the
repository must be able to orient, verify, and contribute offline. The
self-describing map is the first rung: it makes the tree legible without trust
in any host.

## Roadmap for this tooling (small, additive)

1. **CI drift check** — fail the build if `mininet_nav.py build` output differs
   from the committed `docs/_generated/*` (keeps the map honest by construction).
2. **Invariant cross-links** — annotate each crate in the index with the
   `INVARIANTS.md` rows it enforces, so the "Enforced by" column and the map stay
   in sync.
3. **Forge-native surface** — once the forge can host its own source, expose the
   same index as a forge object so the map travels with the code over sync.
