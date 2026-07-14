# Sphinx-style mix network: research report and protocol specification (MN-204)

D-0305. Lane L3 of `docs/design/privacy-cost-doctrine-parallel-execution-plan.md`
(D-0300), closes tracking issue #135. Research and protocol specification
only — **zero Rust code in this lane**, matching L3's declared footprint.
Companion to `docs/research/MININET_RESEARCH_V2_20260713.md` (the source
research this whole track implements) and to the two crates this document
exists to eventually feed: `mini_privacy_policy::Mechanism::MixNetwork`
(D-0094) and `mini_transport_policy`'s `PrivacyTier::Mixed` mechanism list
(D-0301), which already *name* mix-network mixing as part of Tier 2 but
implement none of it — this document is what turns that named-but-empty
slot into a concrete, buildable specification.

## 0. Status, scope, and the constitutional footing this stands on

This is **Phase D** of the source research's phased sequence: "mixnet
research w/ external crypto review — explicitly 'do not market as
globally anonymous before this evidence exists.'" D-0300 flagged this
lane in advance: *"per the source research's own Phase D gate ... `MN-205`
should not start production-style implementation without the same
external-review posture already applied to `mini-value`/`mini-treasury`
(D-0047 gate)."* This document does not lift that gate. It exists to make
the gate meaningful — a reviewer needs a concrete specification to review,
not a paragraph of intent.

**On the no-new-cryptography rule (CLAUDE.md):** everything specified
here composes a single already-published, peer-reviewed, real-world-
deployed construction — Sphinx (Danezis & Goldberg, *IEEE S&P* 2009),
already carrying a decade-plus of production deployment history through
Loopix, Katzenpost, and Nym. This is the same class of prior-art
composition already accepted in this workspace for Bulletproofs
(`mini-value`, D-0036/D-0040) and Filecoin-style SDR sealing (`mini-porep`,
D-0064): implementing a construction the wider field has already vetted,
done in-house to keep governance in-house (D-0063), not inventing
something genuinely novel and unreviewed. Every primitive this
specification calls for — X25519 key agreement, HKDF-SHA256, ChaCha20-
Poly1305 AEAD, BLAKE3 — already exists in `mini-crypto`, already
reviewed, already used elsewhere in this workspace. **No new primitive
is proposed anywhere in this document.**

## 1. Executive summary

A mix network buys the properties Tier 0 (Direct) and Tier 1 (Relayed)
structurally cannot: resistance to a *global* passive observer correlating
traffic by timing and volume (research doc's adversary class A4, "the
floor lives here"), and resistance to blending/flooding attacks that a
simple relay chain has no defense against. It does this by combining
three independent mechanisms — fixed-size layered onion encryption
(Sphinx), randomized per-hop delay (Stop-and-Go / Loopix-style continuous
mixing), and independent cover traffic (Loopix-style loops) — each of
which raises the *cost* of correlation for an adversary without claiming
to make correlation impossible. This document surveys how that
combination arrived at its current form (§2), compares the live design
space (§3), lists the empirical work this repository still owes before
any bandwidth/latency number in `mini-privacy-policy` stops being a
declared estimate (§4), catalogs the attacks a reviewer will ask about
and states this design's residual risk against each (§5), explains why
Tor is not a substitute (§6), proposes a concrete candidate specification
for `MN-205` to implement (§7), and names what is deliberately left for
later so omissions read as decisions, not oversights (§8).

## 2. Historical evolution

**Chaum Mixes (1981).** David Chaum's original mix-net: a server batches
a fixed-size set of RSA-onion-encrypted messages, strips one layer per
hop, and outputs them in a different (typically random) order than they
arrived, breaking the input-position-to-output-position correlation a
naive relay would preserve. Foundational — the batching/reordering idea
every later design still uses in some form — but no formal security
proof, and naive threshold-batch designs are directly vulnerable to the
n-1/blending attack (§5): an adversary who can supply n-1 of a batch's n
messages trivially identifies the one honest message by elimination.

**Mixmaster (early-to-mid 1990s).** The first widely deployed remailer
network, fixed-size message fragmentation, still batch-based, still
email/store-and-forward latency (minutes to hours), no formal
integrity/replay mechanism beyond what operators added ad hoc.

**Stop-and-Go Mixes (Kesdogan, Egner, Büschkes, 1998).** The first
design to replace discrete batching with **continuous-time mixing**:
each message is delayed at each hop by a random draw from a known
distribution (originally proposed with a bound so a "genuine" delay is
statistically distinguishable from an adversarial "held forever" attack,
closing one blending-attack variant). This is the direct ancestor of
Loopix's exponential per-hop delay (below) — the idea that *when* a
message leaves a mix, not just the batch it was nominally in, is the
tunable anonymity/latency knob.

**Mixminion (Danezis, Dingledine, Mathewson, 2003) — "Type III"
remailer.** The first design with a rigorously specified packet format
and threat model paper: fixed-length packets, per-hop integrity via
message authentication rather than trusting the mix, and **single-use
reply blocks (SURBs)** — a sender can hand out a one-time-use return
path without revealing their own identity to the replier. Still
store-and-forward, still not compact (headers grow with path length in
the original design), but the paper's threat-model rigor set the bar
every later design is measured against.

**Sphinx (Danezis & Goldberg, *IEEE Symposium on Security and Privacy*
2009).** The construction this whole document is named for, and the one
still underneath every production mixnet in the historical line below.
Sphinx's contribution is specifically the **packet format**, not a full
network design: a header of **fixed size regardless of path length**
(achieved via a single group element re-randomized at each hop rather
than one key per hop stacked in the header), per-hop integrity via a
MAC computed from each hop's derived shared secret (so a modified packet
fails verification and is dropped, closing the naive tagging attack,
§5), a replay-detectable per-hop tag, and — critically for anonymity —
**bitwise unlinkability**: a mix cannot distinguish a forward packet from
a reply packet, nor tell how many hops remain, from the bytes alone.
Sphinx itself specifies no delay model and no cover-traffic policy; it
is the envelope, not the mixing strategy. Every design below is "Sphinx
packets plus a mixing/topology/cover-traffic policy."

**Loopix (Piotrowska, Hayes, Elahi, Meiser, Danezis, *USENIX Security*
2017).** Adds the mixing/topology/cover-traffic policy Sphinx itself
left open: **continuous-time Poisson mixing** (each hop delays a packet
by a draw from an exponential distribution — the Stop-and-Go idea,
formalized and analyzed), a **stratified topology** (a fixed number of
mix layers, each with a fixed set of nodes, rather than free routing
through an arbitrary relay graph — this bounds path-selection complexity
and makes anonymity-set analysis tractable), and **independent loop
cover traffic**: both senders and mix-providers generate self-addressed
decoy packets indistinguishable from real traffic, at a rate independent
of real traffic volume, specifically to defend against the low-traffic/
sparse-usage regime where a single real message stands out (research
doc's "anonymity-set corollary," §1.4). Loopix's paper is also the first
in this lineage with a formal treatment of the latency/bandwidth/
anonymity trilemma most later production systems cite directly.

**Katzenpost.** A production-oriented, open-source implementation of a
Loopix-style design (not an academic paper on its own so much as the
reference engineering artifact that made Loopix deployable): a real
directory-authority/PKI design for mix-node discovery, real client SURB
handling, and the codebase Nym's own mixnet is derived from. This is the
closest existing prior art to what `MN-205` would need to become a real
crate — worth studying its engineering choices even though this
workspace will not import its code (D-0034 point 3: reimplement proven
designs in-house, don't vendor a third-party P2P framework).

**Nym.** The first design in this lineage with a **live, deployed,
economically sustained public network** (operating since roughly
2021-2022): Sphinx packets, Loopix-style continuous mixing and cover
traffic, a 5-hop stratified topology, and — the piece with no academic
mixnet precedent — a token-incentivized relay economy paying mix-node
operators, plus **Coconut**-style compact anonymous credentials layered
on top for access control. The credential layer is directly relevant to
this repository's own `MN-401`-`MN-407` human-evidence work (D-0303) and
is named explicitly in §8 as future research, not adopted here.

**Outfox.** A more recent design (research literature, early 2020s)
targeting **reduced end-to-end latency relative to Loopix-style
continuous Poisson mixing**, while aiming to retain comparable traffic-
analysis resistance through an alternative packet/routing structure.
*Honesty note, per this document's own discipline*: the exact
publication venue, author list, and year are not independently re-verified
in this pass — cite and confirm the primary source before this becomes a
load-bearing citation in any external audit submission for `MN-205`. It
is listed here as the clearest signal that "Loopix-style delay is the
only way to buy this property" is an active, contested research question,
not settled fact — exactly the kind of claim `MN-205`'s external review
should re-examine against whatever the literature looks like at
implementation time, not what this document assumes today.

**Current academic direction (as of this writing, non-exhaustive).**
Post-quantum key encapsulation replacing Sphinx's ECDH-based per-hop
blinding (an active, unsettled research area — see §8); formal
universal-composability (UC) security treatments of mixnet packet
formats specifically (Sphinx's original proof is in a weaker, more ad
hoc model than a full UC treatment); mix networks combined with
anonymous credentials for Sybil-resistant relay/access control (Nym's
Coconut work is the production instance; this repository's own
`mini-uniqueness`/human-evidence track, D-0303, is the same problem from
a different angle); and decentralized, permissionless mix topologies
without a central directory authority — a genuinely open problem
directly relevant to this repository's no-central-server constitutional
stance, and one no production system listed above has actually solved
(Nym, Katzenpost, and their predecessors all rely on some form of
directory authority or bootstrap trust root for topology discovery).

## 3. Comparison matrix

Ratings are qualitative and sourced from each system's own published
design goals and threat-model documentation, not from independent
benchmarking this repository has performed — §4 names the simulation
work that would turn the bandwidth-overhead column into a measured
number instead of a declared range.

| Property | Tor | Loopix | Nym | Sphinx (packet format only) | Outfox | Mininet candidate (§7) |
|---|---|---|---|---|---|---|
| Latency | Low (optimized for interactivity, sub-second added) | High (Poisson delay per hop, seconds-to-minutes typical) | High (same Loopix-derived mixing) | N/A — format only, delay is a policy choice on top | Medium (explicit design goal: lower than Loopix) | High for Tier 2, matching `mini_privacy_policy::expected_cost`'s declared 1s-60s range (D-0094) |
| Adversary model | Explicitly *not* global-passive-adversary-resistant (own design docs state this) | Global passive adversary, bounded active adversary | Same as Loopix | Depends entirely on the delay/cover policy layered on top | Global passive adversary, tuned for lower-latency operating point | Global passive adversary is the explicit target (research doc's A4 class) |
| Active-attack resistance | Circuit-based; vulnerable to several documented active attacks (see §5) | Per-hop MAC + Poisson delay resists tagging/blending better than batch designs | Same as Loopix, plus economic cost via staking for relay Sybil resistance | Per-hop MAC closes tagging at the format level; says nothing about blending/flooding (that's a policy question) | Comparable to Loopix, exact figures need primary-source verification | Sphinx MAC + Loopix-style cover traffic + (§8) resource-cost-gated relay registration |
| Replay handling | N/A (circuit-based, not packet-replay-prone the same way) | Per-hop tag + seen-set with epoch expiry | Same as Loopix | Defines the tag mechanism; epoch/expiry policy is left to the deploying network | Comparable mechanism, needs primary-source verification | Adopts Sphinx's tag + a bounded, epoch-scoped seen-set (§7.5) |
| Packet overhead | Low (streaming circuit, no per-packet header inflation beyond cell format) | Fixed Sphinx header, independent of path length | Same (5-hop fixed topology) | Fixed-size header regardless of path length — the format's own headline property | Claims reduced per-hop overhead vs Sphinx, needs primary-source verification | Fixed-size Sphinx header; exact byte budget is implementation work, not fixed here |
| Bandwidth overhead (traffic multiplier vs. direct) | Low-to-medium (circuit multiplexing, no mandatory cover traffic) | High (independent loop cover traffic is unconditional, not traffic-adaptive) | High (same Loopix inheritance) | N/A — format only | Claims lower than Loopix; needs primary-source verification and independent measurement | `mini_privacy_policy::expected_cost(Mixed)`'s declared 5x-50x (D-0094) — **a declared estimate, not yet measured**, see §4 |
| Implementation complexity | High (mature, ~20 years of production hardening, large codebase) | Medium (reference implementation exists — Loopix's own research prototype) | High (production network, incentive layer, credential system) | Low-to-medium in isolation (one packet format); complexity moves to the policy layered on top | Unknown — no production-hardened implementation is known to this document's author | Medium, if scoped to Sphinx + Loopix-style policy only (§7); High if `MN-406`-style credentials are folded in prematurely (explicitly not recommended, §8) |
| Production maturity | Very high — largest deployed anonymity network, ~20 years | Research prototype; not itself a production network | Production, live public network since ~2021-2022 | N/A — a format Loopix/Katzenpost/Nym all deploy | Low — recent, primarily academic to this document's knowledge | None — this document is the specification, `MN-205` is unbuilt |
| Audit history | Extensive, multiple independent audits, longest track record of any system in this table | Academic peer review (USENIX Security), no known independent security audit beyond that | Some public audit activity around the Nym codebase/credential system; exact scope not independently verified here | Academic peer review (IEEE S&P) is Sphinx's own audit history | Academic peer review only, to this document's knowledge | **None** — `MN-205`'s implementation is explicitly gated on external review before any operational anonymity claim (D-0047-class gate, §0) |
| PQ readiness | Not PQ by default (ongoing experimental work exists in the broader Tor ecosystem, not evaluated here) | Not PQ (ECDH-based, as originally published) | Not PQ (inherits Sphinx's ECDH construction) | Not PQ as originally specified — the per-hop blinding is ECDH group-element math | Not evaluated here — no PQ variant known to this document's author | Not PQ in the §7 candidate; PQ KEM substitution is named explicitly in §8 as future work, not deferred silently |

## 4. Simulations this repository still owes

Every quantitative claim in §3's "Mininet candidate" column and in
`mini_privacy_policy::expected_cost`'s Tier 2/3 multipliers is a
**declared estimate from the source research, not a measurement** (D-0094
says this explicitly of the whole crate). The following simulations
would close that gap, roughly in the order they'd need to run before
`MN-205` implementation begins in earnest:

- **Malicious node percentage sweeps** — vary the fraction of mix nodes
  under a single adversary's control and measure de-anonymization
  probability as a function of path length and topology width. The
  baseline question every reviewer will ask first.
- **Correlated cloud-provider colocation** — model an adversary who
  doesn't need to compromise nodes directly if enough of them share a
  hosting provider (AWS/GCP/Azure/etc.) that provider-level traffic
  visibility gives equivalent power. Directly informs whether operator-
  diversity constraints on relay selection (§7.6) are sufficient or
  need provider-level, not just legal-entity-level, diversity.
- **AS-level (Autonomous System) adversary** — a network-layer observer
  who doesn't control mix nodes at all but sits on enough internet
  routing paths to correlate entry/exit traffic by timing alone. This is
  a *different* adversary class than node compromise and needs its own
  simulation.
- **Jurisdiction diversity** — model legal-compulsion risk (the F5 floor
  from the cost-doctrine research) as a function of how many mix-path
  hops share a mutual-legal-assistance treaty relationship or single
  jurisdiction's subpoena reach.
- **Relay churn** — mix nodes joining/leaving over realistic operator
  behavior (not just adversarial exit) and its effect on topology
  freshness, path-selection staleness, and whether a churning relay set
  creates a *new* correlation signal (a client re-selecting paths more
  often than the topology actually changes is itself a distinguishing
  behavior).
- **Mobile clients** — intermittent connectivity, NAT traversal, and
  battery-constrained duty cycling change both the cover-traffic
  generation model (a mobile client can't sustain Loopix's unconditional
  loop-traffic rate indefinitely) and the anonymity-set math (a client
  who is only sometimes reachable is more distinguishable than an
  always-on desktop peer).
- **Sparse traffic** — the low-usage regime the research doc's
  anonymity-set corollary names directly (§1.4: "doubling your own cover
  traffic while alone doubles your bill and buys nothing"). Needs
  simulation to find the actual crowd-size threshold below which Tier 2
  privacy purchases functionally nothing, informing the subsidy-pooling
  policy `MN-604` will eventually need.
- **Heavy traffic** — the opposite regime: queueing behavior, whether
  Poisson per-hop delay holds its statistical guarantees under sustained
  high load, and whether congestion itself becomes an observable side
  channel (ties directly to the congestion attack in §5).
- **Intersection attacks** — repeated-session correlation: the same two
  parties communicating across many independent observation windows,
  even with per-session mixing working perfectly. This is the empirical
  counterpart to the statistical-disclosure attack in §5 and the F3
  floor in the cost-doctrine research — simulation would quantify how
  many independent sessions an adversary needs before correlation
  succeeds at a given confidence, as a function of traffic volume and
  relationship reuse frequency.
- **Cover traffic cost** — the direct bandwidth/money cost of sustaining
  Loopix-style unconditional loop traffic at various client population
  sizes, feeding `mini_resource_pricing::quote()` with a measured
  multiplier instead of the declared 5x-50x range.
- **Battery impact** — mobile-specific: the energy cost of maintaining
  cover-traffic generation and per-hop cryptographic operations on a
  representative low-power device, informing whether Tier 2/3 should
  ever be a mobile-client default versus an explicit opt-in.
- **Bandwidth cost** — the aggregate network-wide bandwidth budget
  required to sustain a given anonymity-set size at a given traffic
  volume, the input every subsidy/pricing decision (`MN-601`-`MN-605`)
  ultimately depends on.

None of these simulations are performed in this document. Naming them is
the deliverable — an external reviewer should be able to see exactly
what evidence is missing rather than have to ask.

## 5. Attack catalog

Each entry: description, affected systems, mitigation this design
proposes, and residual risk after that mitigation — because a reviewer
should never have to ask "and what happens when the mitigation itself is
imperfect."

**Tagging attack.** An adversary modifies bits of a packet at one hop,
hoping to recognize the corrupted pattern (or the failure it causes) at
another hop, correlating the two observation points. *Affected*: naive
onion-routing designs without per-hop integrity checking. *Mitigation*:
Sphinx's per-hop MAC, computed from each hop's derived shared secret —
any modification causes MAC verification failure and the packet is
silently dropped, never forwarded. *Residual risk*: an adversary
controlling *both* the entry and exit of a path doesn't need tagging at
all — timing/volume correlation (below) still applies regardless of MAC
integrity.

**Replay attack.** An adversary captures a packet and resubmits it later,
observing where the duplicate exits to learn something about the
original's path. *Affected*: any mixnet without an explicit replay
defense. *Mitigation*: Sphinx's per-hop tag, checked against a seen-set
scoped to an epoch window (§7.5); a replayed packet's tag is already
present and the packet is dropped. *Residual risk*: the seen-set's size
scales with epoch length — too long an epoch grows unbounded state, too
short a rotation and an adversary can replay just after rotation to
reset the window; a malicious mix node could also selectively drop-and-
replay-later to manufacture timing signal, which the tag mechanism alone
does not close.

**Predecessor attack.** Repeated observation, over many independent
rounds, of which node preceded a given stream on its path statistically
identifies the true origin — the set of "who could this session's
predecessor be" narrows every round even without breaking any single
round's mixing. *Affected*: any low-latency, session-persistent design;
most documented against Tor's circuit model specifically. *Mitigation*:
per-packet (not per-session) mixing means there is no persistent
"circuit" whose predecessor accumulates evidence round over round the
same way; path re-selection per message, not per session, blunts this
specific accumulation. *Residual risk*: still degrades with enough
independent messages between the same two parties over long enough time
— this is the *same underlying phenomenon* as the intersection attack
below, viewed from the routing side rather than the traffic-analysis
side.

**Blending / n-1 attack.** An adversary floods a mix's batch with n-1
messages of its own, so a single honest message becomes trivially
identifiable by elimination once outputs are observed. *Affected*:
threshold/discrete-batch mixers (the original Chaum design's weak point).
*Mitigation*: continuous-time Poisson mixing makes "the batch" an
ill-defined, unobservable concept — there is no discrete batch boundary
for an adversary to fill; independent cover traffic further dilutes the
adversary's ability to isolate a target packet even probabilistically.
*Residual risk*: the attack doesn't disappear, it becomes *expensive* —
an adversary with enough bandwidth to flood a large fraction of a mix
node's total observed traffic over a sustained window can still degrade
the achieved anonymity set proportionally. This is the cost doctrine's
core move applied to a specific attack: cost is raised, not eliminated,
and the exact cost curve is one of the un-run simulations in §4.

**Route capture.** An adversary arranges (via node operation, BGP
hijack, or biased path-selection exploitation) to occupy every hop on a
specific target's path, achieving full visibility without needing to
correlate anything across independent nodes. *Affected*: any relay
network with insufficient path-diversity enforcement. *Mitigation*: path
selection constrained by operator/ASN/jurisdiction diversity — the same
placement invariant this workspace already applies to erasure-coded
storage shards ("never enough shards under one operator/ASN/jurisdiction
to reconstruct," `docs/STATUS.md` §7) extends directly to mix-hop
selection. *Residual risk*: a sufficiently well-resourced state-level
adversary can still achieve high relay-diversity coverage within a
single region or under a single legal-compulsion regime — diversity
constraints raise the *number* of independent entities an adversary must
coerce or compromise, they do not create an entity that can't exist.

**Selective DoS.** An adversary drops or artificially delays traffic
through relays it doesn't control, "starving" honest paths and biasing
clients toward reselecting paths that happen to run through
adversary-controlled relays more often — a lever for traffic analysis
disguised as an availability problem, well documented in the Tor
literature. *Affected*: any relay-selection scheme that reacts to
observed relay performance. *Mitigation*: path-selection policy that
doesn't over-index on recent-performance signals an adversary can
manufacture; redundant/parallel path attempts rather than reactive
single-path rebuilding. *Residual risk*: raises cost and latency for the
defender without fully closing the incentive gradient — an adversary
willing to sustain the DoS still gains *some* bias, just less than an
unmitigated design.

**Timing correlation.** The floor. A global passive observer correlating
packet timing and volume at entry and exit, given a long enough and
distinctive enough session. *Affected*: every system in §3's comparison,
without exception — this is the research doc's own F2 floor
("intersection/timing attacks are floors, not failures — cost pushes the
required observation window and volume up; it does not make it
infinite"). *Mitigation*: Poisson per-hop delay plus independent cover
traffic raise the required observation window and traffic volume for a
given confidence level. *Residual risk*: unremovable by construction —
see D-0094's `ResidualFloor::GlobalObserverLongSessionCorrelation`. No
mitigation in this document, or in any system in §3, closes this floor;
all of them only push it further out.

**Congestion attack.** An adversary deliberately induces congestion at a
target mix node (e.g. by flooding it with legitimate-looking traffic)
and observes resulting timing shifts in the target's real traffic as it
passes through — a form of active timing attack distinct from passive
observation, documented against Tor's circuit-queueing behavior.
*Affected*: any system with observable queueing delay that varies with
load. *Mitigation*: constant-rate or Poisson-paced forwarding (not
FIFO-with-load-dependent delay), fixed-size padding per hop so packet
size itself carries no congestion signal. *Residual risk*: sophisticated
congestion fingerprints may still leak through padding at sufficient
adversary resource levels — not independently simulated by this
document (§4).

**Guard compromise.** For designs with a persistent entry point (Tor's
guard nodes), compromising a client's fixed guard gives an adversary a
stable, long-term observation point requiring no further work per
session. *Affected*: guard-based designs specifically. *Mitigation*:
this candidate's stratified, per-message-not-per-session topology
(Loopix-style, §7.6) has no persistent guard-node concept by design —
but a "provider" node (the client's fixed entry into the mix topology
for cover-traffic and message-submission purposes) plays an analogous
role and needs its own rotation/diversity policy, not assumed away.
*Residual risk*: provider-node compromise remains a real, not fully
mitigated risk; the exact rotation policy is unspecified in this
document and is `MN-205` implementation work.

**Flooding.** Raw resource exhaustion against a mix node — distinct from
blending (above), which uses flooding *as a correlation technique* rather
than a pure availability attack. *Affected*: any node accepting traffic
without a resource cost. *Mitigation*: bandwidth/packet submission
priced via `mini_resource_pricing::quote()` (D-0302) rather than free —
raising an adversary's cost to flood proportionally to the flood's size,
consistent with the broader cost doctrine. *Residual risk*: pricing
raises the cost, it does not create a hard ceiling; a well-funded
adversary can still pay to flood, same as every other resource-cost
mitigation in this repository.

**Statistical disclosure attack** (Danezis 2003 and the substantial
follow-on literature). A long-term passive observer, without any
flooding or active attack at all, statistically infers sender/receiver
relationship pairs by correlating *patterns* of activity across many
independent observation rounds — who tends to be active when a given
recipient receives traffic. *Affected*: any mix system where sender/
receiver relationships repeat non-uniformly over time. *Mitigation*:
cover traffic dilutes the signal per round; limiting how often the same
relationship repeats (an application-layer, not protocol-layer,
mitigation) reduces the observer's accumulated evidence. *Residual
risk*: this is, precisely, the research doc's **F3 floor**
("intersection over time") — a fundamentally statistical limit, not an
engineering gap; cost raises the required number of independent
observation rounds, it does not remove the floor.

**Disclosure by long-term intersection.** Listed separately from
statistical disclosure above because the literature treats "who talks to
whom across many independent windows" as a distinct generalized problem
class beyond Danezis's specific statistical model — worth naming
explicitly so a reviewer doesn't read this catalog as claiming the
narrower attack is the only instance of the broader floor. *Residual
risk*: same F3 floor.

**Sybil relay concentration.** An adversary registers many mix-node
identities (cheap, if relay registration has no resource cost) to
increase the probability of occupying multiple hops on a target's path
purely through numbers. *Affected*: any permissionless relay network
without an identity or resource cost gating registration. *Mitigation*:
this is, precisely, the personhood/Sybil problem this entire repository
already names as unsolved (`mini-uniqueness`, the F4 floor) — a
resource-cost-gated relay registration (proof-of-space-time-style, per
`mini-spacetime`/`mini-porep`, or a MINI-denominated stake that creates
**no governance weight**, per the voice/value wall) raises the cost of
acquiring many relay identities without claiming to solve Sybil
resistance generally. *Residual risk*: identical to the F4 floor stated
everywhere else in this workspace — no protocol-level mechanism inside
the mixnet itself closes this; it depends on whatever this repository's
broader personhood work eventually achieves, and no more.

## 6. Why Mininet does not simply use Tor

This question will come up in every review, so it is answered here
directly rather than left implicit.

Tor's own design documentation is explicit that it optimizes for
low-latency, interactive traffic — web browsing, not metadata-resistant
messaging — and that this is a deliberate trade-off, not an oversight:
Tor's threat model explicitly does not claim resistance to a global
passive adversary correlating entry and exit traffic by timing, because
defending against that adversary requires exactly the batching/delay/
cover-traffic machinery Tor omits to keep latency low. Tor's own FAQ and
design papers say this plainly.

Mininet's Tier 2 (Mixed/High-risk) is priced, in `mini-privacy-policy`,
specifically for the adversary class Tor's own documentation excludes —
the research doc's A4, "global passive observer," where "the floor lives
here." Buying resistance to that adversary requires the mixing machinery
this document specifies; Tor's circuit-based, always-fast design was
never intended to provide it, and layering the two designs together in
that role would be a category error, not an enhancement.

**They are not competitors solving the same problem at different price
points — they solve different problems.** Tor's operating point sits
closer to Mininet's own Tier 1 (Relayed/Private) in spirit: entry/exit
separation at acceptable latency, no batching or mandatory cover traffic.
Tier 2 needs something structurally absent from Tor's design.

There is also an architectural distinction worth naming precisely: Tor
is **circuit-based** — a persistent virtual connection multiplexes many
messages/cells over one established path — while Sphinx/Loopix-style
mixnets are **per-packet/datagram-based**: mixing decisions (delay, path)
are made fresh per packet, with no persistent circuit to correlate
against session-over-session (directly relevant to the predecessor
attack in §5). This is not merely "Tor but slower"; it is a different
operational model end to end.

None of this makes Tor irrelevant to Mininet. The source research names
Tor/I2P explicitly as "compatibility bearers and bootstrap, not the whole
architecture" (§1.5), and `MN-207` ("bridge and pluggable transport
interface," a later lane) is exactly where Tor could plug in — as a
Tier 1 bridge option, or as an additional circumvention hop in regions
where direct Mininet traffic is blocked, layered *beneath* Mininet
transport rather than substituting for Tier 2's own mixing.

## 7. Candidate protocol specification for `MN-205`

This section is the concrete deliverable "protocol specification" named
in `MN-204`'s own scope. It is a candidate, not a final design — `MN-205`
implementation and its own external review are expected to refine or
revise details here, and this document does not authorize skipping that
review (§0).

**7.1 Packet format.** A Sphinx-style fixed-size packet: a recursively
layered, fixed-length header (recursion depth bounded by a maximum path
length constant, independent of the *actual* path length used for a
given message — every packet the same size regardless of hop count) plus
a fixed-size payload, sealed per-hop with AEAD. No new AEAD is proposed;
`mini_crypto::AeadKey`/`AeadSuite::ChaCha20Poly1305` (already exported,
already used throughout this workspace) is the candidate.

**7.2 Per-hop key agreement.** One ephemeral X25519 key pair per packet,
already available via `mini_crypto::{AgreementSecretKey, AgreementPublicKey,
SharedSecret}`. Following Sphinx's own construction: the header carries a
single group element, re-randomized (blinded) at each hop by that hop's
own contribution, so the header's size never grows with path length.
Each hop derives its own shared secret via X25519 agreement against the
current group element, then derives per-hop AEAD keys, MAC keys, and the
next hop's blinding factor from that shared secret via
`mini_crypto::KdfSuite::HkdfSha256` (already exported) with a fixed,
versioned domain-separation string
(`mininet/mixnet/sphinx-header/v1`-shaped, matching this workspace's
existing HKDF domain-separation convention, e.g.
`did-mini::Controller::incept_pairwise_pseudonym`'s
`PAIRWISE_PSEUDONYM_SALT` pattern).

**7.3 Padding.** Payload padded to a fixed size class before sealing.
This maps directly onto `mini_transport_policy::PayloadSizeClass`
(already shipped, D-0301): `Small` fits one packet; `Medium`/`Large`
require chunking into multiple fixed-size packets rather than one padded
packet, exactly as that crate's own doc comment already anticipates
("Tier 2+ mix networks pad to fixed frame sizes — a large payload is many
frames, not one padded frame").

**7.4 Delay.** Loopix-style: independent, per-hop exponential (Poisson)
delay. The rate parameter should be chosen so the *aggregate* end-to-end
delay distribution falls inside `mini_privacy_policy::expected_cost
(PrivacyTier::Mixed)`'s already-declared `added_latency_ms_min`/`_max`
range (1,000ms-60,000ms, D-0094) — this specification does not itself
fix the rate constant; that is calibration work informed by §4's
unrun simulations, not a number to freeze here.

**7.5 Replay defense.** Sphinx's per-hop tag (derived alongside the
per-hop keys in §7.2), checked against a bounded seen-set scoped to a
rotating epoch window. A tag already present in the current epoch's
seen-set means the packet is dropped, silently, before any further
processing — matching this workspace's "reject before allocating/
processing further" discipline used throughout its existing wire codecs
(`mini-net::pex`, `mini-consensus::catchup`, `mini-objects::envelope_v2`).
Epoch length is a tuning parameter trading seen-set memory against the
replay-attack residual risk named in §5.

**7.6 Topology.** Stratified/layered (Loopix-style): a fixed number of
mix layers, each with a bounded set of member nodes, rather than free
routing through an arbitrary relay graph. Recommended for the initial
`MN-205` implementation because it bounds path-selection complexity and
keeps anonymity-set analysis tractable for the simulations in §4 — a
free-route (Tor-style) alternative is not ruled out, but should be
evaluated by simulation before adoption, not assumed superior by default.

**7.7 Cover traffic.** Independent, self-addressed loop packets
(Loopix-style), generated by both clients and mix nodes at a rate
independent of real traffic volume — directly informed by
`mini_resource_pricing::quote()` (D-0302), which already computes a
declared price for `PrivacyTier::Mixed` bandwidth; the cover-traffic rate
this specification eventually fixes should be the rate that quote is
actually pricing, closing the loop between L3's protocol design and L4's
pricing model rather than leaving them independently guessed.

**7.8 Path selection and diversity.** Path selection must enforce
operator/ASN/jurisdiction diversity constraints across the hops of a
single path — the same placement invariant `mini-erasure`'s shard
placement already applies ("never enough shards under one operator/
ASN/jurisdiction," `docs/STATUS.md` §7) — directly mitigating the route-
capture attack (§5). The exact diversity policy (how many independent
dimensions, what counts as "independent") is `MN-205` implementation
work.

**7.9 Relay Sybil resistance.** Relay-node registration should be
resource-cost-gated (§5's Sybil relay concentration mitigation) — a
candidate mechanism is reusing `mini-spacetime`/`mini-porep`'s existing
proof-of-space-time machinery, or a MINI stake that creates no governance
weight whatsoever (the voice/value wall applies here exactly as
everywhere else in this workspace). This is explicitly not solved by
this document; it is named so `MN-205` does not silently ship a
permissionless-and-unprotected relay registry.

**7.10 Integration points, already shipped.** This specification
consumes, without modifying: `mini_privacy_policy::{PrivacyTier,
Mechanism::MixNetwork, Mechanism::CoverTraffic, Mechanism::TrafficPadding,
Mechanism::BoundedRandomDelay, expected_cost}` (D-0094); `mini_transport_
policy::{PayloadSizeClass, mechanisms_for_tier}` (D-0301), whose
`PrivacyTier::Mixed` arm already lists exactly the four mechanisms this
specification implements and no others — this document's job was to
figure out what building those four already-named mechanisms actually
requires, not to invent new ones. `mini_resource_pricing::quote` (D-0302)
prices the bandwidth this specification's cover-traffic and padding
policy consumes.

## 8. Future research beyond MN-204

Named explicitly so omissions read as decisions, not oversights.

- **Adaptive path lengths** — trading anonymity strength for latency
  dynamically per privacy tier or per message priority, rather than one
  fixed path length for all Tier 2 traffic.
- **Outfox migration path** — if `MN-205` initially implements a
  Loopix-style design and Outfox (or a successor) proves superior after
  independent review and the primary-source verification this document
  deferred (§2), a migration path should exist without a full protocol
  redesign.
- **Post-quantum KEMs** — replacing Sphinx's ECDH-based per-hop blinding
  (§7.2) with a post-quantum key encapsulation mechanism is active,
  unsettled research (§2's "current academic direction"); not adopted
  here because no PQ Sphinx variant has the production deployment history
  this document's composition-of-prior-art standard requires (§0).
- **PIR mailboxes** — private information retrieval for the MiniMail
  store-and-forward design the source research names (§7 of
  `MININET_RESEARCH_V2_20260713.md`), so polling a mailbox doesn't itself
  reveal which mailbox was polled — a distinct problem from mixnet
  transport, worth its own lane.
- **Decoy routing** — a censorship-circumvention technique distinct from
  the cover *traffic* this specification already includes (§7.7); decoy
  routing hides the existence of a connection to the mix network itself
  from a censoring network operator, a different threat model than
  traffic-analysis resistance.
- **Differential-privacy metrics for anonymity-set quantification** — a
  more rigorous way to state "how anonymous" a given traffic pattern
  actually is than the qualitative language this document uses.
- **Formal verification** of the packet-processing state machine
  `MN-205` eventually ships — proving the implementation matches this
  specification's intent, not just testing it.
- **UC-security proofs** for whatever concrete parameter choices this
  workspace settles on — Sphinx's original proof is not in the strongest
  available security model (§2); a workspace-specific instantiation
  deserves its own analysis, not an inherited claim.
- **Anonymous credential integration** — Nym's Coconut-style credentials
  (§2) as prior art for combining mixnet access control with this
  repository's own `MN-401`-`MN-407` human-evidence work; explicitly not
  folded into `MN-205` itself (§3's complexity note: "High if MN-406-
  style credentials are folded in prematurely").
- **Congestion-aware adaptive routing** — routing decisions that respond
  to real-time congestion signals without themselves becoming a new
  observable side channel (tension with the congestion attack in §5).
- **Relay reputation without deanonymization** — a genuinely hard open
  problem: rate-limiting or reputation-scoring a relay's behavior over
  time without creating a linkable identity that erodes the same
  unlinkability properties §7 exists to provide. No system surveyed in
  §2 or §3 has fully solved this.

## References

Primary sources cited by name and venue where confidently known; see
§2's honesty note regarding Outfox's exact citation, which should be
independently confirmed before any external audit submission relies on
it.

- D. Chaum, "Untraceable Electronic Mail, Return Addresses, and Digital
  Pseudonyms," *Communications of the ACM*, 1981.
- D. Kesdogan, J. Egner, R. Büschkes, "Stop-and-Go MIXes: Providing
  Probabilistic Anonymity in an Open System," *Information Hiding*, 1998.
- G. Danezis, R. Dingledine, N. Mathewson, "Mixminion: Design of a Type
  III Anonymous Remailer Protocol," *IEEE S&P*, 2003.
- G. Danezis, "Statistical Disclosure Attacks," *IFIP SEC*, 2003.
- G. Danezis, I. Goldberg, "Sphinx: A Compact and Provably Secure Mix
  Format," *IEEE S&P*, 2009.
- A. Piotrowska, J. Hayes, T. Elahi, S. Meiser, G. Danezis, "The Loopix
  Anonymity System," *USENIX Security*, 2017.
- Katzenpost project documentation (reference Loopix-style implementation).
- Nym Technologies, Nym network and Coconut credential system
  documentation.
- Outfox — recent literature targeting reduced mixnet latency; primary
  source not independently re-verified in this pass (§2).
- Tor Project design documentation and FAQ, on Tor's own stated threat
  model and its explicit exclusion of global-passive-adversary
  resistance.
- This repository: `docs/research/MININET_RESEARCH_V2_20260713.md`
  (the cost-doctrine research this whole track implements); D-0094
  (`mini-privacy-policy`); D-0301 (`mini-transport-policy`); D-0302
  (`mini-resource-pricing`); `docs/STATUS.md` §7 (erasure shard
  placement diversity, the direct precedent for §7.8's mix-path
  diversity policy).
