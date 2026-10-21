# Review a workspace before native signing

The native CLI now offers an explicit source-snapshot review boundary before
committing a whole workspace. It neither executes source nor invokes Git/GitHub.
Use an existing Mininet identity and project; create/pair those through the
normal owner-controlled workflow before signing a commit.

```text
mini --json repo snapshot-preview --workspace /path/to/source
mini --home /path/to/node --store /path/to/shared-store --json repo commit project-name --branch main --message "Reviewed source" --workspace /path/to/source --confirm-snapshot EXACT_SNAPSHOT_ID
```

Preview is read-only with respect to source, identity and object-store state. It
emits a complete sorted bounded manifest of files and directories, per-file byte
counts/BLAKE3 digests/portable executable modes, directory digests, and the exact
snapshot ID. File contents are not printed. Empty directories are represented.
Reviewed snapshots additionally cap the complete manifest at 20,000 entries and
8 MiB of cumulative relative path bytes. Oversized manifests fail without
truncation or a review ID. Canonical review roots must be UTF-8, at most 8192
bytes and free of control characters. Merkle construction hashes each file
payload once and reuses child digests, avoiding repeated subtree byte scans.
Existing native snapshot depth/node/file/byte/name limits apply, and symlinks or
special files are rejected. Existing `.git`, `.mini`, `.mininet` and `.resident*`
metadata exclusions apply. It is still a full workspace snapshot, not an ignore
engine: other private files, build outputs and untracked files are included.
Keep them outside the selected directory and inspect the entire manifest before
signing. Store a redirected preview outside the source directory so the preview
file itself does not become part of a later snapshot.

The snapshot ID binds the canonical source root, exact paths/tree structure,
file bytes and executable modes. Editing bytes, modes, empty directories or paths
requires a fresh preview. Confirmed commit reads source once, verifies the exact
ID before opening/mutating the object store or signing, then signs the same
immutable in-memory snapshot; it never rereads a different tree after checking.
The checked path requires an already initialized identity and never creates one
implicitly. Legacy unconfirmed commit commands retain their existing behavior.

The source ID does not bind the separately supplied project, branch, message,
signing identity or resolved governed parent. Inspect these explicit arguments
as well. This creates a proposed source commit only: it neither moves a governed
head nor opens/approves/merges a proposal or publishes a release. Source review is
not a test result, security audit, private-file detector or governance approval.

Workspaces must be owner controlled. This does not provide a globally atomic
filesystem snapshot against editors racing individual reads. Its guarantee is
that the accepted bytes match the reviewed manifest ID and the immutable buffer
used for signing. Store I/O failures can leave verified intermediate objects;
the feature does not introduce multi-object transaction atomicity or automatic
rollback. Desktop handoff, signing-metadata review and native proposal submission
remain separate integration work.

## Review the full signing plan

For the stronger workflow, review the source manifest and signing context together:

```text
mini --home /path/to/node --store /path/to/store --json repo commit-preview project-name --branch main --message "Reviewed source" --workspace /path/to/source
mini --home /path/to/node --store /path/to/store --json repo commit project-name --branch main --message "Reviewed source" --workspace /path/to/source --confirm-plan EXACT_PLAN_ID
```

The plan displays and binds the canonical home/store/workspace paths, source
snapshot ID, human/device signing DIDs, resolved project ID, branch, message and
the captured governed parent and governance state. Preview needs an existing
identity and initialized object store; it never initializes keys, allocates a
sequence or signs/publishes objects. Identity lock metadata may be used by the
existing load path. Inspect the entire manifest and the signing metadata before
confirming the exact plan ID.

Confirmation captures the current source and signing context, recomputes the
plan ID and rejects a mismatch before signing/object/sequence writes. Successful
execution uses that same source buffer and resolved signing context, rather than
checking one context and reloading another. A changed signer/profile, project,
message, branch, source or relevant governed state requires a fresh preview.
`--confirm-plan` and the older source-only `--confirm-snapshot` cannot be combined,
and both require workspace mode. Legacy/source-only workflows remain available.

This confirms the displayed plan, not an exact future object ID or a one-use
authorization token: sequence and creation time are allocated during execution.
A repeated explicit invocation may create another proposed commit if the bound
state is unchanged. It grants no merge, release or canonical-head authority.
Capturing governed state is not a distributed compare-and-swap: an external writer
can advance it after capture. Subsequent native exact-head review/merge must still
validate current governed state, as before. Owner-controlled paths and ordinary
store durability limits remain part of the supported profile.
