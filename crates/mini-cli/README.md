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
mini repo snapshot-preview --workspace <dir>
mini repo commit-preview <project> --branch <b> --message <m> --workspace <dir>
mini repo commit <project> --branch <b> --message <m> <path>...
mini repo commit <project> --branch <b> --message <m> --workspace <dir> [--confirm-plan <plan-id> | --confirm-snapshot <snapshot-id>]
mini repo checkout <commit-id> <dest-dir>
mini repo branch <project> <branch> [--set <commit-id>]   (raw pointer; NOT canonical -- see `repo status`)
mini repo status <project>                  governed canonical state (resolve_project)

mini pr propose-preview <project> --branch <b> --title <t> --head <commit-id> [--base <id>]
mini pr propose <project> --branch <b> --title <t> --head <commit-id> [--base <id>] [--confirm-plan <plan-id>]
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

mini build run --component <wasm> --store-dir <dir> --scratch-dir <dir> --artifacts-dir <dir> [--capability <cap>]...
mini build serve --addr <host:port> --work-dir <new-dir>
mini build dispatch --peer <host:port> --component <wasm> --workspace <dir> --artifacts-dir <new-dir> [--capability <cap>]...
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
- **Distributed builds are one-shot and explicitly selected.** `build serve`
  accepts one encrypted request and exits. The requester verifies the exact
  request binding and every artifact digest, but a worker remains untrusted:
  its result is evidence, not release approval or provenance quorum. Discovery,
  worker identity/reputation, scheduling, payment, retries, chunking beyond
  the current 15 MiB frame, and persistent supervision remain separate work.
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

## Native migration and collaboration

See [the migration runbook](../../docs/design/native-forge-migration.md) for
`mini git import-snapshot` / `export`, `repo commit --workspace`, structured
`pr list|show|diff|review|approve|merge`, and saved `remote` endpoints. These
compose signed native collaboration without GitHub; source exchange never
installs software or bypasses governance approval.

The [Unix software launch profile](../../docs/design/native-forge-launch.md)
adds executable-preserving, resumable history migration and
`mini service run|status|stop` with explicit peer allowlists and pinned
mutual authentication. Run `python3 tools/check_native_forge.py --release`
for the repeatable coding acceptance gate.

### Review an exact commit and proposal plan

Select an existing CLI profile with `--home` / `--store`, or the defaults
above. These examples assume that profile already knows the project's alias
and trusts the required identities. Preview commands do not initialize an
identity, allocate a sequence, or create signed objects; existing profile lock
metadata may still be opened.

```sh
# Inspect the complete bounded source manifest; file contents are not printed.
mini --json repo snapshot-preview --workspace ./work

# Review source, signer, project, message and current governed parent together.
mini --json repo commit-preview PROJECT --branch main --message "Describe the change" --workspace ./work

# Copy plan_id from that review; keep the inputs identical.
mini --json repo commit PROJECT --branch main --message "Describe the change" --workspace ./work --confirm-plan COMMIT_PLAN_ID

# Use the returned commit_id as the proposal's exact head.
mini --json pr propose-preview PROJECT --branch main --title "Describe the change" --head COMMIT_ID
mini --json pr propose PROJECT --branch main --title "Describe the change" --head COMMIT_ID --confirm-plan PROPOSAL_PLAN_ID
```

`snapshot_id` binds the canonical workspace, complete included tree, file
bytes, empty directories and portable executable flags. Known repository and
`.resident*` metadata are excluded; other private files can still be included
when the owner selects a whole workspace. Inspect the manifest before signing.

The commit `plan_id` additionally binds canonical node/store paths, signer
DIDs, resolved project, branch, message and current governed parent/state.
The proposal has its own `plan_id`, binding its verified exact head, title,
branch, signer, project and current governance base/state. A changed input or
base requires a fresh preview and confirmation. These IDs do not predict future
sequence numbers, timestamps or newly signed object IDs, and confirmation is
not a global transaction against other writers racing after context capture.

Existing positional-path commits, unconfirmed workspace commits and unconfirmed
proposals remain available. `--confirm-snapshot` is the source-only confirmation
option; it does not bind commit metadata and cannot be combined with
`--confirm-plan`. Planned proposals require the current verified governance tip
as their base; they do not silently rebase an earlier plan.

Creating a commit or proposal does not synchronize it to another node, run the
code, approve it or merge it. Exchange objects explicitly through configured
`mini remote sync` / sync sessions, then use the existing exact-head review and
governed merge process. See the migration runbook for peer trust and transport
setup; AI findings and plan confirmations carry no approval weight.

## Resident AI

`mini resident` offers owner-configured local model assistance, bounded source
context, local search/inspection, shipped diagnostics and explicitly confirmed
patches. Hosting/fee commands plan from owner inputs without networking or
payments. See [Resident AI](../../docs/design/resident-ai.md).


### Campaign review evidence

These commands record signed coordination evidence; they do not assign work,
close findings automatically, authorize funding, or constitute Forge approval.
All commands support global `--home`, `--store`, and `--json`.

- `mini campaign create --title <text> --description <text> --expires-ms <unix-ms>`
  creates a campaign with an expiry strictly later than its creation time.
- `mini campaign show <campaign-id>` shows verified findings and attributed
  responses. JSON uses `author_identity_root` consistently across object types.
- `mini finding create --target <object-id> [--head <object-id>] --findings <text>`
  records an observation about an existing local object and optional exact head.
- `mini disposition create --finding <finding-id> --disposition <value> --notes <text>`
  records a response. Values: `corrected`, `accepted-with-gate`, `rejected`,
  `deferred`, `constitutional-disagreement`. Review outcomes such as
  `needs-changes` are not finding responses. Notes must explain correction,
  risk gate, rejection, deferral, or constitutional disagreement as applicable.

Titles are limited to 256 UTF-8 bytes; descriptions, findings and response notes
are limited to 4096 UTF-8 bytes. Campaign snapshots reject more than 1024
selected findings or responses rather than silently truncating evidence.
