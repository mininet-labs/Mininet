# Native Forge migration and collaboration

This implements the practical migration/workspace/review portions of the
Batch 5 plan in `self-hosted-forge-spine.md`. GitHub can remain a replaceable
mirror: importing source, editing, exchanging proposals, reviewing exact
commits and resolving governed heads use local Mininet identities and stores.
This engineering feature does not declare a governance cutover or close an
external release gate.

## Bring existing source into Mininet

Install Git and build the CLI with `cargo build --locked -p mini-cli --bin mini`.
Use a local clone; this command never fetches from a remote or executes a
checkout, filter, build or hook:

```text
mini --home <author-home> --json git import-snapshot <local-git-repo> --ref refs/heads/main
```

The result contains `commit_id`, `provenance_id`, `source_format` and
`source_commit`. SHA-1 and SHA-256 repositories are accepted. Every object read
is rehashed, and the entire supported snapshot is validated before signing.
The importing identity signs the snapshot and its provenance; Git author
strings and embedded Git signatures remain source evidence, not authenticated
Mininet authorship. The import is one snapshot, not the original history.

Executable files require explicit `--allow-executable-loss`: the current
Forge file object stores bytes but no executable bit. Original paths/modes/IDs
are retained in signed provenance. The option permits that known metadata
loss; it never permits dropping files. Symlinks, submodules, unsupported names,
oversized objects and excessive work are rejected. Git LFS pointer files are
ordinary source bytes; LFS content is not fetched. See `git.rs` for exact
resource caps. Import does not move a governed branch or approve a proposal.

Native tree components support safe ASCII filenames up to 255 bytes, including
the long document filenames in this repository. Older peers must upgrade to
read components longer than 64 bytes. Branch/project/governance name limits
and wire framing are unchanged. Spaces and Unicode names remain unsupported.

## Work on and review exact source

Initialize a project with the intended reviewer roots, exchange KELs through
`mini kel export` / `mini kel trust`, and verify those identity fingerprints
out of band. Project membership is not inferred from GitHub access.

```text
mini --home <author-home> --json repo init <project> --maintainer <author-did> --maintainer <reviewer-did> --min-approvals 1
mini --home <author-home> repo checkout <commit-id> <new-workspace-directory>
mini --home <author-home> --json repo commit <project> --branch main --message "describe change" --workspace <workspace-directory>
mini --home <author-home> --json pr propose <project> --branch main --title "describe change" --head <commit-id>
mini --home <reviewer-home> --json pr list <project-id>
mini --home <reviewer-home> --json pr show <proposal-id>
mini --home <reviewer-home> --json pr diff <proposal-id>
mini --home <reviewer-home> --json pr review <proposal-id> --head <commit-id> --findings "technical evidence and limitations"
mini --home <reviewer-home> --json pr approve <proposal-id> --head <commit-id> --findings "exact source reviewed"
mini --home <maintainer-home> --json pr merge <project-id> <proposal-id>
```

Use the `project_id`, `commit_id`, `pr_id`, `review_id` and `merge_id` JSON
fields to chain commands. Source snapshots are deterministic and bounded;
`.git`, `.mininet` and `.mini` metadata are excluded. Snapshotting rejects
symlinks and special files. Checkout stages a complete tree before publication
and requires a new destination: it never overwrites an existing workspace.
`--workspace` preserves root-relative paths and supports deletion of all files.
It is a complete snapshot, not Git's index or an ignore-pattern implementation;
keep build outputs, secrets and personal files outside the selected workspace.
Filesystem operations assume owner-controlled directories, not protection from
a malicious local process swapping paths during an operation.

`pr show` reports the exact reviewed head, current governance tip, stale base,
fork status, distinct-root approval count and verified review evidence. Diffs
report added/changed/deleted paths and bounded text; binary/large content is
explicitly omitted. `pr review` records findings without an approval verdict,
the appropriate route for AI technical evidence. An AI assistance declaration
does not turn AI evidence into human approval. Merge refuses absent quorum,
wrong project/base or an unauthorized merger. Fork flags remain visible;
neither a provisional branch display nor a review is release finality.
Protocol repositories retain their existing minimum approval floors.

## Exchange updates directly

Keep collaboration objects in a store intended for sharing. Full sync offers
the store's signed object inventory, not just one project or branch. Never
place unrelated private objects in that store expecting project filtering.

```text
mini --home <peer-home> sync listen --addr <listen-ip:port> --repeat 3
mini --home <author-home> --json remote add team <peer-ip:port>
mini --home <author-home> --json remote list
mini --home <author-home> --json remote sync team
mini --home <author-home> --json remote remove team
```

Saved endpoints are local configuration, not trusted identities. Numeric-address
connections have a 10-second connect timeout and 30-second socket I/O timeouts
(the latter bound stalled reads/writes, not total session duration). Addresses are
explicit IPv4/IPv6 socket addresses; DNS/discovery and endpoint authentication
are not added here. Sync exchanges signed objects bidirectionally over the
existing encrypted channel. KEL trust remains explicit. JSON reports local
ingest counts, not a receipt proving the peer accepted or merged an update.
`canonicalized:false` means this command did not create a merge. Store contents
may still include independently governed heads received from peers.
The listener serves the requested number of connections sequentially and exits;
there is no new background daemon, public hosting service or NAT traversal.

## Keep a replaceable Git mirror

```text
mini --home <author-home> --json git export <commit-id> --to <new-bare-repository> --object-format sha1
```

SHA-1 export supports conventional Git hosts; SHA-256 export is also available.
Export creates a new bare repository and verifies written object IDs before
publishing its branch. It never overwrites an existing mirror or pushes to a
network host. Git export does not carry Mininet signatures, review authority
or canonical status: distribute signed Mininet objects as the authoritative
collaboration record. The existing release/provenance/installer commands remain
the route to reproducible evidence, governed release verification and each
owner's explicit adoption; source sync never installs updates.

## Release acceptance

The focused tests cover real SHA-1/SHA-256 Git import/export, provenance,
unsupported input and overwrite refusal; workspace roundtrips, exclusions and
symlink rejection; exact-head JSON review with two reviewers; and saved-peer
proposal transfer over loopback TCP between independent stores. Existing
governance and developer/network collaboration suites remain regression checks.
Public-host operations, arbitrary full-history Git migration, executable-mode
roundtrips, daemon availability and external audit are separate remaining work.

The actual 1,161-file repository snapshot at Git commit
`0b7b93afaa3ac5a65ac382b78d5f039bc747f53a` was imported and exported to a
SHA-1 bare mirror: every path and blob ID matched, and `git fsck --full`
passed. The explicitly permitted metadata loss affected 15 executable bits.
The CLI test sources were also run through a temporary production-dependency
harness excluding only the unused Wasmtime runner dev-dependency, avoiding
compilation of the unrelated sandbox executor. Production CLI build and Clippy
used the repository's unchanged manifest and lockfile.
