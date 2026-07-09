# Human uniqueness proof research (signal b) — founder decision needed

Gates [roadmap #21](https://github.com/britak420/Mininet/issues/21).
**This is not an engineering backlog item.** The whitepaper itself calls
on-device behavioral/location entropy proved in zero-knowledge *unsolved
research*, and nothing in this repository — or, as far as this session's
research went, in the public cryptography literature — has a known
construction satisfying all of: private (no raw behavioral trace leaves
the device), mobile-friendly (runs on a phone, not a data center), and
platform-vendor-independent (doesn't quietly make Apple/Google attestation
a trust root).

## Why the system doesn't block on this today

D-0038's multi-signal redesign (`mini-uniqueness::status`) deliberately
made no single signal load-bearing: `FullHuman` requires diversity across
live sources, and — since D-0054's hardening — specifically requires the
seed-anchored vouching-graph signal to be live, not merely "any two of
N." Signal (b) is one optional contributor among several, not a
dependency. This is why #21 is P1, not P0: the system works today without
it, just with a narrower evidence base than the whitepaper's original
three-signal design envisioned.

## The three options, argued honestly

### Option A — deprioritize signal (b) permanently for launch (recommended)

Launch personhood on presence + social graph + slow vesting + cross-
cluster clamp — the three signals actually built and hardened this
session (`docs/audits/issue-17-presence-attack-review.md`,
`docs/audits/issue-18-sybil-social-graph-review.md`). Signal (b) stays an
optional `SignalSource::BehavioralEntropy` slot
(`mini-uniqueness::confidence::BehavioralEntropySource`) that can plug in
later with zero architecture change, exactly as designed. This is the
lowest-risk path: it ships what's actually tested today and treats the
hardest open research problem as genuinely open, rather than rushing a
weak implementation that would then need to be honestly disclosed as
weak anyway.

### Option B — fund academic research

Create a research bounty or fellowship with a narrow, precisely-scoped
brief:

> A privacy-preserving proof of on-device behavioral/mobility entropy,
> where (1) no raw location or behavioral trace ever leaves the device,
> (2) no persistent device identifier enters the personhood-eligibility
> state, (3) the proof runs within a real phone's compute/battery budget,
> and (4) the construction does not require trusting a specific platform
> vendor's attestation as a root of trust.

This is a real research question with no guaranteed answer on any
timeline — treat funding it as a bet on eventually strengthening
personhood evidence, not a launch dependency.

### Option C — TEE attestation interim

Technically available today (Apple/Google device attestation), but
philosophically weaker: it makes a specific platform vendor's attestation
chain a trust dependency for personhood, which sits uncomfortably next to
Directive 2 ("assume every central authority will eventually fail") and
P3 ("no owner, no admin key") — a TEE vendor is exactly the kind of
central authority the constitution is built to route around elsewhere.
**Recommendation: avoid making this mandatory** even if it's offered as
one more optional weighted signal alongside the others; never let it be
the signal that alone unlocks `FullHuman`.

## The Rust shape, regardless of which option is chosen

Signal (b) should stay exactly what it already is — pluggable and
non-authoritative:

```rust
pub trait EntropySignal {
    fn verify_entropy(
        &self,
        proof: EntropyProof,
        epoch: Epoch,
    ) -> Result<EntropyVerdict, EntropyError>;
}

pub enum EntropyVerdict {
    Unavailable,
    WeakAttested,
    StrongZk,
}
```

## Hard rules regardless of which option the founder picks

- No identity may reach `FullHuman` from signal (b) alone — the
  `full_required_sources` gate (D-0054) already enforces requiring the
  seed-anchored graph signal; signal (b), whenever it exists, adds to the
  score, it never substitutes for that requirement.
- No raw behavioral trace, and no permanent device identifier, may ever
  enter `mini-uniqueness`'s personhood state — only a derived
  strength score, matching the existing `SignalEvidence` discipline (P5:
  no raw personal data).

## What closes this gate

A founder decision among A/B/C above, recorded as a new D-number. Option
A requires no further action beyond that record. Options B/C each need
their own follow-up issue once chosen.
