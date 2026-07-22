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
| **Timestamp manipulation** | `mini_chain::BlockHeader::timestamp_ms` was proposer-controlled and completely unchecked — a proposer could set any value, including flat or decreasing across heights, biasing any downstream logic that assumes monotonic block time (roadmap #44). | `mini_execution::LedgerChain::apply_finalized_block` and `mini-consensus`'s `validate_proposal` require `timestamp_ms` to equal the block's own height exactly — deterministic logical time, not proposer discretion bounded by a rule (D-0085 first shipped a monotonicity-only bound; D-0087 tightened it after noting a merely-increasing value, e.g. jumping straight to `u64::MAX`, would still satisfy monotonicity). | ✅ Closed — the proposer has no discretion over this field at all, so there is nothing left to bias; a real wall-clock consensus protocol is separate, not-yet-designed future work, not a residual gap in what exists today. |
| **Cross-domain signature replay** | `mini_chain::Vote`'s signed transcript had no domain-separation tag — every *other* signed transcript in this workspace (`mini-consensus`'s `Proposal`, `mini-settlement`'s `PaymentClaim`, `mini-bounty`'s claim) already prepends one, per `CLAUDE.md`'s typed-domain rule. A vote was the one place a signature over the wrong domain's bytes could in principle be replayed as a vote if the byte layouts ever collided (roadmap #44). | `VOTE_SIGN_DOMAIN` now prepended to `Vote::transcript` (D-0085) | ✅ Closed — same-workspace domain confusion is now structurally impossible; a genuine multi-chain/multi-network chain-id concept (for replay *across separate deployments* of this same protocol) remains unbuilt and is separate, larger follow-up work. |
| **Fee-mechanism manipulation** | `mini_value::fee::PriceHistory::add_entry` accepted a governed price of `0`, which would make every fee free regardless of the real-world value target; separately, `fee_in_micro_mini`'s final `u128`-to-`u64` conversion used an `as` cast, which truncates silently on overflow instead of failing (roadmap #44). | `PriceHistory::add_entry` and `fee_in_micro_mini` both reject a zero rate (D-0085); `fee_in_micro_mini` now returns `Result<u64>` and rejects an unrepresentable quote via `u64::try_from` instead of truncating it (D-0087) | Partial — both code-level bugs (zero rate, silent overflow) are closed; a rate-limit/max-jump bound between consecutive prices is a genuine governance-policy question (how fast *should* the price be allowed to move), deliberately left to a founder decision rather than invented unilaterally. |
| **Stale-KEL replay** | `did_mini::verify_delegation` checks only the KEL it is handed; a verifier given an old, pre-revocation copy of a root's KEL still sees a revoked device as validly delegated (audit #12 finding F4, founder review's `kel-freshness` P0 item). | `did_mini::FreshnessPins::check_and_pin` (D-0088): rejects a KEL whose `sn` is lower than one already pinned for that SCID | Partial — closes the case where a verifier has already seen the fresher KEL at some point; a verifier who has *never* seen it has nothing to pin against, and that residual case needs real witness receipts and gossip-based duplicity proofs (SPEC-01 §7), still unbuilt. |
| **Dropped equivocation evidence** | `mini-consensus::net`'s network driver received genuine, independently-verifiable proof of double-signing (`Emit::Equivocation`) and discarded it unconditionally — accountability evidence that reached the wire had nowhere to go (founder review's `consensus-evidence` P0 item, §5.4). | `mini_consensus::EquivocatorRegistry` (D-0088): independently re-verifies and records every equivocation emit instead of dropping it | Partial — the evidence is now durably recorded and queryable instead of thrown away, but nothing yet *acts* on a flagged root (no exclusion from a validator epoch, no economic penalty) since dynamic validator-set transitions don't exist yet (roadmap #36-#45); safety was never at risk either way (an equivocator's vote already counts at most once, P2). |

## 3. Economic threats — the currency dying without anyone attacking it

| Threat | Why it matters for Mininet specifically | Stopped by | Status |
|---|---|---|---|
| **Dead economy** | A currency nobody uses for real value transfer, existing only as a governance-adjacent token, would validate every critic who says "voice/value walls are pointless if there's no value to wall off." | None — this is a market-adoption risk, not a protocol-enforceable one | Aspirational — no invariant can force usage. This risk is managed by product/ecosystem decisions (mini-bounty, real payment rails), not code. |
| **Whale concentration** | Even without governance capture (see §1), a small number of holders controlling most circulating value undermines the "thousand cheap machines beat one warehouse" thesis economically, not just technically. | M1-M3 (money never merges, never CRDT-collapses, canonical ordering resolves disputes) protect *integrity* of holdings, not their *distribution*; D-0074 (`docs/design/inflation-and-whale-resistance.md`) now designs a 3% issuance ceiling and an enumerated anti-whale governance-input wall | Design decided, not yet implemented or simulated — D-0074 fixes the mechanism and formal wall; the 200-year adversarial simulation suite that validates the calibration hasn't been built or run (`docs/gates/economic-simulation-spec.md`). |
| **Fee starvation** | If transaction fees are the only sustainable validator/storage incentive and usage stays low, the honest-majority security model (Directive 15) degrades because there's no longer sufficient reward to keep enough honest capacity online. | V2 (storage/seeding earns value); D-0074's 0.75%/yr service-reward channel | Partial — the invariant establishes *that* storage earns value and D-0074 now bounds the channel, but whether 0.75%/yr is *sufficient* under low-usage conditions is still an open simulation question, not yet run. |
| **Treasury capture** | Any pooled treasury (bounty pools, mini-treasury, storage reward pools) is a concentrated target — exactly the kind of central point of failure Directive 2 warns about, even when the treasury itself is protocol-native rather than a company. | A1 (production audit gate), D-0047; D-0073 (`docs/design/treasury-economic-model.md`) now designs cellular ≤10%-per-vault custody, signer eligibility from verified-human governance only, and an explicit governance may/may-not list | Design decided, not yet implemented or audited — D-0073 fixes the mechanism (cellular custody, signer rotation, three-separated security domains for receipts/custody/issuance); the chain-specific FROST/XRP and FROST/XMR custody integrations still need external audit (#93) before real value, and the mechanism itself is unimplemented (`docs/STATUS.md`). |

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

## 6. Edge-provider threats — the world Mininet touches but does not control

Founder Directive 18 (D-0352): the core must survive the total
disappearance of any bank, carrier, courier, state, vendor, or court it
touches. This category exists because a core that is individually safe
from each of these threats can still fail in aggregate if the *edge*
quietly re-centralizes what the core was designed to keep decentralized.

| Threat | Why it matters for Mininet specifically | Stopped by | Status |
|---|---|---|---|
| **Provider capture as an attack class** | If 90% of humans reach the network through three providers (a dominant conversion/card issuer, a dominant carrier, a dominant vendor), the network is centralized in practice while decentralized on paper — the same failure mode as a chain with "permissionless" validators that in practice run on three cloud regions. No individual FD-18 non-negotiable is violated by any single user's choice, yet the aggregate outcome is exactly what the core was built to prevent. | INV-18-04/INV-18-05 (no canonical registry, no network-wide provider off switch) prevent the *protocol* from picking winners; they do nothing about *organic* concentration from convenience/marketing/network effects | Aspirational — no invariant can force diversity of provider choice, the same way no invariant can force usage (§3's "dead economy" entry). Clients should surface provider-concentration metrics to at least make the drift legible to users, rather than invisible; this is unbuilt. |
| **Provider-as-shadow-governance** | A provider large enough to matter economically could attempt to informally influence protocol development, review outcomes, or roadmap priority without ever touching a vote — buying access, not votes. | Directive 16 (voice/value wall), INV-18-02 (no edge crate reaches governance) | Partial — the wall stops a *technical* dependency edge from governance to an edge crate; it does not stop informal, off-protocol influence (lobbying, funding developers, sponsoring audits with strings attached), the same residual gap §1's "governance capture" entry already names for wealth generally. |
| **Declaration dishonesty at the edge** | A provider could publish a `ProviderDeclaration` that understates its actual custody/freeze/death/exit posture — the doctrine requires the fields exist and be mandatory, not that their contents be true. | INV-18-09 (mandatory, non-`Option` fields) forces a claim to be made; nothing yet forces the claim to be checked | Aspirational — mandatory disclosure is not verified disclosure. A reputation/reporting layer for catching a provider that lied in its own declaration is undesigned, and is exactly the kind of "curated list"/`ProviderRanker` problem Part II.1 leaves to client-side plugins rather than protocol consensus, on purpose (Directive 16: no canonical registry, no protocol-level arbiter of provider trustworthiness). |
| **Succession/voice-transfer attempts** | Directive 18's card/estate doctrine touches death and inheritance directly — an attacker (or a well-meaning legal system) could attempt to route a deceased identity root's governance weight to an heir, executor, or estate, effectively buying a second vote through inheritance law. | INV-18-07 (a vote extinguishes at death and cannot transfer) | Partial by design — the invariant is a hard "no" at the protocol level; it cannot stop a jurisdiction's court from *believing* it has ordered a transfer, only stop the protocol from honoring that order. `mini-succession` (Wave 3) is unbuilt. |

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
