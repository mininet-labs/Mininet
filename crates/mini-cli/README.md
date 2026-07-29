# mini-cli

The developer spine's first real deliverable (Batch 1,
[`docs/design/self-hosted-forge-spine.md`](../../docs/design/self-hosted-forge-spine.md),
D-0066): a real command-line tool wrapping already-real library code
(`did-mini`, `mini-forge`, `mini-store`, `mini-objects`) so a human can
actually drive identity, repo, and governed-review operations without
hand-writing Rust against the library API.

## What this proves

A founder-adopted external audit found no complete path exists from a
developer's change through review to a governed canonical state without
GitHub being the authority. This crate closes that specific gap. Its
integration test (`tests/two_developers.rs`) demonstrates the real claim:
three independent `mini` homes (simulating three separate developers on
three separate machines), sharing nothing but a `--store` filesystem path
(standing in for a synced folder, a USB stick — any medium that copies
files), exchange a signed commit, review it bound to its exact hash,
correctly *refuse* to merge under insufficient quorum, then merge once
quorum is met, converging on identical canonical state as seen from a
third, fully independent home. No GitHub, no daemon, no networking code.

## Commands

```
mini identity init                          create a human root + delegated device
mini identity show                          print this home's DIDs
mini kel export                             print this home's KELs (hex) for another home to trust
mini kel trust <hex>                        trust another home's exported KELs

mini repo init <name> [--maintainer <did>]... [--min-approvals N]
mini repo track <name> <project-id>         local alias for a project someone else created
mini repo commit <project> --branch <b> --message <m> <path>...
mini repo checkout <commit-id> <dest-dir>
mini repo branch <project> <branch> [--set <commit-id>]   (raw pointer; NOT canonical -- see `repo status`)
mini repo status <project>                  governed canonical state (resolve_project)

mini pr propose <project> --branch <b> --title <t> --head <commit-id> [--base <id>]
mini pr approve <pr-id> --head <commit-id> [--reject] [--findings <text>]
mini pr merge <project> <pr-id> [--prev <id>]
mini pr ai-assisted <pr-id> --owner <did>   declare AI-assistance + accountable human owner (informational, never quorum)
mini pr findings <pr-id>                    list recorded findings + AI-assistance declaration

mini team propose <project> --group-id <id> --name <name> --purpose <text> --path <glob>... --term-policy <text> --appeal-policy <text>
                                                   [--lifecycle proposed|incubating]
mini team list [--project <project>]        list verified charter proposals
mini team show <charter-id>                 inspect one charter proposal

mini task create <project> --route <route> --risk <class> --title <title> --description <text> --path <glob>... --evidence <text>... --acceptance <text> --non-goal <text>...
mini task list [--route <route>] [--path <path>] [--limit <n>]
mini task suggest --route <route> [--path <path>] [--limit <n>]   advisory local routing only
mini task show <task-id>                    show the verified task/claim/review graph
mini task claim <task-id> --role <role> --path <glob>... --expires-ms <future-ms> [--base <id>]
mini task review <task-id> --head <exact-id> --kind peer|external|ai --disposition observations|needs-changes|blocked --findings <text> --evidence <text>... --limitations <text>
```

`team` and `task` commands support `--json`. Their objects travel through the
same local store and `mini sync` path as Forge repository objects. A task
suggestion is not an assignment; a claim is not ownership; and a technical
review handoff is not `mini pr approve`. AI evidence has zero approval weight.

Global flags (any position): `--home <path>` (default `~/.mininet`, or
`$MININET_HOME`), `--store <path>` (default `<home>/store`).

## Honest limits

- **No key rotation from the CLI yet.** Identity is reconstructed each
  invocation by replaying the same deterministic inception + device-
  delegation sequence from a saved seed file — real persistence, but
  rotating would need the full KEL persisted, not just the original seeds.
  Deferred; orthogonal to Batch 1's exit condition.
- **No daemon (`mini-devd`).** Every invocation is a fresh process reading
  local files — fine for solo/small-group use, not background sync or live
  event subscriptions. Deferred fast-follow.
- **KEL trust is explicit and manual** (`mini kel export` / `mini kel
  trust`), the honest "trust-on-first-use, no witnesses yet" limitation —
  skipping it is a hard, visible refusal (`resolve_project` returns an
  error), never a silent bypass. `mini-forge`'s own `oracle.rs` module docs
  name this as the same posture `mini-sync`'s verified ingest already uses.
- **Live network sync is a separate invocation.** `mini sync listen` and
  `mini sync connect` already exchange verified objects over the existing
  bearer/sync path (D-0062/D-0080). The new coordination test uses a shared
  store for deterministic CLI coverage; it is not a claim of a live
  multi-machine contributor service or background daemon.
- **No automatic recruitment or personnel directory.** `mini task suggest`
  filters signed task briefs already present locally. It does not infer
  identity, employment, competence, consent, availability, or authority, and
  it does not publish contributor preferences.
- **The per-home sequence counter is concurrency-safe** across parallel
  `mini` invocations against the same home: allocation uses an OS-backed
  exclusive lock. Other files in a home are not thereby made transactional.
- **`repo branch --set` is a raw, ungoverned pointer move** — the same
  primitive `mini_forge::set_branch` always was. Only `repo status`'s
  governed canonical heads (via `resolve_project`) are authoritative.

```sh
cargo test -p mini-cli
cargo run -p mini-cli -- identity init
```

License: CC0-1.0 (public domain).
