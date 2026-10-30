# Team leaders and architects welcome pack

> Supplied welcome pack v1.0, dated 21 October 2026. This is a **proposed operating model**, not an enacted governance decision, current team appointment or launch claim. The status counts, PR queue and readiness labels below are the supplied document’s dated snapshot, not a live dashboard. Check [current status](../STATUS.md), [release roadmap](../ROADMAP_TO_RELEASE.md), [governance](../governance/52_PRE_GO_LIVE_GOVERNANCE_PAUSE.md) and [open PRs](https://github.com/mininet-labs/Mininet/pulls) before acting.

[Original Word document](source/Mininet_Team_Leaders_Architects_Welcome_Pack.docx) · [Contribution rules](../../CONTRIBUTING.md) · [Repository home](../../README.md)

Source SHA-256: `a563d2a018bff5cff22e71d77050606d6a865214c9776b9c21615e0d99849d12`. The 87-member main workspace baseline was checked against commit `857b35c9648b37477366572940bf4f0298c39aa4`; proposed branches are separate.

## Build the network, not just the crates.

Mininet engineering ownership, launch gates and team-lead operating charter

21 October 2026   •   Proposed CTP operating model   •   v1.0

MISSION  Create user-sovereign, locally useful, privacy-preserving infrastructure that can continue without any founder, hosted service, software store or privileged operator. Engineering success is measured by independent, reproducible evidence—not the number of merged crates.

| CURRENT VERIFIED BASELINE | NOT A LAUNCH CLAIM |
| --- | --- |
| 87 Rust workspace crates on main; Android source and Debian deployment profile | The physical BLE keystone acceptance path is not evidenced complete. |
| Forge, CLI, sync, release/installer libraries and substantial private-value prototypes exist | Real value remains NO-GO without external audits, legal and personhood gates. |
| Seven open pull requests reviewed in this snapshot; [#334](https://github.com/mininet-labs/Mininet/issues/334) and [#373](https://github.com/mininet-labs/Mininet/issues/373)-375 are open, not on main | Branch-only crates/features are proposed until merged and independently verified. |

This charter is a proposed ownership overlay, not a claim that GitHub teams, approver roles or hires already exist. The source repository remains the technical authority; resolve conflicts by the Constitution, decisions, invariants and current implementation-status register.

## 01  Your first hour

Read in order: founder intent → constitutional limits → latest status → release gates → your interfaces → runnable evidence. The developer map still mentions an old 62-crate count; the authoritative 21 October workspace manifest lists 87.

| ORDER | DOCUMENT / PURPOSE |
| --- | --- |
| 1 | [docs/FOUNDER_DIRECTIVES.md](../../docs/FOUNDER_DIRECTIVES.md) — why the rules exist |
| 2 | [docs/INVARIANTS.md](../../docs/INVARIANTS.md) + [docs/DECISION_LOG.md](../../docs/DECISION_LOG.md) — which boundaries are frozen; decisions outrank comments |
| 3 | [docs/STATUS.md](../../docs/STATUS.md) + [docs/ROADMAP_TO_RELEASE.md](../../docs/ROADMAP_TO_RELEASE.md) — current implementation and R1–R20 release blockers |
| 4 | [docs/governance/52_PRE_GO_LIVE_GOVERNANCE_PAUSE.md](../../docs/governance/52_PRE_GO_LIVE_GOVERNANCE_PAUSE.md) — temporary anonymous-only bootstrap and one-way Go-Live |
| 5 | [docs/BETA_STATUS.md](../../docs/BETA_STATUS.md) + [docs/gates/REAL_VALUE_RELEASE_CHECKLIST.md](../../docs/gates/REAL_VALUE_RELEASE_CHECKLIST.md) — beta versus real-value NO-GO |
| 6 | [CONTRIBUTING.md](../../CONTRIBUTING.md) + [AGENTS.md](../../AGENTS.md) + [.github/CODEOWNERS](../../.github/CODEOWNERS) — contribution/review rules as actually configured |

### The five rules leaders cannot bargain away

- Wealth, grant amount, staking and contribution volume never confer governance or reviewer authority. Current distinct identity roots are not proof of distinct humans.

- No permanent founder/admin override, kill switch, forced update, compulsory central naming service or mandatory identity disclosure may enter the protocol.

- Offline signed IOUs and partition-local observations do not become globally final until canonical consensus accepts them.

- No security-critical prototype is “audited” because its tests pass. External audit, legal review, device proof and research evidence close their own gates.

- Before Go-Live, bootstrap participants remain canonically anonymous; Forge canonicality and Go-Live must be one-way state transitions.

### How to read ownership

Each crate has exactly one primary team. “Shared” means another team is a required interface reviewer—not a second maintainer with competing merge authority. The roster gives technical review routing; it does not override the pre-Go-Live governance pause or current CODEOWNERS configuration.

## 02  Proposed engineering organization

| DOMAIN | OWNER TEAM | CURRENT EXECUTION PRIORITY |
| --- | --- | --- |
| Trust | T01 Identity / Personhood | KEL witness assurance, presence evidence |
| Security | T02 Crypto / Privacy | External audit scopes and transport threat closure |
| Connectivity | T03 Mesh / Sync | Physical BLE and multi-hop tests |
| Finality | T04 Consensus / Ledger | R8 production validity and admission |
| Economics | T05 Value / Treasury | R3 migration decision; external money gates |
| Data | T06 Objects / Storage | Durable retention, verification and repair |
| Discovery | T07 Search / Web | Bounded source-aware discovery |
| Experience | T08 Clients / Mobile | Real Android keystone, recovery, wallet UX |
| Development | T09 Forge | No-GitHub contributor lifecycle |
| Platform | T10 Release / Ops | Real-target install, rollback and reproducibility |

CAPACITY MODEL  These are ten accountability domains, not ten mandatory staffing teams. With limited developers, appoint one lead to multiple adjacent domains, but never remove independent security review or cross-team acceptance evidence.

## 03  Team leader charters

### T01  Identity, Continuity & Personhood

Own identity truth and the limits of what can be inferred about a human from an identity root.

Primary crates: `did-mini`, `mini-witness-service`, `mini-uniqueness`, `mini-presence`, `mini-spacetime`

Next commitment: Wire usable KEL assurance into permitted authority call sites only after [#315](https://github.com/mininet-labs/Mininet/issues/315) policy; complete witness networking and real co-presence evidence ([#21](https://github.com/mininet-labs/Mininet/issues/21), [#97](https://github.com/mininet-labs/Mininet/issues/97)).

Cross-team contract: T02 crypto/security; T03 radio/session bindings; T04 validator admission; T08 account recovery and pairing.

Proof of done: Revoked or stale delegated devices rejected; real two-device presence transcript and explicit assurance level; no claim of unique human without evidence.

### T02  Cryptography, Privacy & Protocol Assurance

Own cryptographic primitives and security-policy interpretation; act as independent review lane for every authority or private-value change.

Primary crates: `mini-crypto`, `mini-pq-anchor`, `mini-attest`, `mini-privacy-policy`, `mini-transport-security`, `mini-commons-policy`, `mini-publication-policy`

Next commitment: Finish authenticated/anonymous transport dual-path, anti-eclipse and onion/mix hardening ([#291](https://github.com/mininet-labs/Mininet/issues/291)); scope and coordinate external cryptographic audits (R12/R13).

Cross-team contract: Every team; especially T01, T03, T04, T05, T09, T10.

Proof of done: Attack-oriented tests, bounded parsers, no hidden identity leak; external reviewer report for value-bearing activation, not self-issued audit clearance.

### T03  Mesh Networking, Sync & Offline Transport

Deliver transport-agnostic communications that survive outages, do not require central discovery, and can prove real multi-hop behavior.

Primary crates: `mini-bearer`, `mini-net`, `mini-mesh`, `mini-relay`, `mini-dtn`, `mini-transport-policy`, `mini-bootstrap`, `mini-sync`

Next commitment: Integrate Kotlin BLE GATT into complete app flow; execute 1/2/3-device hardware matrix ([#335](https://github.com/mininet-labs/Mininet/issues/335)/#97); peer discovery resilience ([#291](https://github.com/mininet-labs/Mininet/issues/291)); [#98](https://github.com/mininet-labs/Mininet/issues/98) Wi-Fi later.

Cross-team contract: T01 pairing/range; T02 auth/privacy; T07 object retrieval; T08 Android; T09 Forge sync.

Proof of done: No-internet two-phone keystone, A-B-C physical relay, reconnect/restart, bounded queues and explicit partition behavior.

### T04  Consensus, Execution & Finality

Own canonical ordering, chain admission, shielded-claim execution validity and the boundary between pending offline credit and final state.

Primary crates: `mini-consensus`, `mini-chain`, `mini-execution`, `mini-settlement`, `mini-ticket`, `mini-shielded-verify`

Next commitment: Complete R8 deployed validator admission, evidence retention/recovery, peer lifecycle, adversarial recovery; migrate transparent claims only under a recorded R3 decision.

Cross-team contract: T01 validator identity; T02 security; T03 mesh; T05 proofs/ledger interface; T09 release evidence.

Proof of done: Independent nodes converge, reject invalid claims, cannot mint finality during partitions, and reconcile slash-free equivocation accountability by protocol rules.

### T05  Value, Economics & Custody

Build private-value mechanisms, transparent model boundaries and treasury custody without allowing wealth to confer governance authority.

Primary crates: `mini-value`, `mini-private-payment`, `mini-reward`, `mini-treasury`, `mini-custody`, `mini-bridge`, `mini-airdrop`, `mini-airdrop-treasury`, `mini-economy`, `mini-econ-sim`, `mini-resource-pricing`, `mini-bounty`, `mini-contribution`, `mini-engagement`

Next commitment: Resolve production payment migration (R3), fee model and traffic-calibrated decoys (R7); commission external tokenomics, value crypto, DKG and legal reviews (R12-R16).

Cross-team contract: T02 audit; T04 settlement/finality; T06 resource evidence; T09 contributor grants; T08 wallet UX.

Proof of done: End-to-end private receive-spend-reconcile with fail-closed negative cases, complete treasury key ceremonies and bounded simulations. No real-money claim pre-audit.

### T06  Objects, Storage & Media Reliability

Own signed object semantics, CRDT/store durability, content repair, provider evidence and resource-bounded media retention.

Primary crates: `mini-objects`, `mini-store`, `mini-crdt`, `mini-storage`, `mini-erasure`, `mini-porep`, `mini-storage-fraud`, `mini-provider`, `mini-replication-policy`, `mini-media`

Next commitment: Multi-node object durability, corrupted/stale data rejection, sustained retention/retrieval/recovery and content seeding; review stacked retention work in PR [#375](https://github.com/mininet-labs/Mininet/issues/375).

Cross-team contract: T03 replication; T05 resource pricing; T07 search indexing; T08 library UX; T09 Forge object storage.

Proof of done: Data recoverable after churn and restart with measured integrity, budget and latency; availability claims based on independent peers.

### T07  Discovery, Search & Web/Data Services

Own opt-in discoverability and web intake/search indexing while preserving provenance, privacy and user choice.

Primary crates: `mini-query`, `mini-lexical-index`, `mini-ranker`, `mini-private-index`, `mini-search-federation`, `mini-search-federation-net`, `mini-intake-types`, `mini-web-types`, `mini-intake`, `mini-intake-social`, `mini-web-extract`, `mini-crawler`, `mini-crawler-fetch`, `mini-extract-protocol`, `mini-extract-host`, `mini-app-protocol`, `mini-app-service`

Next commitment: Integrate bounded peer search, source/provenance explanations and extraction isolation; later harden search federation under real peer/network conditions.

Cross-team contract: T06 signed objects/provider; T03 federation transport; T02 private search; T08 UX.

Proof of done: Queries disclose only owner-authorized data; sources verifiable and fetch/extract bounded; offline/error states explicit.

### T08  Client, Mobile & Human Experience

Make existing core capabilities usable without silently replacing user sovereignty with hosted convenience.

Primary crates: `mini-ffi`, `mini-desktop`, `mini-windows-vault`, `mini-setup`, `mini-windows-setup`, `mini-messaging`, `mini-social`, `mini-safety`, `mini-keystone`

Next commitment: Complete physical Android keystone and multi-hop acceptance ([#335](https://github.com/mininet-labs/Mininet/issues/335)), Windows/desktop onboarding, secure vault/recovery, Forge and wallet surfaces ([#336](https://github.com/mininet-labs/Mininet/issues/336)/#337).

Cross-team contract: T01 identities; T03 BLE/local networking; T04/T05 wallet finality; T06 library; T07 discovery; T09 Forge.

Proof of done: One/two/three-phone journeys, owner-confirmed actions, understandable recovery and no false claims about payment/finality/personhood.

### T09  Native Forge & Developer Workflow

Make Mininet able to develop, review, merge and coordinate contributions through its own verified object network.

Primary crates: `mini-forge`, `mini-cli`, `mini-provenance`, `mini-pipeline`, `mini-pipeline-protocol`, `mini-build-runner-wasmtime`

Next commitment: Review/land [#371](https://github.com/mininet-labs/Mininet/issues/371) and [#373](https://github.com/mininet-labs/Mininet/issues/373); finish [#336](https://github.com/mininet-labs/Mininet/issues/336) campaign-to-task CLI; prove [#338](https://github.com/mininet-labs/Mininet/issues/338) GitHub-outage full contribution loop; record irreversible Forge canonical transition.

Cross-team contract: T06 object/store; T03 sync; T02 provenance/security; T10 installer/release; T05 separate beta-grant policy.

Proof of done: Independent nodes complete issue -> task -> review -> merge -> build -> release without GitHub; mirrors never confer authority.

### T10  Release, Platform Operations & Verification

Own reproducible packaging, owner-approved updates, rollback, deployment, whole-workspace gates and operational evidence.

Primary crates: `mini-installer`, `mini-update`, `mini-durable`, `mini-selftest`, `mini-value-selftest`

Next commitment: Exercise R19 on real Debian/Windows/Android targets; stabilize pinned CI, two-builder reproducibility, fault-injection rollback and audit evidence ledger; [#303](https://github.com/mininet-labs/Mininet/issues/303) Debian appliance.

Cross-team contract: T09 releases/provenance; T08 mobile and desktop installers; T02 security; all team test contracts.

Proof of done: Independent reproducible artifacts, owner-approved activation, broken-release rollback, no kill path, no false green status.

## 04  Cross-team contracts to make explicit

| BOUNDARY | SOURCE → CONSUMER | MUST BE AGREED BEFORE MERGE |
| --- | --- | --- |
| Identity and transport | T01/T02 → T03/T08/T04 | Channel-binding type, peer assurance/freshness, anonymous-vs-named session semantics, revocation and replay contract |
| Transport and data | T03 → T06/T07/T09 | Bounded payloads, inventory/page continuation, verified ingest, restart/partition failure semantics |
| Private value and ledger | T05 → T04 | Shielded proof verifier and claim evidence, nullifier/key-image finality, no vote-weight dependency |
| Object and Forge | T06 → T09 | Signed object schema, exact-state review head, content digest, append-only provenance; GitHub mirror noncanonical |
| Forge to installations | T09 → T10/T08 | Release attestation, transparency/rollback metadata, owner approval, health-check and recovery callbacks |
| Product to core | T08 → all | Owner-consent UX, persistence, status labels and battery/resource budgets; no UI-created authority |

### Decision rights and escalation

- Team lead: backlog order, engineering design inside existing decisions, operational and test evidence for owned crates.

- Domain architect: API compatibility and cross-crate dependency graph; blocks technically invalid merges at the interface review stage.

- CTP / cross-domain council: scope, release sequencing, incident priority, resource conflicts and unresolved architecture proposals.

- Founder/bootstrap custodian: constitutional/Founder-only choices and the temporary canonical integration policy while Pre-Go-Live; must record policy changes as decisions.

- Independent specialist: signs their own audit, legal, hardware or economic review. The CTP cannot substitute an internal opinion for an external gate.

CURRENT GITHUB EXCEPTION  The live [.github/CODEOWNERS](../../.github/CODEOWNERS) routes everything to @mininet-labs under temporary D-0083 and states the active ruleset requires zero approvals. Do not represent a proposed two-reviewer practice as enforced today. For critical paths, require named independent technical/security sign-off in the delivery record regardless of temporary platform enforcement.

## 05  Release road: what remains

Canonical reference: [docs/ROADMAP_TO_RELEASE.md](../../docs/ROADMAP_TO_RELEASE.md), R1–R20. Status below uses its declared readiness state, not inference from an open/closed issue alone. “Outside” means code alone cannot close the item.

| ITEM / STATUS | ACCOUNTABLE | REQUIRED CLOSURE |
| --- | --- | --- |
| R1  Governance validator consistency DONE | T10 / T09 | PR [#309](https://github.com/mininet-labs/Mininet/issues/309); validator evidence |
| R2  Meaningful CI baseline DONE | T10 | Scheduled full green baseline; advisory triage retained |
| R3  Retire transparent payment graph READY | T05 + T04 | Founder D-decision; five consumer crates private-only |
| R4  Private fee, change and multioutput DONE | T05 | D-0455 shielded path |
| R5  Chain-backed private ledger view DONE | T04 + T05 | D-0457; not sufficient for R8 validity |
| R6  Optional account disclosure DONE | T05 | D-0458 bounded disclosure |
| R7  Decoy distribution calibration BLOCKED | T05 / T02 | Wait for R17 real traffic |
| R8  Production consensus completion ACTIVE | T04 + T02/T03 | Deployed admission, evidence gossip/recovery, weak-peer/partition tests |
| R9  KEL freshness / witnesses ACTIVE | T01 + T09 | Integrate assurance gates once [#315](https://github.com/mininet-labs/Mininet/issues/315) policy decided |
| R10  Physical BLE/local radio OUTSIDE | T03 + T08 | T1–T6 physical-device evidence; [#335](https://github.com/mininet-labs/Mininet/issues/335)/#97 |
| R11  Usable end-user client READY | T08 | Two-physical-device end-to-end acceptance |
| R12  Independent crypto audit OUTSIDE | T02 + T05 | Applied cryptography external sign-off ([#72](https://github.com/mininet-labs/Mininet/issues/72)) |
| R13  FROST DKG audit OUTSIDE | T02 + T05 | Independent DKG/treasury report ([#93](https://github.com/mininet-labs/Mininet/issues/93)) |
| R14  Legal counsel review OUTSIDE | CTP + T05 | External counsel: money/treasury/contribution rails ([#96](https://github.com/mininet-labs/Mininet/issues/96)) |
| R15  Sybil/personhood research OUTSIDE | T01 / research | SPEC-02 evidence: roots are not humans ([#21](https://github.com/mininet-labs/Mininet/issues/21)) |
| R16  Tokenomics validation OUTSIDE | T05 / expert | Independent mechanism-design calibration ([#47](https://github.com/mininet-labs/Mininet/issues/47)/#50) |
| R17  Adversarial live network test BLOCKED | T03 + T04 + T10 | Needs R8 and R11; publish faults and network traces |
| R18  Genesis and activation BLOCKED | Founder + T04 | Needs R8 and R12; explicit D-decision |
| R19  Real-target release/rollback READY | T10 + T09 | Real Debian/Windows targets; induced failure rollback; [#303](https://github.com/mininet-labs/Mininet/issues/303) |
| R20  Truthful public launch disclosure BLOCKED | CTP + all | Last step: reconcile R12–R18 evidence and residual claims |

## 06  Three release lanes: do not conflate them

| LANE | MINIMUM ACCEPTANCE | NO-GO LIMIT |
| --- | --- | --- |
| A · Open Beta / real hardware | One-phone lifecycle, two-phone offline discovery/identity/presence, A-B-C relay with redacted physical evidence; client error/recovery UX; [#335](https://github.com/mininet-labs/Mininet/issues/335). | Simulations or Android CI cannot substitute for physical device results. Beta MINI must be labelled, resettable and non-convertible. |
| B · Forge canonical transition | Native campaign → finding → disposition → task → review → contribution → optional beta grant; cross-node sync without GitHub; one-way transition [#336](https://github.com/mininet-labs/Mininet/issues/336)–[#339](https://github.com/mininet-labs/Mininet/issues/339). | GitHub user/admin/server/Founder key cannot remain authority root; no retroactive pre-Go-Live DID link. |
| C · Real-value / full-network Go-Live | R3/R8/R9/R11/R12–R20 evidence, legal/economics/crypto/DKG/personhood signoffs, real adversarial network and owner-approved releases. | No real production MINI, “one human one vote,” safe privacy or global finality claims until evidence genuinely warrants them. |

### Milestone plan: dependency-led, not date-led

- Gate 0 — Baseline and architecture: freeze crate ownership, assign interface reviewers, reproduce focused CI and sign unresolved founding decisions ([#315](https://github.com/mininet-labs/Mininet/issues/315), R3).

- Gate 1 — Beta hardware and operator UX: T03/T08 complete [#335](https://github.com/mininet-labs/Mininet/issues/335); T10 collects device logs; T01 verifies range/freshness semantics.

- Gate 2 — Contributor Forge flow: T09/T06/T03 complete [#336](https://github.com/mininet-labs/Mininet/issues/336), land reviewed [#371](https://github.com/mininet-labs/Mininet/issues/371)/#373 as appropriate; validate native object/sync against separate stores.

- Gate 3 — Beta economics and canonical cutover: T05/T09 implement distinct test-only issuance policy [#337](https://github.com/mininet-labs/Mininet/issues/337)/#339; prove [#338](https://github.com/mininet-labs/Mininet/issues/338) outage and irreversible handoff.

- Gate 4 — Production candidacy: T04/T02/T05 close core risk and external review gates; T10 runs R19 real-target drills and R17 adversarial campaign; CTP issues R20 truth statement only afterward.

## 07  Live work queue and PR integration

As checked on 21 October 2026: seven open pull requests ([#334](https://github.com/mininet-labs/Mininet/issues/334), [#363](https://github.com/mininet-labs/Mininet/issues/363), [#371](https://github.com/mininet-labs/Mininet/issues/371)–[#375](https://github.com/mininet-labs/Mininet/issues/375)). An open PR is not merged code, even when its description claims targeted tests have passed. Treat stacked branches as dependency graphs, not parallel mergeable features.

| ITEM | LEAD / REVIEW | EXECUTIVE DISPOSITION |
| --- | --- | --- |
| [#334](https://github.com/mininet-labs/Mininet/issues/334) · Open Beta / Forge transition | T09 + T05; T08 | Large feature branch with mini-beta reference ledger (not yet on main). Break out validated seams, keep beta-value wall intact. |
| [#371](https://github.com/mininet-labs/Mininet/issues/371) · Campaign/finding/disposition CLI | T09; T06 + T02 | Inspect object compatibility and two-independent-store flow; connects to [#336](https://github.com/mininet-labs/Mininet/issues/336)/#338. |
| [#373](https://github.com/mininet-labs/Mininet/issues/373) · Native Unix Forge launch | T09; T10 + T02 | Validate migration/resumable service and security proofs; predecessor for [#374](https://github.com/mininet-labs/Mininet/issues/374) and [#375](https://github.com/mininet-labs/Mininet/issues/375). |
| [#374](https://github.com/mininet-labs/Mininet/issues/374) · Resident AI (stacked on [#373](https://github.com/mininet-labs/Mininet/issues/373)) | T08 + T07; T02 | Advisory only; never approval, value movement or hidden tool execution. Feature expansion, not R12 substitute. |
| [#375](https://github.com/mininet-labs/Mininet/issues/375) · Seed/search mesh (stacked on [#374](https://github.com/mininet-labs/Mininet/issues/374)) | T06 + T07; T03/T05/T02 | Draft; retention, payment receipts and owner controls need cross-team verification. Not a launch-blocking substitute for R10. |
| [#372](https://github.com/mininet-labs/Mininet/issues/372) · Stealth view-tag optimization | T05; T02 | Benchmark, wire-binding, recovery and external audit still necessary before wallet promotion. |
| [#363](https://github.com/mininet-labs/Mininet/issues/363) · Durable private relationships | T01 + T08; T02/T03 | Draft: Windows runtime test, privacy and revocation seam before defaulting UI on. |

EXPLICIT DEFERRAL  Full custom Android OS, AI marketplace, interplanetary transport, exhaustive social-platform parity and new token features should not displace real-phone Beta, native Forge proof, production audits or installer safety. They remain legitimate programs with separate entry gates.

## 08  Daily operating contract

### For each technical task, require a one-page ticket

- Why now: R#/issue, user-visible claim, dependency and owner.

- Change scope: exact crate(s), public APIs, schema or wire version, consumer and migration plan.

- Safety: relevant invariant/Founder directive, abuse case, privacy impact, failure mode and roll-back.

- Acceptance: runnable command, negative tests, physically independent evidence when needed; precise limitations.

- Handoff: reviewer(s) from adjacent team, [docs/STATUS.md](../../docs/STATUS.md) + roadmap row update, decision log only if policy actually changes.

### Definition of Done

Do not label a capability complete until: merged to the intended canonical source; same state used for tests; cross-domain caller integrated; restart/offline/failure cases exercised; real hardware or external evidence obtained where required; status and public claim updated in the same PR; no critical unresolved safety or authority violation. “Logic implemented”, “app wired”, “physical acceptance”, and “audited” are four different labels.

### Suggested focused validation ladder

Run the cheapest checks that prove your change first, then the mandatory branch merge gates. Avoid full builds on every speculative iteration, but never skip the release suite on a release candidate.

```sh
cargo fmt --all -- --check
```

```sh
cargo clippy -p <crate> --all-targets --all-features -- -D warnings
```

```sh
cargo test -p <crate> --all-features
```

```sh
cargo clippy --all-targets --all-features --workspace -- -D warnings
```

```sh
cargo test --workspace --all-features
```

ARTIFACT DISCIPLINE  Attach an evidence bundle with tested SHA, exact hardware and toolchain where applicable, redacted traces, security/reliability negative cases, and a link to the status-row update. A passing suite is one evidence class, never the entire claim.

## 09  Leader onboarding — first five working days

| DAY | LEADER ACTION | OUTPUT / REVIEW |
| --- | --- | --- |
| Day 1 | Read governing texts and run current core tests (or record why the environment cannot). | One-page current/partial/outside risk map; no first-week policy inventions. |
| Day 2 | Map every crate/module owned to APIs, active PRs, issue IDs and consuming teams. | Owner acknowledgement plus shared-interface checklist. |
| Day 3 | Choose one nearest launch gate, its current failure, smallest closure slice and test artifact. | A bounded task brief with reviewer from another domain. |
| Day 4 | Reproduce a real end-to-end path with failure injection or physical device if that gate requires one. | Runnable transcript plus limitations/negative results. |
| Day 5 | Publish handoff: what shipped/failed, next blockers, required founder/external decisions. | Roadmap row, [docs/STATUS.md](../../docs/STATUS.md) and a CTP review note aligned. |

### Leader questions for the weekly cross-team council

- What user-level vertical path became demonstrably better on independent nodes/devices this week?

- Which remaining blocker is outside code, and who is booking the required external/hardware decision?

- Which change may accidentally turn money into authority, GitHub into canon, identity roots into “humans,” or local IOUs into finality?

- What claim in the README, UI or release notes is stronger than the actual test evidence?

- What dependency could break another team at a version/schema boundary, and have both sides signed the interface?

## 10  Crate ownership directory · main branch

Exactly 87 current Cargo workspace crates. All entries below are exclusive primary ownership. Other teams must be requested as interface reviewers where listed in their charters. Android source app/android belongs to T08 with T03/BLE and T10/build review. The deploy/ Debian appliance belongs to T10 with T09 release review.

| TEAM | PRIMARY-OWNED CRATES |
| --- | --- |
| T01 · Identity, Continuity & Personhood | did-mini, mini-witness-service, mini-uniqueness, mini-presence, mini-spacetime |
| T02 · Cryptography, Privacy & Protocol Assurance | mini-crypto, mini-pq-anchor, mini-attest, mini-privacy-policy, mini-transport-security, mini-commons-policy, mini-publication-policy |
| T03 · Mesh Networking, Sync & Offline Transport | mini-bearer, mini-net, mini-mesh, mini-relay, mini-dtn, mini-transport-policy, mini-bootstrap, mini-sync |
| T04 · Consensus, Execution & Finality | mini-consensus, mini-chain, mini-execution, mini-settlement, mini-ticket, mini-shielded-verify |
| T05 · Value, Economics & Custody | mini-value, mini-private-payment, mini-reward, mini-treasury, mini-custody, mini-bridge, mini-airdrop, mini-airdrop-treasury, mini-economy, mini-econ-sim, mini-resource-pricing, mini-bounty, mini-contribution, mini-engagement |
| T06 · Objects, Storage & Media Reliability | mini-objects, mini-store, mini-crdt, mini-storage, mini-erasure, mini-porep, mini-storage-fraud, mini-provider, mini-replication-policy, mini-media |
| T07 · Discovery, Search & Web/Data Services | mini-query, mini-lexical-index, mini-ranker, mini-private-index, mini-search-federation, mini-search-federation-net, mini-intake-types, mini-web-types, mini-intake, mini-intake-social, mini-web-extract, mini-crawler, mini-crawler-fetch, mini-extract-protocol, mini-extract-host, mini-app-protocol, mini-app-service |
| T08 · Client, Mobile & Human Experience | mini-ffi, mini-desktop, mini-windows-vault, mini-setup, mini-windows-setup, mini-messaging, mini-social, mini-safety, mini-keystone |
| T09 · Native Forge & Developer Workflow | mini-forge, mini-cli, mini-provenance, mini-pipeline, mini-pipeline-protocol, mini-build-runner-wasmtime |
| T10 · Release, Platform Operations & Verification | mini-installer, mini-update, mini-durable, mini-selftest, mini-value-selftest |

Proposed future or unmerged crates—including mini-beta in PR [#334](https://github.com/mininet-labs/Mininet/issues/334) and mini-resident / seed-network features in stacked PRs—are not added to this main-branch registry. Upon acceptance: mini-beta is led by T05 with T09/T02 review; resident AI is led by T08 with T02/T07 review; retained seed-network modules belong to T06, with T03/T07 shared interfaces.

## Sources, authority and revalidation checklist

The governing material is current main plus issue/PR snapshots checked on 21 October 2026. Before using this pack operationally, owners should re-check the links below for changes; status is not a guarantee of present CI, running deployment or shipped release.

Main and Cargo workspace: https://github.com/mininet-labs/Mininet/blob/main/Cargo.toml

Founder directives: https://github.com/mininet-labs/Mininet/blob/main/docs/FOUNDER_DIRECTIVES.md

Invariants and known limitations: https://github.com/mininet-labs/Mininet/blob/main/docs/INVARIANTS.md

Implementation register: https://github.com/mininet-labs/Mininet/blob/main/docs/STATUS.md

R1–R20 road to release: https://github.com/mininet-labs/Mininet/blob/main/docs/ROADMAP_TO_RELEASE.md

Beta status: https://github.com/mininet-labs/Mininet/blob/main/docs/BETA_STATUS.md

External gate index and checklist: https://github.com/mininet-labs/Mininet/blob/main/docs/gates/README.md

Pre-Go-Live temporary governance: https://github.com/mininet-labs/Mininet/blob/main/docs/governance/52_PRE_GO_LIVE_GOVERNANCE_PAUSE.md

CODEOWNERS (currently bootstrap-guarded): https://github.com/mininet-labs/Mininet/blob/main/.github/CODEOWNERS

Master roadmap issue [#92](https://github.com/mininet-labs/Mininet/issues/92): https://github.com/mininet-labs/Mininet/issues/92

Open Beta issue [#335](https://github.com/mininet-labs/Mininet/issues/335): https://github.com/mininet-labs/Mininet/issues/335

Forge migration [#336](https://github.com/mininet-labs/Mininet/issues/336)/#338: https://github.com/mininet-labs/Mininet/issues/336

Beta test economy [#337](https://github.com/mininet-labs/Mininet/issues/337)/#339: https://github.com/mininet-labs/Mininet/issues/337

Privacy transport [#291](https://github.com/mininet-labs/Mininet/issues/291): https://github.com/mininet-labs/Mininet/issues/291

Founding KEL assurance decision [#315](https://github.com/mininet-labs/Mininet/issues/315): https://github.com/mininet-labs/Mininet/issues/315

