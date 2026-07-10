# Pre-coding issue matrix

Classifies open issues by what can actually close them, so effort isn't
spent writing code before the governing assumptions are settled. This is
a triage tool sitting alongside `docs/STATUS.md` (what's built) and #99
(external-gate tracking) — not a replacement for either.

## Closure categories

- **Code-closeable** — implementation and tests can close it.
- **Spec-closeable** — a design document and acceptance criteria close it
  (for now — implementation is separate, later work).
- **Simulation-closeable** — needs adversarial/economic simulation before
  code, or before treating a parameter as settled.
- **Hardware-gated** — needs real devices or lab testing; no sandbox
  substitute.
- **External-gated** — needs legal, academic, security, mechanism-design,
  or domain-expert review this project cannot self-certify.
- **Founder-decision-gated** — needs a value or sequencing judgment call,
  not more engineering.
- **Deferred** — explicitly not MVP, preserved for later.

## Priority coordination issues — current state (2026-07-10)

| Issue | Area | Status | Pre-coding artifact |
|---|---|---|---|
| #102 | Self-hosted forge spine | Batches 1-4 **shipped** (D-0067/68/69/70/71); Batch 5 (P2P forge sync) not started; Batch 6 (horizontal roadmap breadth) partially started via D-0073/74/75 | `docs/design/self-hosted-forge-spine.md` |
| #91 | Failure Book | **Shipped and live** — `docs/FAILURE_BOOK.md` exists with real entries (not a template to seed); ongoing maintenance duty (see `ISSUE_CLOSURE_RULES.md`) | `docs/FAILURE_BOOK.md` |
| #99 | External legitimacy gates | **Live index**, kept current as gates change | #99 itself, `docs/gates/README.md` |
| #92 | Roadmap control plane | **Live hub**, kept current as issues close | #92 itself, this matrix, `PHASE_DEPENDENCY_GRAPH.md` |

## High-risk issue table — current state

| Issue | What | Closure class | Spec/simulation prep done | Outside need |
|---|---|---|---|---|
| #47 | Treasury economic model | Simulation + external | **Done** — D-0073 design, `tools/sim/tokenomics_sim.py` harness run once | mechanism-design reviewer to validate calibration |
| #50 | Long-term inflation/whale modeling | Simulation + external | **Done** — D-0074 design, same harness | same reviewer as #47 |
| #21 | Human uniqueness proof research | Spec + academic | **Done** — D-0075 design (`docs/design/human-continuity-proof.md`), predicate/signal/threat detail filled in | academic cryptography review; research-track funding |
| #97 | BLE/UWB presence-ranging | Hardware-gated | **Done** — detailed T1-T6 test matrix, log schema (`docs/gates/hardware-test-protocol.md`) | real Android/iPhone hardware + FFI work |
| #98 | Local Wi-Fi bearer | Hardware-gated | **Done** — W1-W7 test matrix (`docs/gates/wifi-bearer-test-protocol.md`) | phones/routers |
| #28 | Extreme-environment design | Spec + domain expert | **Done** — network-mode/message-class/finality reasoning (`docs/gates/dtn-design-constraints.md`) | satellite/DTN domain expert to confirm regime scope |
| #102 | Self-hosted forge spine | Multi-batch engineering | Batches 1-4 **done**; Batch 5 vs. resuming Batch 6 is a **founder sequencing call**, not spec-gated | none — this is the founder-decision-gated item on this list |

The pattern worth naming: every P1/P2 item in the External Legitimacy
Gates system (#99) now has its internal spec/simulation/protocol prep
done. What remains on all of them is genuinely outside engineering's
reach (hardware, funding, an external reviewer's judgment) — this matrix
existing doesn't change that, it just makes the boundary visible instead
of implicit.

## Phase-level pre-coding questions (forward-looking, still open)

These aren't resolved yet and apply to future work in each phase,
regardless of what's already shipped:

### Phase 0-2: founder intent, identity, personhood
No KYC gate in protocol core; no biometric dependency; no single identity
provider; no admin recovery key; no personhood claim stronger than
evidence supports (D-0075 already enforces this for signal (b) — extend
the same discipline to anything phases 0-2 add later).

### Phase 3: transport and offline operation
One bearer interface first (`mini_bearer::Bearer` already is this);
attach TCP/BLE/UWB/Wi-Fi-LAN/DTN modes behind it, not as parallel
special cases.

### Phase 4: storage
Before incentives are coded, decide what's rewarded separately:
possession, availability, repair participation, bandwidth served,
long-term reliability — `mini-reward`/`mini-porep`/`mini-erasure` already
separate several of these; keep that discipline for anything new.

### Phase 5: consensus
A partition may preserve local usefulness, but must never create fake
global finality — the same rule `docs/gates/dtn-design-constraints.md`
states for disaster mode applies here too.

### Phase 6: economics
Parameters may be observable and adjustable, never *instantly*
adjustable — D-0074's timelocked governance-may/may-not list is the
concrete instance of this rule.

### Phase 7: governance
AI may summarize, audit, simulate, and recommend. AI is evidence, never
authority — matches `mini-forge`'s existing AI-assistance-declaration
model (informational, never quorum-counted, D-0067).

### Phase 8: object model
Don't make every feature a special object; a small object kernel plus
typed extensions (the existing `mini-objects`/`ObjectType::Custom`
pattern) stays the rule.

### Phase 9: forge and release
Self-hosted merge legitimacy, build provenance, binary transparency, and
safe installation are one spine, not separate optional features — this
is exactly what Batches 1-4 already built end to end.

### Phase 10: security and abuse
Abuse handling cannot become hidden admin control — transparent process,
limited powers, appeal paths, matching the voice/value wall's existing
"no admin key" discipline (P3).

### Phase 11: AI
AI outputs stay advisory and auditable; human-rooted governance remains
the authority layer — same rule as Phase 7, restated for anything
AI-assisted-search-shaped (#80) adds later.

### Phase 12: survivability
Long-horizon work produces constraints first, not speculative code —
`docs/design/inflation-and-whale-resistance.md`'s 200-year simulation
requirement (not yet run) is the concrete instance of this rule for
economics specifically.
