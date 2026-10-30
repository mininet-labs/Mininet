# Mobile reference system: implementation roadmap

**Status:** proposed, design-only. All items below are unstarted work packages
in this proposal unless they explicitly identify existing code to reuse.
This roadmap extends the [mobile OS proposal](MOBILE_OS_PROPOSAL.md); it does not
close or reprioritize accepted rows in [ROADMAP_TO_RELEASE](../ROADMAP_TO_RELEASE.md).

## 1. Stages and exit gates

Estimates are engineering planning ranges, not release dates. They assume two
dedicated client/core engineers for P0–P2, access to physical devices, and later
additional Android platform/release expertise. Review, research, procurement and
security audit lead times are excluded. Re-estimate after P0; do not add these
ranges into a promised launch date or infer that existing contributors are assigned.

| Stage | Deliverables | Dependencies and exit gate | Indicative effort |
|---|---|---|---|
| P0: current-state proof and platform feasibility | Exact-revision mobile inventory; two-phone baseline; device shortlist; maintenance/cost model; stock-vs-system experiment plan | Scope acceptance; Android build evidence; named test owners; select one candidate only after its unlock/relock and firmware terms are verified | 2–4 weeks |
| P1: dependable ordinary Android client | Recovery/delegation, QR/BLE UI, lifecycle adapter, durable messaging and basic social flow, phone↔desktop compatibility | P0; real-device evidence including process death, permission refusal, offline recovery and three-node relay; no production claims beyond existing gates | 6–12 weeks |
| P2: useful remote network and contribution controls | Relay/rendezvous/reconnect, shared outbox, bounded content services, optional seeding, privacy dashboard, SDK draft | P1; CGNAT and provider-removal tests; resource-budget measurements; no exclusive OEM server | 8–16 weeks; shared networking gaps may take longer |
| P3: one-device reference image | Pinned image manifest, optional setup, minimal system adapter, AVB/relocking evidence, owner-approved OTA, recovery instructions and update custody design | P0 platform spike may run early; broad use waits for P1/P2. Exit requires demonstrated OS benefit, firmware support and passed update/security drills | 8–16 weeks with platform/release specialist capacity |
| P4: supported limited pilot | Exact tested image, support process, private/local diagnostics, compatibility report, independent security review and repeated patch releases | P3; all applicable release gates; supported daily phone functions; pre-go-live participation rules respected | 8–12 weeks minimum observation; audit completion not schedulable here |
| P5: open manufacturer ecosystem | Published capability/test kit, second independent vendor build, migration/interoperability evidence and commercial support terms | P4 sustainable maintenance; manufacturer commitments and verified evidence; no badge-dependent admission | No date: partner and capacity dependent |

P0 → P1 → P2 provides value even if P3 is stopped. The P3 feasibility spike can
run alongside client work, but cannot consume the work needed to make the network
usable. Do not divert external-audit or forge-critical resources without an explicit
resource decision. The [Node Appliance](../design/mininet-node-appliance.md) remains
the home/community infrastructure complement to battery-limited phones.

## 2. Bounded implementation PRs

These are work-package IDs, **not allocated GitHub issue numbers**. Reuse or link
existing issues #196–#205, #22, #97 and #98 where their actual scope matches;
check their current status before creating or assigning follow-ups. Each PR should
ship one working slice plus tests and truthful status updates.

| Work package | Proposed changes / existing paths | Depends on | Definition of done |
|---|---|---|---|
| M00 inventory and device harness | `app/android`, Android CI, `docs/mobile`; reconcile stale mobile claims with exact build/test outputs | None | Build revision/toolchain captured; two-device test instructions repeatable; emulator and hardware evidence separate |
| M01 custody and recovery | `mini-ffi`, `did-mini`, Android custody adapter; reuse #197–#199 | M00 | Restart, new-phone enrollment, encrypted backup restore, key invalidation and revocation tested; unsupported hardware signing labelled honestly |
| M02 lifecycle persistence | `mini-ffi::lifecycle`, Kotlin lifecycle glue, manifest/service types appropriate to actual work; reuse #202 | M00/M01 | Durable checkpoint/outbox survives process death; denied notification and background limits handled; no invisible unbounded worker |
| M03 real nearby mesh | Existing BLE adapters and `MeshHandle`; UI controls and permissions; reuse #200/#201/#22 | M01/M02 | QR pair; three physical phones relay without direct endpoint link; deny/revoke permissions; bounded peers, bytes, hops and dedup state |
| M04 local Wi-Fi handoff | Bearer negotiation and platform adapter; coordinate #98 | M03 | Peer binding survives BLE→Wi-Fi handoff; no plaintext downgrade; fallback when Wi-Fi Direct/hotspot unavailable; transfer resume |
| M05 mobile social/outbox | Extend existing `mini-social`/`mini-sync`/FFI; native screens | M01/M02 | Two phones and desktop exchange actual posts/messages; duplicate suppression, cancellation and queued/delivered state correct |
| M06 shared internet connectivity | Shared transport/bearer discovery, relay/rendezvous and reconnect work | M05; transport security scope review | Two phones on separate CGNAT networks connect through replaceable relays; IP changes recover; bad referrals fail safely |
| M07 bounded contribution and content | Shared storage/retrieval plus Android quotas, Library/search UI and resource dashboard | M04/M05/M06 | Stop and quota enforcement; encrypted/device-only content excluded from seeding; useful-byte/energy measurements; unpaid tickets never shown as spendable |
| M08 app capability API | Extend typed FFI/app adapter with grants and revocation; sample app | M05 | Version mismatch and malicious caller tests; deny-by-default access; no secret material across API |
| M09 reference image spike | Proposed new `platform/android/` manifests/patch inventory; isolate from protocol workspace | M00 and selected device | Clean image boots; preinstalled unprivileged app works; optional setup; stock-vs-image experiment published with adverse results |
| M10 justified system adapter | Proposed narrow Binder/Settings integration; SELinux policy if necessary | M02/M09 and demonstrated need | Every privilege maps to measured requirement; cross-user/caller abuse tests; owner can stop/disable service |
| M11 APK/image release adapters | Platform-specific manifest verification and staging; compose existing forge/provenance/update semantics | M09/M10; approved key custody design | Independent rebuild evidence; wrong device/key/replay tests; artifact import from peer; no production keys in PR/CI defaults |
| M12 OTA and recovery | Supported Android updater integration and migration journals; lab fault injection | M11 | Interrupted update, full disk, bad payload, failed boot, schema failure and anti-rollback recovery on physical device; exact-version consent |
| M13 pilot evidence package | Device matrix, independent review findings, maintenance rota, support and incident runbooks | M03–M07/M12; applicable external gates | Closed blocking findings; patch drills on multiple consecutive releases; redacted evidence reproducible by another tester |
| M14 manufacturer conformance kit | Turn [device requirements](MOBILE_DEVICE_REQUIREMENTS.md) into runnable tests and versioned reports | M13 | Two independently built products exchange/migrate; vendor disappearance drill; no certification server in client admission |

First coding task after review: M00's exact-revision two-phone baseline and Android
lifecycle/mesh wiring gap report, followed by M02/M03. Do not start by creating an
empty ROM tree or duplicating identity, governance, social or networking crates.

## 3. Roles and capacity

| Role | Work | Capacity condition |
|---|---|---|
| Client/platform engineer | Compose, lifecycle, Android permissions, device tests and later product integration | Dedicated P0–P3; must have real device/toolchain access |
| Core/network engineer | FFI, outbox, bearer integration, reconnect, resource limits and desktop interop | Dedicated P0–P2; coordinate shared network changes with existing maintainers |
| Android platform/release specialist | Device trees, SELinux, Verified Boot, vendor updates, image builds/OTA | Required before supported P3 images; not safely assumed to be spare client-engineer time |
| Security/release reviewer and physical QA | Threat review, update attacks, compatibility and failure injection | Separate review responsibility; independent external audit where required |
| Support/partner owner | Patch rota, support lifetime, hardware supplier evidence and incident communication | Named and resourced before P4/P5, not an informal volunteer promise |

P0 costs must include at least three physical phones (for an actual relay topology),
one additional recovery/spare target where possible, low-end stock-client testing,
two independent network paths, build capacity/storage, independent review and an
ongoing patch/support budget. Record quotations at procurement time; no invented
funding commitment or device purchase is authorized by this plan.

## 4. Measurement contract

The following are proposed pilot targets, not current performance claims. P0 must
record device model, battery health, radio conditions, OS revision, workload and
measurement method, then review the targets before the trials. Publish failures
and percentiles, not only a selected successful demo.

| Metric | Initial target / experiment |
|---|---|
| First-use usability | At least 90% of a small, authorized usability cohort completes enrollment and a first exchange in 10 minutes without developer intervention; synthetic identities until gates permit otherwise |
| Background delivery | At least 95% of 100 small-message trials reach peer acknowledgement within 60 seconds when a tested route is available; separately report low-power/delayed mode; never claim always-online |
| Recovery correctness | 100% of prescribed interruption cases preserve an acknowledged operation or expose a recoverable explicit failure; no silent identity regeneration |
| Energy | Proposed client baseline ≤3 percentage points of additional full-battery drain over 8 hours idle against matched OS/radio control; report repetitions and variability, not an asserted universal limit |
| OS justification | Same-device comparison shows a material, reproducible reliability, energy or owner-control benefit that ordinary supported APIs cannot provide; agree required margin before measurements |
| Network independence | Remove all default Mininet/OEM endpoints; nearby operation and verified offline artifact import still pass; remote operation passes with a different compatible relay |
| Resource cost | Record RAM p50/p95, background wakeups, storage growth, useful/total bytes, radio time and thermal stops; set enforceable caps based on weakest-client results |
| Privacy | No unconsented telemetry in observed flows; publish connection inventory and capture method; a quiet trace alone is not proof against malicious firmware |
| Updates | Every fault case in M12 passes; demonstrate prompt patch intake across at least three successive upstream patch cycles before widening device support |
| Inclusion | Ordinary client passes protocol interoperability without hardware attestation, OEM account, paid service or branded OS |

Retained use and voluntary contribution can inform product demand only after the
applicable participation gates permit a pilot. Use opt-in aggregated reports or
local participant diaries; no always-on analytics, identity census or public
social graph. Report cohort size, attrition and sampling limits. These numbers
cannot prove personhood, anonymity or internet-scale readiness.

## 5. Release, pause and exit conditions

P4 is a supported software milestone, not authority to activate real money,
personhood, treasury custody or consensus. Before recruiting real participants,
apply current pre-go-live rules and required external gates. Until then use
synthetic identities, lab devices and explicitly permitted engineering trials.

Pause expansion if security maintenance lacks an owner, vendor support lapses,
recovery is unreliable, critical findings remain, battery costs exceed accepted
budgets, or the stock-client alternative performs equally well. Preserve working
APK and shared-core improvements if the OS experiment stops. Ship an export and
migration path; do not strand identity behind a discontinued device or cloud.

Before every new model, repeat firmware, boot, cellular/emergency-function,
update/recovery and compatibility qualification. Do not reuse another model's
pass report. A generic image booting is not evidence that a phone is safe or
supported for daily use.

## 6. Review checklist for this planning PR

- Is the optional mobile direction a justified extension of D-0446?
- Do proposed privileges each have a concrete future proof requirement?
- Does each stage deliver useful capability if later stages stop?
- Are hardware rights, personhood and governance kept separate?
- Are maintainers, funding and independent review gates explicit?
- Are all estimates, success targets and device standards labelled proposals?

This PR provides scope and acceptance criteria only. It does not implement or test
these work packages, allocate people, open duplicate issues, set a release date,
authorize signing, change mainnet gates, or claim a manufacturer partnership.
