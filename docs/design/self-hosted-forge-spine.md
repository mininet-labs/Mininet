# The self-hosted forge spine — adapted from the founder-adopted external audit (D-0066)

A founder-commissioned external technical assessment (2026-07-10) found that
this repository's implementation breadth (identity, presence, storage
rewards, confidential value, treasury custody, settlement, finality
verification, social objects, forge) has run ahead of *vertical
integration*: there is no complete path from a developer's change through
review, governed merge, reproducible build, release finality, safe
installation, health check, and rollback. The founder adopted the report's
recommended six-batch re-sequencing (D-0066). This document is that plan,
adapted to what is actually already real in this tree versus genuinely
missing — see each batch's "already have" / "genuinely missing" split.

**Batch 1's exit condition, the bar every piece below is measured against:**
*two developers can exchange a signed proposed commit, review the exact
commit, and reach a governed canonical branch head without GitHub being the
authority.*

## Correction to the audit

The report describes a "proposal/review/merge object model" as new work for
its first recommended PR. `crates/mini-forge/src/governance.rs` already
implements this, predating this session:

- `propose()` — a PR object binding an exact `head` commit and the `base`
  chain position it was built against.
- `approve()` — a verdict **bound to the exact head commit reviewed**
  (invalidated by any later commit swap) — exactly the property the report
  asks for.
- `merge()` / `amend()` — chain entries recording a merge or a
  self-amending policy change.
- `resolve_project()` — deterministically walks the chain and counts quorum
  in distinct verified identity roots (author excluded), with fork
  detection.

Real, tested, already-shipped code. What the report correctly identifies as
missing *around* it, addressed in Batch 1 below: review objects carry only
an approve/reject bit, not free-text findings or CI/test attestations bound
to the reviewed commit; no AI-assistance/human-owner metadata field exists;
and — the actual gap — there is no way for a human to drive any of this
without hand-writing Rust against the library API.

## Batch 1 — developer spine

**Already have:** the propose/approve/merge/amend/resolve_project object
model and quorum logic (`mini-forge::governance`); signed content-addressed
files/trees/commits/branches (`mini-forge` core); a real filesystem-backed
object store (`mini_store::FsBackend`); KEL export/import
(`Kel::to_bytes`/`from_bytes`) for building a local trust directory; secure
key-seed export (`SigningKey::to_seed_bytes`) for on-device persistence.

**Genuinely missing, this batch:**
- `mini-cli` — a real command-line tool wrapping the above so a human can
  actually use it (repo init/commit/checkout/status/branch, PR
  propose/approve/merge, identity init/show, KEL export/trust). Ships this
  batch.
- Review findings + AI-assistance/human-owner metadata on review objects.
  Ships this batch (extends the existing `approve()` payload format, does
  not replace the model).
- `mini-devd` (local daemon, socket IPC, event subscriptions) — deferred
  past this batch. The CLI can operate directly against a local `FsBackend`
  without a daemon; a daemon becomes necessary for background sync and live
  event subscriptions, neither of which Batch 1's exit condition requires.
  Tracked as a fast-follow once the CLI's command surface has proven out in
  practice.
- Git SHA-256 import/export bridge — deferred past this batch (already
  named as pending in `mini-forge`'s own docs before this audit; real work,
  not re-scoped here).
- Machine-readable `STATUS.md`/roadmap generation — deferred; the manual
  three-document reconciliation problem the audit names is real, but lower
  urgency than giving developers a working tool.
- Live network peer exchange for the CLI (`mini sync`) — deferred as a
  near-zero-effort fast-follow: `mini_bearer::TcpBearer` +
  `mini_sync::sync_bidirectional` already proved this composition live
  (D-0062, `mini-bootstrap`'s demo). Batch 1's own exit-condition
  demonstration instead uses a shared `FsBackend` directory (content-
  addressed signed objects are safe to share via any medium — a synced
  folder, a USB stick, later real `mini-sync` — the transport is
  interchangeable and out of scope for what Batch 1 is actually proving,
  which is the governance loop itself).

## Batch 2 — in-house scripting and builds

Split into two parts once the actual engineering cost of each became clear
(the same "adapt the plan to what's real" discipline this whole document
exists to apply):

### Batch 2a — build provenance (shipped, D-0068)

`mini-provenance`: SLSA/in-toto-style build provenance as real, tested,
signed objects — `record_provenance()`/`list_provenance()` capture a
builder's environment digest, commands-run digest, output digests,
reproducibility group, and whether networking was enabled, tied to a
subject (a commit or artifact `ObjectId`). `independent_agreement()`
generalizes `mini-forge::verify_release_artifact_only`'s existing
"N distinct verified identity roots agree" pattern to the *build* stage,
before a release is even proposed — directly addressing the audit's named
critique that the current CI's same-runner double-build check must never
be described as independent reproducibility. No new dependency: this is
signing and counting, the same primitives every other crate in this tree
already uses.

**Honest limit, stated once here rather than re-derived per caller:** code
can verify *distinct identity roots* agree on a digest. It cannot verify
*administratively independent infrastructure* — three containers on one
host, signed by three different keys the same person controls, look
identical to this crate. That gap is a policy/process fact about who
controls which signing key, not a code gap; `mini-forge`'s own release-
attestation docs already carry the same caveat, and it applies here
unchanged.

### Batch 2b — sandboxed execution (shipped, D-0069: Wasmtime, isolated)

**Decision (D-0069):** adopt Wasmtime as the reference executor for
untrusted pipeline components. Enforcement is mandatory from the start —
no metadata-only capability phase, no Mininet-specific sandbox as the
first implementation, and Batch 2 is not considered complete with
capability metadata alone.

**Non-negotiable structural constraint: Wasmtime touches exactly one
crate.**

```
mini-pipeline
    Pure manifest, policy, capability, and execution-plan types.
    No Wasmtime dependency.
mini-pipeline-protocol
    Content-addressed request/result messages (parent <-> runner IPC).
    No Wasmtime dependency.
mini-build-runner-wasmtime
    A separate executable. The ONLY crate linking wasmtime/wasmtime-wasi.
```

`mini-cli`, `mini-forge`, `mini-chain`, identity, the eventual update
verifier, and an ordinary end-user node never link Wasmtime — only a
machine that volunteers as a build worker runs the runner binary. This
confines the dependency to one replaceable component instead of the
constitutional core, the same reasoning that keeps `mini-value`/
`mini-treasury`'s heavier crypto dependencies out of `mini-chain`.

**Why the two rejected alternatives are worse, for the record:**
- *Metadata-only capabilities* (a manifest claiming `network:none` while
  actually launching an unrestricted process) is rejected outright —
  describes desired behavior without enforcing it, and must never produce
  a trusted build attestation. If a manifest-only parser exists as a
  stepping stone, it must self-report `execution_security = "unenforced"`
  and `trusted_provenance_eligible = false`.
- *A home-grown OS sandbox* (Linux namespaces/seccomp/Landlock, macOS
  Sandbox, Windows AppContainer/job objects) is not the first
  implementation — it would become its own multi-platform security
  project. OS isolation may later wrap the Wasmtime runner as defense in
  depth (especially for shared public builders), never replace Wasmtime's
  portable, import-based guest capability boundary.

**What Batch 2b actually implements:**

1. **Deny everything by default.** No filesystem, network, environment
   variables, secrets, wall clock, or inherited stdin unless explicitly
   granted; bounded stdout/stderr, memory, and execution time. Filesystem
   access only through explicit preopened directories (Wasmtime's WASI
   implementation grants no filesystem access by default). Capability
   vocabulary: `workspace:read`, `scratch:write`, `artifacts:write`,
   `clock:monotonic`, `random:deterministic`, `network:host("crates.io")`,
   `secret:read("release-token")`. An undeclared capability means the
   interface is *absent* from the linker, never present-but-disabled.
2. **Run out of process.** `mini-build-runner-wasmtime` is a child process
   of the forge/pipeline coordinator, communicating over length-delimited,
   size-bounded messages (stdin/stdout or a local socket, via
   `mini-pipeline-protocol`) — a second boundary against runtime crashes,
   native memory blowups, compiler failures, and future Wasmtime CVEs,
   and the seam that makes deadline enforcement and clean cancellation
   possible.
3. **Explicit resource limits.** Fuel or epoch interruption for CPU;
   parent-enforced wall-clock timeout as an *emergency stop*, not the
   reproducibility mechanism; a `ResourceLimiter` for max linear memory,
   tables, and instances; capped output/stdout/stderr bytes and open file
   count. For reproducibility: deterministic fuel limits with fuel-consumed
   recorded, a deterministic random seed derived from the execution-plan
   digest where randomness is permitted at all, no wall-clock access
   inside the guest, normalized paths/environment, every granted
   capability recorded.
4. **Cranelift, compiling untrusted Wasm inside the isolated runner** (not
   a separate trusted precompiler signing native artifacts — Wasmtime's
   own docs warn that deserializing arbitrary precompiled modules assumes
   trusted input, which pipeline components submitted by contributors are
   not). A split precompiler/execution architecture is future work, not
   this batch.
5. **Trim Wasmtime's feature set.** `default-features = false`, enabling
   only what the chosen WASI Preview 2 path needs (illustratively: `std`,
   `runtime`, `cranelift`, `component-model`, `async` for `wasmtime`;
   `p2` for `wasmtime-wasi` — the implementation PR determines and
   justifies the actual minimum). The true dependency increase (WASI
   Preview 2 pulls in Tokio and capability-oriented filesystem libraries
   via `wasmtime-wasi`) must be measured and stated explicitly, not
   estimated.
6. **Govern the dependency like a trust-boundary dependency, because it is
   one:** pin an exact patch version, commit `Cargo.lock`, run
   `cargo deny`, establish a `cargo vet` policy, generate an SBOM for the
   runner binary, record the Wasmtime version and runtime-config digest in
   every provenance record, watch Wasmtime security advisories, test
   upgrades through a dedicated compatibility suite, vendor build-time
   dependencies for offline reproducibility, and never auto-merge updates
   to this crate.

**Critical, explicitly-stated scope limitation:** Wasmtime does not
sandbox arbitrary native build tools. `cargo build`, `npm install`,
`cmake`, `bash build.sh` are host processes, not Wasm guest instructions —
nothing about adopting Wasmtime makes them safe. Two pipeline step
classes, accordingly:

```toml
[[step]]
kind = "wasm-component"
component = "object:..."
capabilities = ["workspace:read", "artifacts:write"]

[[step]]
kind = "native-tool"
toolchain = "object:rust-toolchain-digest"
arguments = ["build", "--locked", "--release"]
```

For Batch 2b: `wasm-component` steps are trusted-attestation eligible;
unrestricted shell steps are never trusted-attestation eligible;
`native-tool` stays unavailable or explicitly experimental until a
separate OS-isolated, content-addressed tool runner (hermetic container/
microVM/platform sandbox, pinned toolchain image, no shell interpretation,
structured arguments, read-only source, network off by default, cgroup/
job limits, full provenance) is designed and decided the same explicit
way D-0069 was. Wasmtime alone is never described as having made the
whole Rust build hermetic.

**Batch 2b exit criteria** (all twelve must be demonstrated, not merely
argued, before this batch is called done — status below reflects
`mini-build-runner-wasmtime/tests/adversarial.rs`, which drives the real
compiled runner binary as a subprocess against real, freshly-compiled
WASI Preview 2 components):

1. A signed Wasm component executes and produces a content-addressed
   output. **Demonstrated.**
2. No filesystem or network access exists by default. **Demonstrated.**
3. Read-only workspace access and isolated output access are
   independently enforced. **Demonstrated.**
4. An undeclared network import fails. **Demonstrated.**
5. `..`, absolute-path, and symlink escape attempts fail. **Demonstrated**
   (symlink escape specifically not separately fixtured — cap-std's
   directory-capability enforcement, which the `..`/absolute-path tests
   exercise, is the same mechanism that would refuse a symlink escape).
6. An infinite loop is terminated. **Demonstrated** (fuel exhaustion).
7. A memory-growth bomb is rejected. **Demonstrated** (`ResourceLimiter`;
   note a refused grant doesn't always trap cleanly — the guest's own
   allocator may abort instead, so the runner reclassifies from observed
   post-run limiter state rather than trusting the guest's crash message).
8. Excessive stdout and artifact output are bounded. **Demonstrated** for
   stdout (`MemoryOutputPipe`, same reclassify-from-observed-state
   handling as criterion 7); `max_output_bytes` (total artifact bytes) is
   implemented as a post-hoc check but has no dedicated adversarial test
   yet.
9. Runner termination does not corrupt the forge or provenance store.
   **Partially demonstrated** — a resource-exceeded run leaves no state a
   later, independent run can observe, which is what this crate's own
   process-per-step design guarantees; not tested against real
   `mini-forge`/`mini-provenance` storage, since this crate has no
   dependency on either. Full end-to-end proof is a coordinator-level
   integration test, not yet written.
10. Provenance records the full field list. **Demonstrated.**
11. Unrestricted shell execution cannot produce a trusted build
    attestation. **Demonstrated** — a `mini-pipeline` structural
    guarantee (`StepKind::NativeTool::trusted_provenance_eligible()` is
    unconditionally `false`), re-asserted from this crate's own test
    suite too.
12. Two independent runners execute the same deterministic component and
    agree on its output digest. **Demonstrated** for two independent
    invocations of the one reference implementation; a second,
    independently-authored executor to compare against does not exist.

**Sequencing** (Batch 2b is not an open-ended effort to support every
language and build system — one narrow, enforced Wasm-component path,
proven, then move on):

```
Batch 2a provenance (shipped, D-0068)
        v
Batch 2b.1: pure pipeline manifest and policy (mini-pipeline, no Wasmtime) -- shipped
        v
Batch 2b.2: isolated Wasmtime runner (mini-build-runner-wasmtime) -- shipped
        v
Batch 2b.3: adversarial capability/resource tests (the 12 criteria above) -- shipped
        v
Batch 3: TUF-style release verification -- shipped, D-0070
        v
Batch 4: real installation (mini-installer) -- shipped, D-0071
        v
Batch 5: Mininet as the primary forge -- next
```

## Batch 3 — release verification (shipped)

Adapt TUF's role separation (root / targets / snapshot / timestamp,
delegated roles) rather than inventing a new trust model — Mininet's
existing release-registry design (timelock + independent attestation counts
in `mini-forge::release`/`verify_governed_release`) already covers part of
this; missing were metadata expiry, rollback protection, a release
transparency log, and requiring builder quorum from *administratively
independent* builders (not three containers on one host). Adapted to
Mininet's identity-root/governance model rather than TUF's PKI role
separation, per Directive 14: reuse the existing object/index machinery
instead of inventing a parallel signed-metadata format. Four pieces:

1. **Rollback protection** (`mini_forge::release::{Version, check_no_rollback}`).
   A comparable dotted-numeric `Version` type (strict parsing — no empty/
   leading-zero/non-numeric components, capped at 8 components) with
   component-wise, zero-padding comparison so `"1.2" > "1.1.9"` compares
   correctly. `AdoptionState` compares the candidate's version against
   whatever it is currently running before touching any other gate, and
   refuses a non-upgrade (including an exact-version replay) as
   `ForgeError::RollbackRejected` / `AdoptionDecision::Rejected`.
2. **Release transparency log** (`mini_forge::release::{list_releases,
   detect_equivocation}`). No separate signed snapshot metadata format —
   the object store's own append-only, content-addressed nature already
   *is* the transparency log; these functions are the missing query
   surface over it. `detect_equivocation` flags any two `RELEASE` objects
   for the same project/branch that claim the same version but disagree on
   the artifact digest — the Certificate-Transparency-style property that
   no single observer's view is trusted as complete, but two observers
   comparing logs will disagree about what "version 1.2.3" was.
3. **Freshness / metadata-expiry** (`mini_update::FreshnessPolicy`). Not a
   separately signed "timestamp role" object, since Mininet has no
   analogous PKI role to sign one — instead an explicit, caller-supplied
   `last_synced_ms` compared against the policy's `now_ms`, refusing an
   adoption decision as `AdoptionDecision::ViewTooStale` /
   `AdoptError::ViewTooStale` if the device's own view of the network is
   too stale to trust, checked before any governance gate runs. Adapted
   rationale, stated once here: the thing being bounded is "how recently
   did *this device* last sync," not "how old is a repository's signed
   claim of currency." A `FRESHNESS_MAX_ALLOWED_STALENESS_MS` ceiling (30
   days, provisional) stops a caller from weakening the check into
   meaninglessness, the downward-only mirror of
   `mini_forge::ADOPTION_MIN_TIMELOCK_MS`'s upward-only floor.
4. **Independent build-provenance quorum** (`mini_update::ProvenancePolicy`
   + `AdoptionState::evaluate_with_provenance`). An additional, optional
   gate wiring `mini_provenance::independent_agreement` over the release's
   source commit, alongside — never instead of — `mini-forge`'s existing
   release-attestation quorum: two independently-computed distinct-
   identity-root counts as defense in depth rather than trusting one
   mechanism alone. Same honest limit repeated verbatim from
   `mini-provenance`'s own docs: this counts **distinct identity roots**,
   not *administratively independent infrastructure* — three containers on
   one host under keys one person controls are indistinguishable from
   three real builders to any code in this tree.

All four are additive gates layered in front of
`mini_forge::verify_governed_release` (itself unmodified) rather than
folded inside it, since that function is deliberately stateless and these
gates need either `mini-update`'s own device-local state (`running`,
`last_synced_ms`) or a second crate (`mini-provenance`) `mini-forge` must
not depend on. A caller that never constructs a `FreshnessPolicy`/
`ProvenancePolicy` or calls `evaluate_with_provenance` sees no behavior
change from before Batch 3.

## Batch 4 — real installation (shipped)

`mini-installer`, separated from `mini-update` (which stays policy/intent
only, per the existing no-forced-update/no-kill-path freeze). This was the
audit's most safety-critical, most honestly-named gap —
`mini-update::AdoptionState::adopt` verifies a release and records a
decision, but nothing in that crate executes, fetches, or installs
anything, deliberately. `mini-installer` is the separate layer that
actually does, built as a type-state pipeline over the exact named state
machine: `Discovered → Verified → Downloading → Staged → PreflightPassed →
AwaitingOwnerApproval → Activating → HealthChecking → Active` or
`RolledBack`.

- `Installer::stage` fetches real bytes from the store
  (`mini_media::assemble`, genuine chunk reassembly, not a stub) and writes
  them to a real local staging directory, re-verifying the digest
  independently of `mini-media`'s own internal check.
- `Installer::preflight` re-reads and re-verifies the staged bytes on disk
  immediately before activation, catching staging-directory corruption or
  tampering in between.
- `Installer::activate` atomically flips a `current` symlink to the staged
  release (temp-symlink + `rename`, atomic on one filesystem) — but only
  given an explicit, caller-constructed `OwnerApproval` naming that exact
  release id (the typed-domain rule: no generic "approve" call). Records
  the previous target as a real file, so rollback survives a process
  restart, not just an in-memory value.
- `Installer::health_check` runs a caller-supplied predicate (this crate
  cannot know what "healthy" means for arbitrary software — the same
  caller-supplied-policy pattern as `mini_update::FreshnessPolicy`) and
  automatically rolls back to whatever was running before on failure,
  clearing the `current` pointer entirely rather than leaving it on
  known-unhealthy software if there was nothing to fall back to.
- `Installer::rollback` is directly callable too, consumes the recorded
  "previous" pointer on success so repeated calls fail cleanly instead of
  toggling between two releases.

**Honest limits, stated in the crate's own docs:** Unix-only
(`std::os::unix::fs::symlink`); no process supervision (staging + a
pointer flip only — starting/stopping/restarting a process is the
caller's job); no real package-manager/OS integration (activation means a
symlink under an installer-owned directory changes target, not that any
running system has been touched). 10 adversarial/integration tests against
real files on real disk.

**Batch 6's stated exit condition** — "a deliberately broken release
detected, auto-recovered, with a verifiable event history in a test
environment" — is demonstrated by
`a_failed_health_check_rolls_back_to_the_previous_release` and
`a_failed_health_check_on_the_first_ever_activation_leaves_nothing_active`:
a release whose caller-supplied health check deliberately fails is
detected (`HealthCheckOutcome::RolledBack`/`FailedWithNoPriorRelease`),
auto-recovered (the `current` pointer is atomically restored, verified via
`Installer::current()`), with the `ActivationRecord`/`HealthCheckOutcome`
values themselves standing in for the "verifiable event history" in this
test environment — a real local disk, not a simulated one, though not yet
a running distributed system with real network partitions or concurrent
installers racing each other.

## Batch 5 — Mininet as the primary forge

P2P proposal/review synchronization (reusing `mini-sync`/`mini-net`, no new
wire protocol needed per the same composition insight D-0062 already
proved), local object indexing at scale, distributed build workers, native
release retrieval, GitHub import/export mirror automation so the roles
eventually invert (GitHub becomes the read-only mirror). Not started.

## Batch 6 — resume horizontal breadth

Only after Batch 4's exit condition (a deliberately broken release
detected, auto-recovered, with a verifiable event history in a test
environment) does substantial effort return to networked consensus, real
BLE/UWB hardware, personhood research, economic mechanisms, anonymous
value depth, and proof-of-replication depth (#36-#45, #22, #18/#19/#20/#21,
#46-#51, further `mini-value`/`mini-porep` work).

## Cryptography and governance notes from the audit, applied opportunistically

Several of the audit's cryptography recommendations (Noise-framework
handshakes instead of an ever-expanding custom one, hybrid post-quantum
signatures, per-key-role separation, multi-signature release attestation
over single-threshold FROST) are real improvements but are not blocking for
Batch 1 and are not scheduled as their own batch — they get applied where
the relevant subsystem is next touched (e.g. the channel handshake in
`mini-bearer` when that crate is next substantially revisited), rather than
as a standalone rewrite pass, per Directive 14 (don't touch working code
without a concrete reason).

The audit's policy-class table (stricter approval floors for
identity/consensus/forge governance, installer/update, and cryptographic
protocol code than for ordinary changes) is adopted as guidance for
`CONTRIBUTING.md`'s review checklist, tracked as part of Batch 1's metadata
work (AI-assistance/human-owner fields make the distinction visible on the
object itself).
