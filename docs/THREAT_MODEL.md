# Threat model — what could actually kill this

Founder direction (2026-07-08): *"Not a security checklist. A civilization
threat model."* This document is the fifth of the project's canonical
documents, alongside `docs/FOUNDER_DIRECTIVES.md`, `docs/INVARIANTS.md`,
`docs/DECISION_LOG.md`, and `docs/FAILURE_BOOK.md`.

## How this document is different from the other four

- **`FOUNDER_DIRECTIVES.md`** answers *why this project exists and what it
  values*.
- **`INVARIANTS.md`** answers *what can never be broken*, and now carries
  a **Directive** column tracing each invariant back to a directive.
- **`DECISION_LOG.md`** answers *why a specific decision was made, and
  when it was superseded*.
- **`FAILURE_BOOK.md`** answers *what was tried and rejected, and why*.
- **This document** answers a different question from all four: *if
  Mininet fails, what does the failure look like, and which invariant was
  supposed to stop it?* It is a catalog of adversaries and forces, not a
  catalog of decisions. Most entries here point at one or more
  `INVARIANTS.md` IDs; some point at nothing yet, because the invariant
  that would stop them hasn't been designed. Both cases are worth writing
  down — an empty "stopped by" column is itself a finding.

Standard infosec threat models stop at the technical layer: an attacker,
a system, a set of CVEs. That is necessary but not sufficient for a
network meant to run for centuries (Directive 13). A currency can die of
economics with a perfectly secure protocol. A network can die of
governance capture with perfectly sound cryptography. A civilization-scale
system has to be threat-modeled at civilization scale, or the modeling is
theater.

## How to use this file

For each threat: a one-line description, why it matters here specifically
(not a generic definition), which invariant(s)/directive(s) are the
defense today, and an honest note on whether that defense is real,
partial, or aspirational. Entries are not exhaustive — new ones get added
as they're found, the same append-only spirit as `DECISION_LOG.md`, though
this file is a living catalog rather than a strict append-only log:
existing entries may be edited to update their "stopped by" status as
invariants mature, but the threat itself is never deleted once identified,
only marked resolved or superseded.

---

## 1. Human threats — the people, not the machine, are the attack surface

The founder's own framing: most systems that fail, fail because of people,
not math. This category exists because Directive 1 ("Humanity Before
Technology") and Directive 2 ("Assume Every Central Authority Will
Eventually Fail") both point here first.

| Threat | Why it matters for Mininet specifically | Stopped by | Status |
|---|---|---|---|
| **Bribery** | Any quorum-based system (finality votes, forge governance) can be bought if the value of controlling a decision exceeds the cost of bribing enough identity roots. | P2, V1 (equal weight, no balance-based influence) | Partial — equal weight raises the *number* of parties that must be bribed, but does not make bribery impossible, and the identity-root-not-human gap (see `INVARIANTS.md`'s hard limitations) means the true cost of buying "enough roots" is unknown until Sybil-resistant personhood exists ([#18](../../issues/18)). |
| **Coercion** | A validator or identity-root holder can be threatened rather than paid — a strictly worse threat than bribery because it can't be priced or insured against. | None dedicated | Aspirational — no invariant addresses duress today; census-resistant, coercion-resistant voting (deniable participation, plausible non-voting) is undesigned. |
| **Governance capture** | A minority that controls enough delegated `VOTE` capability, tooling, or communication channels can steer forge decisions without ever violating P1/P2 on paper. | P1, P2, V1 | Partial — the invariants stop *balance* capture, not *coordination* capture (cartels of otherwise-legitimate identity roots acting in concert). No invariant currently distinguishes organic consensus from coordinated capture. |
| **Cult formation / personality cults / founder worship** | Named explicitly by the founder as a threat to guard against, including guarding against *themselves* — Directive 3 ("The Network Must Outlive Its Creators") exists precisely so no single person, including the founder, becomes a single point of failure or an object of unquestionable authority. | Directive 3, Directive 2 | Aspirational — this is currently enforced by *documentation and stated intent*, not by any code or protocol mechanism. There is no invariant that limits founder authority mechanically (e.g., no forced key rotation away from founder control, no sunset clause on any founder-held capability). This is an honest, open gap worth naming rather than papering over. |
| **Wealth concentration** | Even with P1's voice/value wall holding perfectly, extreme value concentration can still buy influence *outside* the protocol — hiring developers, running the largest storage/validator fleet, funding a rival client. | P1 (protects governance specifically, not influence generally) | Partial by design — P1 was never meant to prevent wealth from existing, only from converting directly into votes. Economic threats below (§3) cover the rest. |

## 2. Technical threats — the machine

| Threat | Why it matters for Mininet specifically | Stopped by | Status |
|---|---|---|---|
| **Quantum** | Every signature scheme in the tree today (Ed25519-family, ring signatures, stealth addresses) is broken by a sufficiently large fault-tolerant quantum computer. Directive 13 ("Think in Centuries") makes this not-hypothetical. | X2 (crypto-agility) | Partial — X2 means algorithms can be swapped without redesigning the protocol shape, but no post-quantum scheme is implemented or chosen yet. Migration path for *already-issued* identities/funds under a live quantum break is undesigned. |
| **Routing attacks** | A DHT-based discovery layer (`mini-net`, Layer 2 of `ADDRESSING.md`) is vulnerable to classic Kademlia-style attacks: a malicious node can lie about routing tables to isolate or misdirect a peer. | N2 (real transport exists), Layer-1 self-certification (`ADDRESSING.md`) | Partial — a lying router can only fail to route or misdirect, never forge an identity that passes Layer-1 verification (this is the core claim of `ADDRESSING.md`), but availability-level routing attacks (eclipse, below) are not separately defended. |
| **Eclipse attacks** | An attacker who controls all of a victim's peer connections can feed it a false view of the network — false finality, false fork state — without breaking any cryptographic primitive. | V1 (BFT quorum requires >2/3 of real validators, not just what one's local view shows) | Partial — the >2/3 requirement makes eclipsing a validator's *finality view* hard once genuinely connected to the network, but a freshly-bootstrapping device (see `ADDRESSING.md`'s bootstrap section) is exactly the point of maximum eclipse vulnerability, and no invariant currently addresses first-contact eclipse risk. |
| **Sybil attacks** | The sharpest open question flagged in the [#10 frozen-invariants review](audits/issue-10-frozen-invariants-review.md): identity-root creation cost determines how expensive it is to fake "many humans." | P2 (aspirationally), the identity-root hard limitation in `INVARIANTS.md` | **Explicitly unresolved** — this is the single most-flagged gap across `INVARIANTS.md`'s hard limitations, the #10 audit, and this document. Every other invariant that counts identity roots (P1, P2, V1) inherits this risk. |
| **Storage fraud** | The exact warehouse-consolidation attack named in `INVARIANTS.md`'s second hard limitation: one well-resourced server answering challenges for many claimed identities. | S1 (proof-of-space-time) | **Explicitly unresolved** — proves possession, not replication uniqueness; real proof-of-replication is [#31](../../issues/31), not yet built. |
| **Supply chain** | A malicious dependency, a compromised build toolchain, or a poisoned release artifact can compromise every device that trusts a Mininet binary — the same class of attack that hit `event-stream`, `xz`/liblzma, and countless others. | Reproducible builds CI (D-0044), dependency-audit CI (D-0044) | Partial — reproducibility lets anyone *verify* a released binary matches source, and dependency scanning catches *known* CVEs in dependencies; neither stops a novel, undisclosed compromise of a maintainer's own commit access or of a dependency before an advisory exists. No code-signing/multi-party release-signing scheme exists yet. |

## 3. Economic threats — the currency dying without anyone attacking it

| Threat | Why it matters for Mininet specifically | Stopped by | Status |
|---|---|---|---|
| **Dead economy** | A currency nobody uses for real value transfer, existing only as a governance-adjacent token, would validate every critic who says "voice/value walls are pointless if there's no value to wall off." | None — this is a market-adoption risk, not a protocol-enforceable one | Aspirational — no invariant can force usage. This risk is managed by product/ecosystem decisions (mini-bounty, real payment rails), not code. |
| **Whale concentration** | Even without governance capture (see §1), a small number of holders controlling most circulating value undermines the "thousand cheap machines beat one warehouse" thesis economically, not just technically. | M1-M3 (money never merges, never CRDT-collapses, canonical ordering resolves disputes) protect *integrity* of holdings, not their *distribution* | Not addressed — no invariant limits concentration; this is a distributional/policy question, likely for `mini-treasury`/emission design, not yet decided. |
| **Fee starvation** | If transaction fees are the only sustainable validator/storage incentive and usage stays low, the honest-majority security model (Directive 15) degrades because there's no longer sufficient reward to keep enough honest capacity online. | V2 (storage/seeding earns value) | Partial — the invariant establishes *that* storage earns value, not that the amount is sufficient under low-usage conditions; this is an emission/economics design question. |
| **Treasury capture** | Any pooled treasury (bounty pools, mini-treasury, storage reward pools) is a concentrated target — exactly the kind of central point of failure Directive 2 warns about, even when the treasury itself is protocol-native rather than a company. | A1 (production audit gate), D-0047 | Partial — the audit gate reduces the chance of a *cryptographic* flaw enabling theft; it does not address *governance* capture of a treasury's spending authority, which is a §1 human-threat question as much as a technical one. |

## 4. Political threats — states and infrastructure gatekeepers

| Threat | Why it matters for Mininet specifically | Stopped by | Status |
|---|---|---|---|
| **Nation-state censorship** | A state can block known bootstrap peers, IP ranges, or app-store listings — the same playbook used against Tor and VPNs. | Local-first bootstrap (D-0012, `ADDRESSING.md` Bootstrap section) | Partial — Bluetooth/local-Wi-Fi bootstrap means censorship requires physical proximity denial, not just network-level blocking, but the internet-fallback rendezvous-peer set (`ADDRESSING.md`) is exactly as blockable as any other known IP list until it has real diversity/rotation. |
| **Regulation** | Financial regulation (KYC/AML, securities law) could target `mini-value`/`mini-treasury` directly, or target exchanges/on-ramps that give the currency real-world liquidity. | None protocol-level; this is a jurisdiction-and-legal-structure question | Aspirational — out of scope for code; relevant to how any legal entity around Mininet is structured, not to `INVARIANTS.md`. |
| **Forced software updates** | A platform (app store, OS vendor) could compel a version that violates an invariant (e.g., adds telemetry that breaks P5/P6 privacy, or adds a kill switch) as a condition of distribution. | U1 (no forced auto-update) | Partial — U1 stops *Mininet's own* update mechanism from forcing anything, but cannot stop a third-party app store from refusing to distribute a compliant build, which pushes users toward sideloading/alternative distribution as a mitigation, not a solved problem. |
| **ISP blocking** | Traffic classification/DPI could identify and throttle or drop Mininet traffic specifically, short of a full network block. | N2 (real transport), local-first bootstrap | Not addressed — no traffic obfuscation/pluggable-transport equivalent (à la Tor's pluggable transports) exists or is designed. |

## 5. Civilization-scale threats — beyond any single actor

The founder's framing again: these are not "will someone attack us" but
"will the world itself remain hospitable to this design." Directive 13
("Think in Centuries") is the reason this category exists at all — most
projects don't plan past a funding cycle, let alone past infrastructure
assumptions holding for generations.

| Threat | Why it matters for Mininet specifically | Stopped by | Status |
|---|---|---|---|
| **Internet fragmentation** | A splintered internet (national intranets, hard borders on routing) breaks the assumption that any two devices can eventually find a path to each other. | Local-first, mesh-capable bootstrap design (`ADDRESSING.md`) | Partial — local-first design means fragments can each run a fully functional local Mininet; *reunification* of fragments once connectivity returns (canonical ordering across a long partition) is covered in principle by M3 but has never been tested at national-fragment scale or duration. |
| **Solar storm / large-scale infrastructure disruption** | A Carrington-scale event could take down large swaths of networked infrastructure simultaneously — a partition far larger and longer than any tested scenario. | Same partition-tolerance mechanisms as above | Not tested — no invariant or test currently models a partition at this scale/duration; canonical-ordering finality (M3) should in principle degrade gracefully to "everyone keeps a local ledger, reconciles later," but this has not been validated. |
| **Planetary latency** | If Mininet is ever used off-Earth (interplanetary relay, even just as a design constraint worth taking seriously under "think in centuries"), light-speed latency alone breaks any consensus mechanism assuming sub-second or even multi-second round trips. | None | Undesigned — V1's BFT quorum has no defined behavior under multi-minute round-trip latency; this is intentionally out of scope for the current tree but named here so it's not forgotten if the ambition is genuinely centuries-long. |
| **AI superintelligence** | An AI system materially more capable than the humans/AI currently building or reviewing Mininet could out-reason every current invariant, find governance-capture strategies humans wouldn't see, or simply out-compete the honest-majority assumption (Directive 15) at a speed no human quorum can respond to. | Directive 12 (AI Serves Humanity), AI1 (AI draft/human review gate) | Aspirational — AI1 governs *how Mininet's own code gets written*, not how the network defends itself against a misaligned or adversarial superintelligent actor participating in it. This is named honestly as unsolved, not quietly ignored. |
| **Long-term cryptographic decay** | Every cryptographic assumption (discrete log hardness, hash collision resistance) has a shelf life measured in decades, not centuries. A network meant to outlive its creators (Directive 3) will outlive today's cryptography. | X2 (crypto-agility) | Partial — same status as the quantum entry above: the *mechanism* to swap algorithms exists in principle, the actual migration path for live funds/identities under a broken primitive does not. |
| **Humanity splitting across worlds** | Named explicitly by the founder — if humanity becomes multi-planetary, "one human, one vote" (P2) and co-presence attestation (PH1) both encode assumptions (shared clock, boundable RTT, a single "now") that stop holding across interplanetary distances. | None | Undesigned — flagged here specifically so a future maintainer doesn't have to rediscover that P2/PH1 quietly assume a single-planet network; this is the furthest-out entry in this document and is recorded for exactly the reason Directive 13 exists. |

---

## What this document is not

- **Not a replacement for `INVARIANTS.md`.** A threat with no invariant
  stopping it is not itself a violation of anything — it's a gap, logged
  honestly so it can be designed for later, exactly the way the two hard
  limitations at the top of `INVARIANTS.md` are logged.
- **Not a roadmap.** Some entries here point at open roadmap issues,
  most do not yet have one filed. Filing an issue for an unresolved
  threat is appropriate follow-up work, not something this document does
  itself.
- **Not exhaustive, and not meant to be closed out.** New threats get
  added as they're found; existing ones get their "Stopped by"/"Status"
  columns updated as invariants mature. A document like this that stops
  growing is a sign no one is looking anymore, not a sign the network is
  safe.
