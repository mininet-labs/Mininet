# mini-installer

Real, local installation state machine over an already-verified release —
self-hosted forge spine Batch 4 (D-0066/D-0071,
[`docs/design/self-hosted-forge-spine.md`](../../docs/design/self-hosted-forge-spine.md)).

## What this closes

The founder-adopted external audit named this the plan's most
safety-critical, most honestly-named gap: `mini_update::AdoptionState::
adopt` verifies a release and records a device-local decision, but nothing
in that crate — deliberately, per the no-forced-update/no-kill-path freeze
(`docs/INVARIANTS.md` U1) — executes, fetches, or installs anything. This
crate is the separate, real installation layer the design doc calls for.
It never re-derives governance/attestation/timelock trust (that stays
`mini-forge`/`mini-update`'s job); it only acts on an already-verified
`mini_forge::VerifiedRelease`.

## State machine

`Discovered → Verified → Downloading → Staged → PreflightPassed →
AwaitingOwnerApproval → Activating → HealthChecking → Active` or
`RolledBack`, exactly as named in the design doc:

- **`stage()`** — fetches real bytes from the store (`mini_media::assemble`,
  real chunk reassembly) and writes them to a real local staging directory,
  re-verifying the digest independently of `mini-media`'s own internal
  check. Covers `Downloading` → `Staged`.
- **`preflight()`** — re-reads and re-verifies the staged bytes on disk
  immediately before activation, catching corruption or tampering of the
  staging directory in between. → `PreflightPassed`.
- **`activate()`** — atomically flips a `current` symlink to the staged
  release's directory (temp-symlink + `rename`, atomic on the same
  filesystem), but only given an explicit, caller-constructed
  [`OwnerApproval`](src/lib.rs) naming that exact release id. Records
  whatever was active before, as a real file, so rollback survives a
  process restart. → `Activating` → `Active`.
- **`health_check()`** — runs a caller-supplied predicate (this crate
  cannot know what "healthy" means for arbitrary software — the same
  caller-supplied-policy pattern as `mini_update::FreshnessPolicy`). On
  success, stays `Active`. On failure, automatically rolls back to
  whatever was active before; if there was nothing before (first-ever
  activation failed), the `current` pointer is cleared rather than left
  pointing at known-unhealthy software. → `HealthChecking` → `Active` or
  `RolledBack`.
- **`rollback()`** — directly callable too. Consumes the recorded
  "previous" pointer on success, so calling it twice in a row fails
  cleanly (`NoPriorActivation`) instead of toggling back and forth between
  two releases.

Each stage's return type is required as the next stage's input type (a
type-state pipeline), so the sequence is enforced by the compiler, not
just documented. `AwaitingOwnerApproval` is not a blocking call — this
crate never waits on anything, mirroring `mini-update`'s own stance — it is
simply the gap between `preflight()` returning and the caller choosing to
construct an `OwnerApproval` and call `activate()`.

## No forced update, still [FREEZE]

`activate()` requires a caller-constructed `OwnerApproval` naming the
exact release id it authorizes — the typed-domain rule (CLAUDE.md):
authority-exercising functions take a specific named request type, never a
generic "approve"/"go ahead". This crate never constructs one itself and
never calls `activate` on a timer, on startup, or in response to anything
but an explicit caller decision. The actual guarantee is procedural (this
crate's own code never self-invokes activation) — the same honest limit
`mini-update`'s own docs already state about `adopt()`. An automatic
rollback on a failed health check returns the device to whatever was
*already running* — the opposite of forcing new software onto it.

## Honest limits

- **Unix-only.** Activation is a `symlink`/`rename` swap
  (`std::os::unix::fs::symlink`); no Windows support exists yet.
- **No process supervision.** This crate stages files and flips a pointer.
  It does not start, stop, restart, or supervise any process — the
  caller's health-check predicate is where "is the newly activated release
  actually running correctly" gets answered.
- **No real package manager / OS integration.** "Activation" means the
  `current` symlink under an installer-owned directory points at the newly
  staged release's directory. Wiring that into an actual running system
  (restarting a service, swapping a binary on `PATH`, etc.) is the
  caller's job, layered on top of this crate's atomic pointer flip.

## Tests

`tests/installer.rs` — 10 adversarial/integration tests against real files
on real disk (fresh temp directory per test), covering: the full happy
path; a claimed digest that doesn't match the real assembled bytes
(refused at staging); on-disk corruption of a staged artifact between
staging and activation (caught at preflight); an `OwnerApproval` naming
the wrong release (refused); a staged directory removed out from under an
in-flight activation (refused); a failed health check rolling back to the
prior release; a failed health check on the very first activation (leaves
nothing marked active); rolling back with nothing recorded (clean error);
rollback not toggling back and forth on repeated calls; and a full
upgrade-then-explicit-rollback round trip.
