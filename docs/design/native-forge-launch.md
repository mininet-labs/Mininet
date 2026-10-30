# Native Forge software launch profile

This profile covers the native developer Forge on Unix machines with reachable
IP endpoints: source migration, checkout/edit/commit, technical review, governed
merge, persistent authenticated exchange, and the existing verified-release and
owner-adoption pipeline. It does not declare a governance cutover, global
personhood, audited real-money operation or hardware readiness.

Run the coding acceptance gate from the repository root with Rust, Git and
Python 3.11+ installed:

```sh
python3 tools/check_native_forge.py --release
```

`--offline` requires cached dependencies. The runner builds the real CLI,
executes the selected CLI source tests without compiling the unrelated Wasmtime
runner dev-dependency, tests the changed Forge/sync libraries, runs Clippy and
formatting checks, and optionally builds the optimized binary. The workspace
manifest/lockfile remain the production build inputs. This is the repeatable
software gate, not a substitute for external security assessment. Use
`--tests native_service` (or other listed suites) for a focused repair check.

## Migration and recovery

`git import-snapshot` now preserves executable files automatically through a
signed executable-file object; no executable-loss opt-in is needed. The old
`--allow-executable-loss` spelling is accepted for compatibility but does not
turn off preservation. Unix checkout restores the portable executable bit;
platforms that cannot represent it fail before publishing a checkout. Upgraded
readers are required for these additive executable objects. Ordinary legacy
file objects and public checkout APIs remain compatible.

To retain source ancestry and merge-parent order, use:

```text
mini --home <home> --json git import-history <local-repo> --ref refs/heads/main --batch-commits 32
```

Repeat the same command until JSON reports `complete:true`. Every successful
partial result explicitly names both the requested source commit and the
complete ancestry frontier actually imported; it never invents a truncated
root. Decrease the batch size when a snapshot/object budget is exceeded.
Verified importer-signed checkpoints make repeats idempotent and allow an
interrupted run to reuse completed work. Changed or missing checkpoint/source
objects fail verification. `--max-commits N` instead requests a complete bounded
import or a failure; it never silently becomes a partial import.

Import batches must fit the file/object/byte caps; `complete:false` is explicit
progress, not a claim that the original tip is already present. A failed batch
never silently signs a different history.

The importer preserves original raw commit evidence separately from signed
Mininet commits. Git names/emails/signatures do not become Mininet authorship.
Unsupported symlinks, submodules, non-supported filenames and objects beyond
Forge's file cap remain explicit input restrictions. Import validation and
native checkout are bounded; this is not an unbounded Git archival appliance.
The current original repository has historical artifacts far larger than its
current source snapshot. Retain the original Git archive when such objects
fall outside this supported source-import profile; do not silently drop them.

Native Git export supports SHA-1 for conventional mirrors and SHA-256. It
preserves native ancestry, file bytes and executable modes within its explicit
object/byte bounds. Git mirror creation never gives the mirror governance
status and never automatically publishes to an external host.

## Pairing and persistent exchange

1. Create independent owner homes with `mini identity init`.
2. Exchange `mini kel export` bundles and root DID fingerprints out of band;
   use `mini kel trust` at both ends after checking the fingerprints.
3. Keep only objects intended for the allowed collaborators in the sharing
   store. Service sharing is at store scope, not project scope.
4. Start the owner-invoked foreground service:

```text
mini --home <server-home> --store <sharing-store> --json service run --addr <listen-ip:port> --allow-store-sharing --allow-peer <client-root-did>
mini --home <client-home> --json remote add team <server-ip:port> --peer <server-root-did>
mini --home <client-home> --json remote sync team
mini --home <server-home> --json service status
mini --home <server-home> --json service stop
```

Repeat `--allow-peer` for additional roots (maximum 64). The default listener
is loopback `127.0.0.1:7400`; changing the bind address is explicit. Saved
endpoint pins do not automatically trust KELs. Authentication requires both
pre-trusted root and delegated device KELs, an allowed client root, and the
exact pinned server root. Signed session proofs bind a domain, direction,
audience and full fresh encrypted-handshake transcript. The authenticated
service never downgrades to anonymous sync. Old unpinned remotes and `sync
listen/connect` remain explicitly legacy tools and do not speak this service
protocol.

The authenticated service protocol is now v2: both peers must upgrade together;
v1 proofs are rejected rather than silently selecting another wire protocol.
Legacy anonymous sync is unchanged. Inventory pages are flow controlled, with
at most 65,536 missing IDs retained per pull. Already-held IDs do not consume
that window. Repeat a sync reporting `complete:false` to continue larger stores.

The service separates eight pending handshakes from four authenticated sessions.
At most two pending handshakes per source IP and one active session per allowed
root are admitted. Handshakes have a two-second absolute deadline. Socket idle
timeouts (30 seconds by default) and absolute session deadlines (four times the
idle timeout) also bound workers. Peers sharing a source IP share its admission
budget; this is resource isolation, not a guarantee against network saturation.

Before persisting each new incoming blob, the service durably reserves its exact
signed byte length against a 2 GiB global store budget and a 512 MiB per-peer
lifetime budget. Existing blob bytes are reconciled at startup. Reservations and
peer accounting survive restart; retries of existing objects cost no additional
reservation. Failed or interrupted writes may conservatively retain reservations.
The quota applies to service ingress, not independently authorized local writers;
filesystem metadata overhead is additional. Quota exhaustion fails the session
without silently claiming completion. Owners must inspect usage and back up the
store before changing/removing accounting files; there is no automatic refund.
Stalled, malformed, unauthorized or replaying connections fail individually.
`service stop` requests graceful shutdown; poll `status` until `running:false`
before backup/restore. An OS lock proves instance liveness; stale state files
and PIDs never grant permission to kill a process or claim a service is alive.
A crash releases the lock, so a new owner invocation can restart cleanly.

For optional Linux user-service supervision, see
`deploy/forge/mininet-forge.service.example`. Install the release binary at
`~/.local/bin/mini`, initialize the example home before starting, replace the
peer placeholder and exchange KELs. Copy the edited unit to the user's systemd
configuration, then use `systemctl --user daemon-reload` and
`systemctl --user start mininet-forge`. The unit starts on loopback, grants
write access only to the owner state and its maintenance lock, and does not
create a privileged system service. Stop the user unit before appliance
maintenance so its restart policy cannot race a restore.

Authenticated transfers persist verified bounded windows. When either
direction has more content than the window budget, `remote sync` returns
`complete:false` / `continue_required:true`; repeat until `complete:true`.
A socket failure is still an error, not a successful partial result. Signed
objects already stored are idempotent and need not be sent again. Windows
bound memory/work per encounter rather than requiring the entire store to
fit in memory at once. `completed` in service status counts completed
sessions, not independently verified whole-store convergence.

Authenticated replication uses bounded inventory pages beyond the legacy
4,096-ID frame. It preserves the original anonymous protocol for its existing
callers. Transfer counts describe local verified ingest, never remote review,
merge, human independence, or release adoption. The existing explicit release
verification and owner approval gates remain necessary after source exchange.

## Local key safety

Identity initialization serializes concurrent creators and creates secret
files with owner-only permissions before writing any bytes. Seeds are
zeroized in temporary buffers. Loading refuses symlinks, hard links,
permissive Unix seed modes, corrupt lengths and interrupted partial creation.
A partial identity is never overwritten with newly generated keys. Preserve
and recover the original files from backup rather than deleting an ambiguous
partial state. Existing valid 64-byte seed files remain compatible.

Owner-controlled directories are still required. Filesystem administrators
and processes running as the owner can replace whole state; this does not
add a hardware keystore or protect a compromised operating system. Existing
passphrase-encrypted appliance backup tooling remains the off-device recovery
route. CLI key rotation and arbitrary-Git/platform support are outside this
narrow launch profile, not hidden claims of cryptographic or hardware safety.

## Acceptance boundary

The software checks exercise executable roundtrips, merge ancestry and
resumable history import, exact-head JSON review, rejection of missing quorum,
real authenticated loopback clients, denied roots, wrong pins, replay proofs,
concurrent/stalled peers, service stop/restart, large paged inventory, and the
existing release-to-install regression. Hardware ceremonies, network field
trials, economic/personhood research and independent cryptographic audit remain
separate evidence supplied by the relevant people.

Validation for this candidate: 130 focused tests passed across the CLI, Forge
and sync suites (including the final history-lock regression). The optimized
Linux CLI build, production Clippy with warnings denied, formatting, diff
checks and systemd unit syntax check passed. A source snapshot at
`44fa8765c109e67940ae162bd9d6317ffff60040` completed Git → signed native
snapshot → SHA-1 mirror with all 1,165 paths/blob IDs and all 15 executable
modes identical; `git fsck --full` passed. This is source-profile evidence,
not a claim that the original 1.5 GB uncompressed historical archive fits the
bounded native importer, nor that independent builders reproduced the binary.
