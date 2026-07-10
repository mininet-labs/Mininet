# Private Human Continuity Proof — founder decision (D-0075)

Resolves [roadmap #21](../../issues/21) and updates `docs/gates/
personhood-signal-b-decision.md`'s Option A/B/C choice. The founder does
not pick one of those three cleanly — the decision **redefines signal
(b) itself**, then funds a narrow research program plus a conservative
interim implementation. This document is a design spec, not a
completed construction: see "What remains open."

## The redefinition

Old framing: *behavioral/location entropy as proof of human uniqueness*
— already named unsolved research by the whitepaper and by
`personhood-signal-b-decision.md`.

New framing: **a private, time-accumulated proof that one hidden
Mininet identity has continuously controlled a diverse collection of
independently rooted human-existence signals.**

This fits inside D-0038 rather than reversing it: D-0038 already made
`mini-uniqueness::status` an open-ended, extensible multi-signal
accumulator specifically so the system doesn't depend on any one signal
being solved, and D-0054 already requires the seed-anchored vouching
graph to be *live* for `FullHuman`, not merely one-of-N. This document
adds a new, richer optional signal source under that same architecture
— it does not touch D-0054's requirement.

**Security objective, reframed:** not "can this device perform
human-like behavior" (increasingly passable by AI/scripts/farms/paid
operators) but "has this hidden identity maintained a difficult-to-
reproduce web of independent, time-separated relationships with humans,
devices, homes, institutions, and authenticated services." Five
properties: continuity (same hidden identity persists), diversity
(unrelated trust domains), human anchoring (a path to already-recognized
humans), non-reuse (no anchor binds two Mininet roots), current control
(periodic live proof of continued control).

**Honest limit, stated up front and never softened:** this cannot make
Sybils mathematically impossible. A well-resourced attacker using real
cooperating humans can still build genuine-looking identities. The
achievable target is that each additional mature identity requires its
own long-lived collection of scarce, independently rooted evidence — the
same "no longer cheap, not impossible" claim the whitepaper already
makes for the system as a whole (§11), extended to this specific signal.

## 1. Identity-secret architecture

One private `human_secret` per person, controlling `did:mini` but never
revealed to any issuer/witness/website/government/network. Per evidence
provider: `pairwise_pseudonym = PRF(human_secret, provider_domain)` — a
government, a bank, a family member, and a web-notary each see a
*different* pseudonym and cannot compare notes. The final proof shows in
zero knowledge that every accepted credential binds to pseudonyms
derived from the same hidden secret — continuity without one globally
visible identifier.

## 2. Evidence stamps

Raw evidence never enters the network. It is converted locally into
short-lived `EvidenceStamp`s: `signal_class`, `trust_domain`,
`time_bucket`, `subject_pairwise_pseudonym`, `quality_bucket`,
`expires_at`, `revocation_handle`, `one_root_binding_nullifier`,
`issuer_signature`. No stamp contains browsing history, location trail,
government number, family graph, biometric template, or account name.
Each stamp: one signal class, limited lifetime, bound to one hidden
identity, revocable, selectively presentable, produces a nullifier
preventing prohibited reuse, contributes only a capped amount of
confidence.

## 3. Signal classes and interim weights

| Signal class | Max contribution | Function |
|---|---|---|
| Seed-connected human vouching | 30 | path into the existing human community (D-0054's required live signal) |
| Repeated physical co-presence | 20 | recurring interaction with independent humans |
| Device or home-node continuity | 15 | long-term control of persistent hardware |
| Government or external credential | 15 | institutional evidence of a living registered person |
| Authenticated web-life continuity | 10 | control of aged accounts across real services |
| Household or family relations | 5 | persistent human relationship |
| Ephemeral live-interaction evidence | 5 | current active control |

Provisional `FullHuman` requirements via this signal: fused score ≥ 70;
≥ 4 signal classes; ≥ 3 independent trust domains; ≥ 180 days since
first accepted evidence; live seed-connected vouching evidence present;
evidence spread across ≥ 4 separated monthly/quarterly periods; no one
trust domain supplying more than 35% of the accepted score. **These are
provisional simulation parameters, not constitutional constants** — see
D-0054 for the separately-enforced hard requirement that seed-anchored
vouching stay live regardless of this signal's score.

## 4. Independence discipline

Signals sharing an organizational root don't count as independent:
e.g. Android attestation + a Google account + Gmail activity share a
Google dependency; several accounts at one bank are one institutional
domain; several relatives in one household are one relationship
cluster; several devices owned by one person are one hardware domain
unless independently witnessed. Within one trust domain: strongest
signal at full value, second at ≤ 25%, further correlated signals at
zero. The trust-domain dependency map must be public, governance-
controlled, and conservative — otherwise an attacker manufactures
"diversity" from credentials ultimately rooted in one company, state,
household, or machine.

## 5. Authenticated web-life continuity — narrow predicates, not browsing history

General browsing history is never uploaded or treated as evidence (easy
to automate/fabricate, extremely revealing of medical/political/
religious/sexual/financial life, biased toward constant-internet-access
people). Instead: narrowly defined predicates from authenticated TLS
sessions (account age > 24 months; authenticated activity in ≥ 8 of the
last 12 months; accounts from ≥ 3 unrelated service classes; this
account not already bound to another active Mininet root) — never
service name, username, dates, page contents, searches, messages,
amounts, contacts, or URLs. A web-notary cluster witnesses the TLS
session, the user proves the predicate, the notary issues an unlinkable
`WebContinuityStamp`, session tokens and page contents are discarded.
Services are grouped into privately provable categories (government
portal, utility, education, employment, regulated finance, long-lived
communications, community/professional org) via private membership in a
governance-published Merkle set — an attacker-controlled site gets no
trust weight merely for using HTTPS.

## 6. Government accreditation — optional, capped, never sovereign

A person may privately prove: valid + non-revoked credential, an age
range, that the issuer recognizes one civil person, non-reuse against
another Mininet root — without the network learning name, document
number, birth date, nationality (unless specifically needed), address,
photo, or issuing office. Duplicate suppression uses blind issuance/OPRF:
`government_binding_nullifier = OPRF(government_subject_secret,
"mininet-human-v1")` — the government confirms one-subject-one-credential
but never learns the resulting nullifier or `did:mini`. An issuer that
can't support privacy-preserving uniqueness still contributes an
age/possession proof, but at lower weight and never as a uniqueness
anchor. Capped at 15 points regardless — can speed maturity, never
independently create `FullHuman`. This is Directive 8 applied here:
governments attest facts, verified humans stay the legitimacy root.

## 7. Home/device continuity

A phone, home node, router-class device, or laptop derives a
non-exportable (or locally protected) key and answers unpredictable
continuity challenges over separated epochs. A valid stamp proves the
same key answered across several epochs, challenges weren't
predictable, the device stayed available over a meaningful period, and
the anchor binds to only one active human root — never IP, street
address, Wi-Fi name, GPS, uptime schedule, or serial number. Hardware
attestation (WebAuthn, platform attestation) may raise confidence but
stays optional and capped, since not all devices support it — an old
unsupported phone still participates with a software-protected key plus
witness evidence, at lower hardware confidence, never with permanent
exclusion (Directive 11: usable by the weakest honest device).

## 8. Family/household relationships

Mutual, revocable-by-either-party, private, non-public, unable to
transfer identity control, limited score, unable to bootstrap an
isolated cluster alone. Meaningful weight only when both identities
already have some external anchoring, the relationship persists, at
least one outside relationship exists, and the household doesn't
recursively create unlimited trust. Scoring saturates fast: first
household relationship gets limited weight, second smaller, further
ones add nothing — preventing ten mutually-vouching devices in one
household from becoming ten fully verified identities.

## 9. Temporary live-data proof

Unpredictable interactive challenges (changing visual/audio prompt,
random motion sequence, short peer co-presence ceremony, touch+movement+
timing+device key) — raw camera/microphone/touch/motion data stays in
volatile memory, is processed locally, is never uploaded, never enters
the permanent record, and is erased immediately after the evidence
commitment is produced; no reusable biometric template results. The
stamp proves only that a valid challenge was completed in epoch *E* by
the identity key's controller — not global uniqueness. Accessibility
alternatives must always exist for blind/deaf/motor-impaired/homebound/
cognitively atypical people. Capped at 5 points given this remains a
research-grade primitive, not a solved one.

## 10. Two nullifier layers

- **Epoch claim nullifier** — `PRF(human_secret, "human-share" ||
  epoch)` — prevents one Mininet root claiming Human Share (D-0074) more
  than once per epoch.
- **Evidence binding nullifier** — `PRF(anchor_secret,
  "mininet-root-binding-v1")` — prevents one strong external anchor
  (government seed, authenticated account secret, hardware-attested
  device secret, relationship credential, witness credential) maturing
  multiple Mininet roots. Published once at first binding; later
  presentations prove continued ownership without republishing a stable
  identifier. Transfer to a replacement `did:mini` only through the
  formal recovery protocol — never by binding to a second live root.

## 11. Slow vesting tied to evidence maturity

Evidence determines the *speed of release*, not the *amount* of the
human right — every honest person still receives the same Human Share
formula (D-0074).

| Maturity | Max unlocked |
|---|---|
| First human-rooted vouch + initial evidence | 10% |
| 30 days, ≥ 2 live classes | 25% |
| 90 days, ≥ 3 classes | 50% |
| 180 days, `FullHuman` policy satisfied | 75% |
| 365 days, continuity across ≥ 4 classes | 100% |

The remainder accrues as a locked personal claim — never lost for
lacking government ID, modern hardware, conventional banking, mobility,
family, social media, or regular connectivity. A complete
no-government/no-bank/no-biometric path must exist (seed-connected
vouching + repeated physical co-presence + elapsed time + device/home
continuity + limited web activity or additional witnesses), plus an
offline-heavy path. Possessing a passport, expensive phone, bank
account, or home server never increases total Human Share — it may
reduce uncertainty sooner, but the mandatory age floor prevents wealthy
people from purchasing instant maturity.

## 12. Why this makes farms expensive, and what it still can't stop

An automated farm can fabricate browser profiles, software keys,
activity logs, virtual devices, self-signed presence events, and
attacker-controlled websites — none of that alone crosses the threshold.
For every mature Sybil the attacker must additionally obtain and
*maintain*: seed-connected human trust, recurring real-world co-presence,
non-reused external credentials, aged authenticated accounts at
independent services, persistent physical devices, household/community
relationships, unpredictable live participation, and months of elapsed
time — not all sourceable from one company, government, household,
witness, device farm, or attacker-run website. The attack changes from
"create another key and run a script" to "build and maintain another
credible human life footprint across several unrelated domains for six
to twelve months."

**What remains genuinely impossible to defeat:** paid genuine humans
(cryptography can't tell independent action from paid/coerced action);
corrupt governments issuing several credentials to one operator (why
government evidence is capped and non-substituting); one real person
holding multiple legitimate lives (several citizenships/devices/
accounts/households can't be proven to be one biological person without
an authoritative global biometric system, which this design explicitly
does not build); stolen identity/coercion (periodic live control,
recovery, revocation, and relationships reduce but don't eliminate this
risk); a patient, resourced nation-state (already recorded in the
repository as defeating a pure social/behavioral system). **The honest
production claim stays:** Mininet strongly limits cheap/automated Sybil
creation and makes mature identities costly to manufacture at scale. It
does **not** claim to mathematically prove one biological human has
exactly one identity.

## 13. Privacy requirements (mandatory, not aspirational)

Raw behavioral/location data never leaves the device; exact websites and
locations are never disclosed; family/social graphs stay hidden;
government identifiers stay hidden; repeated proofs are unlinkable
outside the Mininet root; providers see pairwise pseudonyms, never the
global identity; evidence is bucketed, not exact-timestamped; raw
challenge data is destroyed immediately; no permanent biometric template
is required; source-specific stable nullifiers appear only where
unavoidable for one-root binding; all proof policies are open and
reproducible. This is Directive 9 — the network should never learn what
it doesn't have to.

## 14. Weak-device architecture

Evidence reduces incrementally at the source (event → local check →
compact stamp → raw data erased → wallet stores only the stamp), so
monthly aggregation proves over dozens of stamps, not millions of raw
events. Prefer native anonymous-credential proofs over one giant
general-purpose SNARK where possible. A home node may generate the
aggregate proof for its owner, but owning home hardware can never be
required — the reference phone must be able to generate the proof
slowly, incrementally, or while charging, and verification must stay
fast enough for weak nodes. No remote prover ever receives unencrypted
behavioral, government, relationship, or browsing witnesses.

## Research program

Narrowly scoped tracks, not "solve proof of humanity":

- **A — private TLS life proofs:** account age/active-month-count/
  source-category-membership/cross-provider-diversity/non-reuse without
  revealing providers or account identities.
- **B — sensor provenance:** what's provable about temporary local
  sensor input without mandatory vendor attestation, permanent hardware
  allowlists, raw-data upload, or excluding old devices — a formal
  impossibility/limitation result is a valuable outcome here too.
- **C — private co-presence diversity:** possession of credentials from
  ≥ k distinct witnesses across ≥ m epochs over ≥ d elapsed time with no
  reused event, without revealing witnesses, times, places, or the
  social graph.
- **D — blind uniqueness credentials:** OPRF-based issuance letting
  governments/institutions issue a one-per-person binding anchor without
  learning the resulting nullifier or identity.
- **E — coercion/puppeteering modeling:** identity rental, credential
  surrender, controlled voting, purchased vouches, household control,
  employer/state coercion.
- **F — weak-device proving:** benchmark every proposed proof on the
  oldest supported phone and low-cost home hardware.

## Implementation phases

1. Evidence framework: typed `EvidenceStamp`, pairwise pseudonyms,
   issuer/trust-domain registry, source caps, expiry/decay, binding-
   nullifier registry, epoch claim nullifiers, mock issuers for
   simulation. No browsing/behavioral collection required yet.
2. Practical adapters: home/device continuity, private co-presence
   stamps, W3C VC/OpenID4VCI credential import, TLS-notary-style web
   predicates, family/household credentials.
3. Aggregate proof: one hidden-root binding, diversity/time
   requirements, nullifier non-reuse, epoch non-duplication.
4. Live-data research prototype: no retained raw data, accessibility
   alternatives, strict low weighting, independent privacy/crypto review.
5. Adversarial simulation: automated browser/device farms, fake
   websites, corrupt issuers, purchased vouches, family rings, sleeping
   Sybils, nation-state production, stolen credentials, long-term
   credential rental — production weights get selected from simulation,
   not intuition.

## What remains open

This document is the founder's design decision, not a shipped
construction. `personhood-signal-b-decision.md`'s "what closes this
gate" is superseded by: implement the evidence/nullifier model
(Phase 1), prototype web and home-device continuity (Phase 2), build the
aggregate ZK proof (Phase 3), fund Research Tracks A–F, calibrate every
threshold in §3/§11 through adversarial simulation (Phase 5) before any
of it becomes load-bearing, and preserve `VouchingGraph` as a required
live `FullHuman` source (D-0054) unless a future recorded decision
provides an equally human-rooted replacement. #21 stays open, retitled,
as the research-and-integration issue this describes — it is not a
launch blocker, since D-0038/D-0054 already ensure the system doesn't
depend on this signal alone.
