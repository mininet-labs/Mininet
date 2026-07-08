# The Mininet Failure Book

Founder proposal (tracked in GitHub issue #91): a permanent record of every
failed idea, rejected design, abandoned cryptographic primitive,
considered-and-rejected economic exploit, governance loophole, and
simulation that demonstrated a failure mode — not just bugs.

## Why this exists, and how it differs from the Decision Log

`docs/DECISION_LOG.md` records decisions *made*, numbered and dated, each
with its rationale. This document records paths *not taken* — including
ones that never got far enough to need a formal `D-00xx` entry, or that
are folded into a decision log entry's rationale but deserve their own
standalone record so a future engineer doesn't have to reconstruct "why
not X" by reading between the lines of "why Y."

Per the founder's own framing (`docs/FOUNDER_DIRECTIVES.md`, adopted
D-0043): "future engineers are much less likely to repeat mistakes if
they can see why earlier approaches were rejected. Over a century-scale
project, preserving engineering reasoning is almost as important as
preserving the code itself."

## Entry format

Each entry answers four questions. Keep entries short — this is a
reference, not an essay:

```
### [Short name of the rejected/abandoned approach]
**Considered/tried:** when, and in what context.
**What it was:** one or two sentences.
**Why it was rejected:** the actual reasoning, not just "we chose
something else."
**Would it become viable again?** State a real condition, or "no — this
is closed, see [reference]" if it's a closed question (per Directive 13/
the Decision Log's own convention of marking some questions permanently
closed rather than open-ended).
```

Add new entries at the bottom of the relevant section, oldest first —
this is a chronological record, not a ranked list.

---

## Architecture & infrastructure

### Radio / LoRa as a core transport
**Considered/tried:** early networking-core design (pre-D-0009).
**What it was:** using LoRa or similar long-range radio as one of
Mininet's core bearer types, alongside BLE and local Wi-Fi.
**Why it was rejected:** dropped in favor of adapted-proven plumbing
(BLE + local Wi-Fi/hotspot/mDNS + optional internet relay, store-and-
forward/delay-tolerant sync). Reaffirmed explicitly in D-0033.
**Would it become viable again?** No — founder decision (2026-07-07,
reaffirmed D-0033): "radio/LoRa is permanently out of scope... a closed
question, not an open one to revisit as the network scales."

### Cosmos SDK / Go chain stack
**Considered/tried:** early chain-stack decision (pre-D-0001, referenced
as "Founder Decision A1").
**What it was:** building Mininet's chain on the Cosmos SDK (Go).
**Why it was rejected:** Founder Decision A1 targeted a custom Rust chain
instead — keeping the whole stack in one language (D-0001) and avoiding a
live external dependency on a framework this project doesn't control, per
the same "adapt the design, not the dependency" stance later formalized
in D-0034 point 3.
**Would it become viable again?** No — superseded language was rewritten
in place across the docs rather than preserved "for history" (D-0034);
this Failure Book entry is that history now.

### Flutter for the client UI
**Considered/tried:** D-0019.
**What it was:** using Flutter for the eventual mobile/desktop client.
**Why it was rejected:** superseded by D-0020 — a Google-governed
toolchain and non-reproducible builds conflict with the sovereignty-first
directive and the frozen SPEC-11 reproducible-build requirement.
**Would it become viable again?** Only if Flutter's build were made fully
reproducible and toolchain-independent of Google — not the case today.

## Personhood & identity

### Integrating an existing proof-of-personhood project
**Considered/tried:** SPEC-02 personhood design phase.
**What it was:** using a third-party proof-of-personhood service/project
instead of building Mininet's own graph-based uniqueness algorithm.
**Why it was rejected:** puts a third party's graph/servers in the trust
path — directly against Directive 8 ("the human is the root of trust...
not corporations... if any mechanism begins drifting toward institutional
trust, remove it") and the whitepaper's own architecture.
**Would it become viable again?** Not as a dependency. A future audit
could still borrow *design ideas* from such projects (same "adapt the
design, not the dependency" stance as everywhere else in this tree)
without adopting their infrastructure.

### Biometrics for personhood verification
**Considered/tried:** SPEC-02 personhood design phase.
**What it was:** using biometric data (fingerprint, face, iris) as a
uniqueness signal.
**Why it was rejected:** rejected outright by P5 (privacy is structural;
no raw personal data may be collected or transmitted for this purpose).
**Would it become viable again?** No — this is a constitutional privacy
floor (Directive 9: "depend only on mathematics"), not a technical
limitation that better cryptography could lift.

### Fixed three-signal personhood fusion as the permanent design
**Considered/tried:** original SPEC-02 implementation (`mini-uniqueness::
confidence::fuse_confidence`).
**What it was:** personhood as exactly three fixed, weighted signals
(social vouching, physical presence, behavioral/location entropy) fused
into one score.
**Why it was superseded, not exactly rejected:** signal (b) (behavioral/
location entropy proved in zero-knowledge) is unsolved research — the
whitepaper says so itself. Rather than blocking on solving it, D-0038
redesigned personhood into an open-ended, weighted multi-signal system
(`mini-uniqueness::status`) so the *system* doesn't depend on any one
signal being unbreakable. `fuse_confidence` is not deleted — it's still
correct for the fixed three-signal case — but it's no longer the
forward-looking design.
**Would it become viable again?** The open-ended design is not expected
to be reverted; if signal (b) is ever actually solved (see issue #21),
it becomes one more signal in the open-ended system, not a reason to go
back to exactly three.

## Implementation-level corrections

### `ConfidentialAmountScheme`'s original trait shape
**Considered/tried:** first pass at `mini-value::confidential` (before
D-0040's Bulletproofs implementation).
**What it was:** `fn commit(&self, amount: u64, blinding: &[u8]) ->
Option<Vec<u8>>` — a commitment with no accompanying way to actually
produce or verify a range proof.
**Why it was rejected:** once the real Bulletproofs implementation was
designed, this shape turned out to be insufficient — a commitment without
a range proof can't prove a hidden amount is non-negative, which is the
entire point of confidential amounts. Corrected to `commit_with_proof`/
`verify_range_proof`/`verify_balance` (D-0040).
**Would it become viable again?** No — this was a genuine design error
caught before anything depended on it, not a tradeoff. Noted here mainly
as an example of the entry format at implementation scale, not just
architecture scale: this book is for correcting the record, not for
grand rejected ideas only.

---

## Open item

This document was seeded from the project's own history at the time
`docs/FOUNDER_DIRECTIVES.md` was adopted (D-0043) and issue #91 was
filed. It is not exhaustive — anyone who knows of an earlier
considered-and-rejected approach not listed here should add it, following
the entry format above.
