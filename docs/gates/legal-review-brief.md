# Legal counsel review — technical brief

Gates [roadmap #96](../../issues/96), P0.
**Founder action required: engage counsel.** This document is written so
counsel can review the actual mechanism instead of reverse-engineering it
from source — it is not a request for anyone in this repository, human or
AI, to give legal advice.

## The mechanism, plainly

Mininet's contribution path (roadmap [#23](../../issues/23),
SPEC-07) mints MINI from **verified deposits**, not sales:

1. A contributor sends BTC to a treasury-controlled address, or completes
   an XMR→BTC atomic swap and the resulting BTC lands there.
2. `mini-treasury` verifies the BTC deposit via **SPV proof** (a
   cryptographic proof the transaction is in a confirmed Bitcoin block) —
   no oracle, no manual approval, no human deciding who gets to
   contribute.
3. A bounded MINI amount mints automatically, keyed to the verified
   deposit amount and a governance-set rate.

There is **no seller**: no entity is offering MINI for sale, no privileged
party allocates it, and no admin key exists that could change who
qualifies. The mechanism is closer to "a vending machine that only accepts
verified coins" than to a sale.

## Why direct XMR isn't accepted directly

SPEC-07 is explicit about a real cryptographic limit, not a policy choice:
Bitcoin's chain state can be trustlessly proven to `mini-chain` via SPV;
Monero's cannot be proven the same way to a chain that isn't running
Monero's own consensus. The accepted path is XMR → atomic swap → BTC →
SPV proof, never a direct "trust us, the XMR arrived" oracle. This is a
cryptographic constraint counsel should know is not negotiable without
reintroducing exactly the trusted-third-party risk the atomic-swap design
avoids.

## What counsel needs to evaluate

- **Securities law** — does a contribution receipt, or MINI itself,
  function as a security under the relevant jurisdiction's test (e.g.
  Howey in the US)? The "no seller, no promise of profit from others'
  efforts" framing is the technical fact pattern; counsel determines the
  legal characterization.
- **Money transmission** — does verifying a BTC deposit and issuing MINI
  in response constitute money transmission requiring licensing? Does it
  matter that no party ever custodies the contributor's original BTC
  beyond the treasury's own multi-signature custody (see `mini-treasury`,
  gated by [#93](../../issues/93) before
  any real-value ceremony runs)?
- **AML/KYC** — the protocol accepts pseudonymous BTC deposits with no
  identity verification step anywhere in the mechanism (that would
  require exactly the admin/gatekeeper capability the Constitution
  forbids, per P3). What exposure does that create, and is there a
  legally sound way to operate without adding one?
- **Sanctions exposure** — same question specifically for OFAC/sanctions
  list screening, which conventionally *requires* a KYC-like gate this
  protocol structurally doesn't have.
- **Tax treatment** — for the protocol/treasury itself (if it has any
  legal-entity wrapper), for contributors receiving MINI, and for the
  founder.
- **Consumer protection / public communications** — what can and cannot
  be said publicly about MINI (price expectations, "investment," etc.)
  without triggering securities or consumer-protection exposure.

## Related documents this brief doesn't duplicate

- `docs/design/bounty-and-review.md` (D-0051) — the voice/value wall as it
  applies to developer bounty payouts, structurally separate from the
  contribution mechanism above but worth counsel's awareness since both
  move real value.
- `docs/DECISION_LOG.md` D-0036/D-0040/D-0041/D-0047/D-0048 — the
  cryptography-readiness gates that must also close before any of this
  touches real funds; legal clearance and cryptographic audit clearance
  are independent gates, both required.

## Hard constraint on the review

Per Directive 2 and P3 (no owner, no admin key, no central authority
Mininet assumes will eventually fail): counsel advises on **launch
posture** — which jurisdictions to operate in, how to structure any legal
entity, what disclosures to make. Counsel does **not** get to require
adding a KYC gate inside the protocol core, an admin seizure key, or any
other capability that would violate the frozen invariants. If a
jurisdiction's requirements are incompatible with those constraints, the
answer the founder should expect is "don't launch value-bearing
contribution in that jurisdiction," not "weaken the constitution to fit
the jurisdiction."

## What closes this gate

A legal opinion (or engagement letter plus a completed review) covering
the questions above, with a clear go/no-go/go-with-conditions verdict for
#23 accepting real funds — recorded as a new D-number once received.
