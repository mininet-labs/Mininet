# Personhood/Sybil-resistance: the vouch-quota growth ceiling, and five proposed extensions to the Gate #21 anonymous-vouching architecture

Research target

Repository: mininet-labs/Mininet
Issue: [#21](../../issues/21) — Private human-continuity proof research
Related: `docs/design/human-continuity-proof.md` (D-0075), `docs/gates/
personhood-signal-b-decision.md`, `docs/audits/source-reports-2026-09-12/
Mininet_External_Personhood_Audit_04_Gate_21_Sybil_Human_Continuity_FINAL.txt`
(the "Gate #21 audit" below)
Research date: 12 September 2026

## Status of what this builds on

The Gate #21 audit is, like the Gate #96 legal review it arrived alongside,
written as an "external-auditor adoption form" — a finished report a
qualified external reviewer can sign — with its attestation block blank.
It is **not yet adopted**, and this document does not change that. What it
does do is take the audit's own architecture (canonical `PersonhoodPolicyV1`,
Semaphore-style anonymous vouching, no raw social graph, capped external
credentials, an explicit named bootstrap trust assumption) as the current
best design, and contribute independent analysis and five concrete proposed
extensions on top of it — the "propose alternatives" this document was asked
to do. None of these five are proposed as replacements for the audit's
architecture; each is either a quantitative concern about a parameter the
audit leaves for simulation, or an additional research-track item in the
spirit of the design doc's already-open Tracks A–F. One candidate
alternative is explicitly considered and rejected, stated plainly as such
rather than omitted.

Nothing here invents new cryptography. Every proposal composes an already
published, peer-reviewed construction (Semaphore-style nullifiers — already
adopted by the audit itself, verifiable delay functions, cryptographic
accumulators) or is pure parameter/mechanism-design reasoning over the
audit's own stated formulas — consistent with this project's rule against
inventing unreviewed cryptographic primitives.

## Contribution 1 — the vouch-quota ceiling that bounds attacker growth bounds honest growth by the same formula

This is the most load-bearing finding in this document, and it falls out
directly from arithmetic the audit itself supplies, so it is worth stating
precisely rather than qualitatively.

The audit's Section 1, point 7 derives:

```
attacker-created MatureHumans/year <= floor(A * 12 / 24) = floor(A / 2)
```

where `A` is the number of already-mature identities under attacker control,
12 is the number of one-per-epoch vouch slots a MatureHuman gets in a year
(30-day epochs), and 24 is the number of vouches a new candidate needs.

That derivation never actually uses an assumption that the vouchers are
malicious. **The same arithmetic bounds the whole network's promotion rate**,
attacker and honest growth alike, because a vouch slot is a fungible
resource: a mature human's one vouch per epoch is spent on *some* candidate,
Sybil or genuine, and the network-wide production of new MatureHumans in any
given year is bounded by:

```
new MatureHumans/year <= floor(TotalMatureHumans * 12 / 24) = floor(TotalMatureHumans / 2)
```

*at best* — this is the rate only if every mature human spends every one of
their twelve annual vouch slots specifically on onboarding a brand-new
candidate (rather than, say, vouching for a friend who already has other
vouchers, or not using a slot at all in a given epoch), and only if the
network manages to coordinate the "≥6 vouches in one shared epoch"
requirement efficiently for every candidate. Real honest growth will be
slower than this ceiling, not faster.

Applying this to the audit's own Section 16 numbers: 64 BootstrapHumans,
growing at the theoretical-maximum 50%/year (`A_{n+1} = A_n + floor(A_n/2)`),
reach the audit's own required mainnet-exit threshold of 2,048 active
MatureHumans only after:

```
64 * 1.5^n >= 2048  =>  n >= log(32) / log(1.5) ≈ 8.55
```

— a **best-case minimum of roughly nine years**, assuming perfect
coordination and zero vouching "waste" on existing relationships, before
this design's own stated mainnet-exit gate can be satisfied. This is not a
security flaw — the ceiling is precisely what makes the attacker's growth
just as slow, and that is the point of the design — but it is an adoption-
timeline consequence the audit does not itself quantify, and it deserves to
be named explicitly and checked against the project's own century-scale
patience (Directive 13) rather than discovered as a surprise once real
bootstrap is underway.

**Proposed mitigation, offered as a parameter for the audit's own Phase 5
adversarial simulation, not asserted as a settled number:** a time-decaying
"bootstrap dividend" on the per-epoch vouch quota — e.g., 3 vouch-slots per
epoch instead of 1 while `TotalMatureHumans < N_low` (some early-network
threshold), linearly or geometrically stepping back down to the steady-state
1-per-epoch rate as the population crosses one or more higher thresholds.
This directly speeds up *both* honest and attacker growth in absolute terms,
but the ratio `attacker-controlled / total` at the finish line is what
matters for security, and that ratio is set by the bootstrap cohort's own
diversity requirements (Section 16.2), not by the quota multiplier — so a
higher early quota does not by itself weaken the attacker-growth bound
relative to genuine growth; it only changes how long the whole network,
honest and adversarial members alike, takes to reach a given size. This
needs to be verified by the same adversarial simulation the audit's own
Phase 5 and this design's Section 21 already call for — it is a testable
mechanism-design hypothesis, not a claim I can verify without that
simulation harness existing.

## Contribution 2 — epoch-boundary compression on the "spread across distinct epochs" requirement

The audit's promotion rule requires vouches "spanning >=10 distinct 30-day
epochs" for time diversity. If epochs are discrete calendar buckets (epoch N
= days 1–30, epoch N+1 = days 31–60, etc.), a vouch issued at 23:59 on the
last day of epoch N and a second vouch issued at 00:01 on the first day of
epoch N+1 satisfy "two distinct epochs" while being separated by two minutes
of real elapsed time. An attacker (or an impatient honest cohort trying to
speed up a friend's promotion) that times vouches to straddle every epoch
boundary could satisfy the letter of "10 distinct epochs" while compressing
the actual elapsed-time diversity the rule is meant to enforce.

**Proposed fix:** parameterize the nullifier scope by a rolling window
(e.g., "no two counted vouches for the same voucher closer than 25 days
apart, and the 10-distinct-window requirement measured as any 10
non-overlapping 25-day windows") rather than a shared discrete calendar
epoch. This keeps exactly the same cryptographic machinery the audit
already approves — Semaphore-style one-signal-per-scope nullifiers — and
only changes what defines a "scope": a sliding real-time window per voucher
rather than a globally shared calendar bucket. It closes the boundary-
compression gap without adding any new primitive.

## Contribution 3 — verifiable delay functions as a harder anchor for the elapsed-time requirements (Research Track F candidate)

The "≥365 days since first accepted evidence" and "fresh evidence in the
preceding 45 days" requirements presumably anchor to consensus block
timestamps. Most BFT designs (including the bounded-timestamp-drift model
this repository's own `mini-consensus` likely assumes) tolerate some small
timestamp manipulation tolerance per block; a well-resourced attacker with
enough validator influence to nudge timestamps at the margin, applied
consistently over months, could shave real elapsed time off the maturation
clock in a way that is individually within each block's tolerance but adds
up over a 365-day requirement.

**Proposed hardening, offered as a Research Track F item:** anchor the
epoch-advance signal to a **verifiable delay function** (VDF) — a
peer-reviewed, already-deployed primitive (Wesolowski 2018; Pietrzak 2018;
in production use by Chia and studied extensively by the Ethereum Foundation's
VDF Alliance) that provably requires real sequential wall-clock computation
to evaluate, with a proof that is fast to verify. Using a VDF output (rather
than only a block timestamp) to gate epoch advancement for personhood
maturation purposes would make "time elapsed" cryptographically enforced
rather than solely consensus-timestamp-enforced, closing the margin the
attack above depends on. This is squarely a benchmarking/feasibility
question for Track F (weak-device proving), since VDF *verification* is
cheap but the network still needs one canonical VDF evaluator (or a
few, redundantly) — a role that must not become a new indispensable
authority, so the research question is specifically whether a VDF chain can
be maintained as commons infrastructure (e.g., contributed redundantly by
multiple independent operators, verified by everyone) rather than a single
operator's output being trusted.

## Contribution 4 — cryptographic accumulators as a complementary MatureHumanSet membership structure at scale (Research Track F candidate)

The audit approves Semaphore's Merkle-tree group membership for the
MatureHumanSet. Merkle inclusion proofs are well understood and already
peer-reviewed for this purpose, but proof size and the prover's tree-path
maintenance cost grow with `log(n)`, and a member's cached path must be
refreshed whenever the tree root changes (i.e., whenever anyone joins or
leaves) — a real cost on a weak device once the set is not thousands but
millions of members, which is this project's own century-scale, planet-scale
ambition.

**Proposed research candidate:** benchmark peer-reviewed dynamic
cryptographic accumulators (RSA or class-group accumulators per Boneh–
Bünz–Fisch's "Batching Techniques for Accumulators," 2019) as a complementary
or eventual-successor membership structure, specifically for the
weak-device-proving goal Track F already names. This is offered honestly as
a tradeoff to benchmark, not a proven improvement: dynamic accumulators
typically need either a semi-trusted witness-updater service or a bounded
self-update cost, and introducing a witness-updater role would itself need
the same "never an indispensable authority" scrutiny this project applies
everywhere else. The right outcome of this research item may well be "stay
with Semaphore's Merkle tree, the tradeoffs don't favor accumulators here" —
that is a valid and useful result, in the same spirit Track B already
invites for sensor provenance.

## Contribution 5 — a concrete mechanism sketch for Research Track E (coercion / vouch-rental markets)

The design doc's Track E ("coercion/puppeteering modeling") and the audit's
own residual-risk list both name purchased/coerced honest vouching as a
risk cryptography cannot directly prevent — a real person who sells or is
coerced into spending their vouch on an attacker's candidate produces a
cryptographically valid vouch. Both documents correctly leave this as named,
unsolved residual risk rather than pretending a proof system closes it. This
document does not claim to close it either — but offers one concrete
mechanism worth formal modeling rather than leaving Track E fully open-ended.

**Sketch — a "regret nullifier" bound to the original vouch's own
nullifier:** allow a voucher who was coerced or paid, and later wants to
retract without exposing themselves to retaliation, to submit — at any later
time, anonymously — a second zero-knowledge proof that cryptographically
binds to the *specific* vouch's nullifier they issued (proving "I am the
person who issued this exact vouch" without revealing which epoch's group
membership they used to do it) and that flags the *specific* candidate that
vouch helped promote for a scoring penalty or renewed evidentiary scrutiny.
Binding the regret proof to the original vouch's own nullifier is essential
and is the entire point of the sketch: it prevents a griefing attacker from
filing regret proofs against arbitrary targets, since only the actual issuer
of a given vouch can produce a valid regret proof for it.

This does not, and cannot, identify the voucher, the buyer, or reverse the
original promotion outright — doing so would violate the no-de-personing
principle the audit itself correctly enforces (Section 19 / point 8: human
status cannot be stripped by anyone's say-so). What it can do is make a
purchased-vouch market riskier for the *buyer*: a rational buyer must now
discount every purchased vouch by the probability the seller later regrets
it, which a purely cryptographic threshold cannot otherwise price in at all.
This needs real game-theoretic modeling — how much scoring penalty per
regret nullifier, whether regret should be individually actionable or only
statistically meaningful in aggregate across many vouches, and whether the
existence of a "regret" mechanism itself creates a new coercion vector
(an attacker forcing a victim to vouch, then separately forcing them *not*
to regret it) — before it is anything more than a research candidate. It is
offered here as exactly that: a starting point for Track E, not a proposed
launch feature.

## Explicitly rejected alternative — reviving graph-correlation heuristics for "one person, many lives" detection

The most obvious-looking "fix" to the audit's honestly-named residual risk
of one real person holding multiple legitimate mature identities (Section 1:
"several citizenships/devices/accounts/households can't be proven to be one
biological person without an authoritative global biometric system, which
this design explicitly does not build") is some form of private
set-intersection over voucher graphs — flag two MatureHumans whose vouching
histories overlap suspiciously, using PSI or similar so no one party learns
the full graph.

I considered this and reject it, and want to say so explicitly rather than
silently omit it, since it is exactly the kind of appealing-sounding
proposal that deserves to be named and refused on the record rather than
quietly avoided. The audit's Section 15 removes the raw social graph
specifically because *any* graph-shaped analysis — even privacy-preserving
PSI — requires the network (or some computing party on its behalf) to hold
or process relationship structure that Directive 9 says it should never
learn in the first place. Reviving graph correlation to catch a residual
risk the audit already honestly discloses as out of scope for cryptography
alone would trade a named, bounded residual risk for a new, structural
privacy regression. The correct response to "cryptography can't stop one
person holding two legitimate identities" is the audit's own answer: name
it, bound its value (the human-share safety envelope and slow vesting
already limit what one extra identity is worth), and refuse to build
surveillance infrastructure to chase a residual that composing more
correlation analysis would only partially address anyway (a determined
person using genuinely unrelated evidence classes for each identity leaves
no correlation to find).

## Summary

| # | Proposal | Status |
|---|---|---|
| 1 | Vouch-quota ceiling is symmetric; ~9-year best-case minimum to reach the 2,048-MatureHuman mainnet-exit threshold from a 64-person bootstrap; propose a time-decaying bootstrap quota dividend | Quantified concern + testable mitigation, for Phase 5 simulation |
| 2 | Epoch-boundary compression on the "10 distinct epochs" rule | Concrete fix using the same nullifier machinery already approved |
| 3 | VDF-anchored time gating against timestamp-margin manipulation | Research Track F candidate |
| 4 | Cryptographic accumulators as a Semaphore-Merkle alternative at very large scale | Research Track F candidate, explicitly not asserted as better |
| 5 | "Regret nullifier" sketch for coerced/purchased vouches | Research Track E starting point, needs game-theoretic modeling |
| — | Reviving graph-correlation heuristics for multi-life detection | Considered and rejected — reintroduces the exact privacy regression Section 15 correctly removed |

None of the above is proposed as a launch requirement. Consistent with both
the design doc and the audit, #21 should stay open as the research-and-
integration issue it already is, and this document's proposals should enter
the same adversarial-simulation and independent-review pipeline (Phase 5,
Section 21 of the audit) as every other threshold and mechanism in this
design before any of them become load-bearing.
