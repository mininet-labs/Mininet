# Anonymous resource payment and redemption preparation (MN-602/MN-603, D-0099)

**Status:** Doctrine and research preparation only. No `mini-resource-token`,
`mini-resource-redemption`, or `mini-resource-wallet` crate exists. No
blind-signature dependency added. `mini-resource-pricing` (MN-601, D-0302)
is unmodified — it remains pure quoting logic with no keys, no issuance,
no transfers.

**Full research:** `docs/research/
MN602_MN603_ANONYMOUS_RESOURCE_PAYMENT_RESEARCH_20260715.md`
(founder-supplied, 2026-07-15). This document does not reproduce that
report — it records what direction was adopted from it and why, and links
back for the full role-separation design, token-format sketches,
adversarial test taxonomy, and phased rollout the report itself lays out.

## Decision

Per the report's own executive conclusion, Mininet must not pay relay,
bridge, mix, storage, cover-traffic, or private-index providers by
attaching an ordinary MINI transfer to each service request — that turns
the payment graph into a second metadata graph correlating payer
identity, privacy tier, provider, and timing. `mini-resource-pricing`
(MN-601, D-0302) already produces a `Quote` (`quote.rs`) without
executing any payment; this decision adopts the report's recommended
follow-on architecture for the payment itself, and freezes it as
doctrine before any code exists.

The adopted first-protocol shape: **online-spend, issuer-backed,
fixed-denomination blind-signature tokens**, with immediate atomic
spent-token checking and batched provider redemption — not offline
anonymous cash, not a new general currency, and not a fork or
reimplementation of Privacy Pass, GNU Taler, or Coconut. Each of those
three prior-art systems is named as a reference point, not a dependency:
Privacy Pass's issuance/redemption role separation is the closest model
for the first, non-monetary test-credit prototype; GNU Taler is the
strongest deployed prior art for actual anonymous *payer* privacy with
accountable *provider* settlement, evaluated later as a possible
external rail rather than embedded; Coconut's threshold-issued
credentials are reserved for later distributed-mint research (MN-603B),
not the first single-issuer version.

## Role separation (the report's central discipline)

Five roles must stay separable in any future implementation, and no
future PR may collapse them without a new decision:

1. **Funding source** — knows the withdrawal account and amount; never
   knows which service or provider consumes a token.
2. **Token issuer** — verifies funding, blindly signs token material;
   never learns the unblinded serial.
3. **Client wallet** — holds unspent tokens, selects denominations,
   presents them.
4. **Service provider** — accepts a token for one typed resource; never
   learns the payer's withdrawal or root identity.
5. **Redemption service** — atomically marks serials spent and credits
   providers; may initially be the same operational service as the
   issuer, but its logs and protocol role stay separable from day one.

## What a token represents

Not a general bearer MINI coin. A fixed unit of resource credit scoped
to one `ResourceCreditClass` (relay byte bucket, mix packet, storage
byte-day, private-index query, bridge session, cover-packet
contribution — vocabulary only, no type exists yet) and one fixed
denomination from a small standard set. This keeps the first protocol's
scope narrow: exchange-rate complexity stays outside spending, provider
redemption can reference `mini-resource-pricing`'s existing quote table,
and the token structurally cannot become a general parallel currency by
accident.

## Online-spend, not offline e-cash

The report's own reasoning is adopted verbatim: offline spending needs a
provider to accept a token without consulting the issuer and later
detect reuse, which drags in identity-escrow double-spend tracing,
conflict ordering, and disconnected-settlement risk this workspace is
not ready to design or review. The first protocol instead requires a
real online round trip — provider submits the redemption batch/
reservation, the redemption service atomically marks the serial spent,
the provider gets a signed acceptance, then service begins. Mininet's
BLE/local-Wi-Fi/delay-tolerant ambitions may eventually justify offline
payment; that is explicitly deferred (report §19), not solved by
assumption now.

## Subsidies use the same token format as paid credits

A non-negotiable constraint carried forward from the report: a system
that prices every privacy layer but gives no subsidised baseline access
makes surveillance resistance a luxury good, and a distinguishable
subsidy token would let a provider learn which users could not or did
not pay — defeating the anonymity the paid token is supposed to provide
everyone. The funding policy may differ (universal allotment, community
grant, application-funded, proof-of-work); the *spend* protocol must
never carry a bit that says which kind of credit this was. Because
personhood/Sybil resistance remains unsolved (per `docs/INVARIANTS.md`'s
hard-limitation section), no subsidy mechanism may be represented as
one-human-one-share — early subsidies must stay low-value and bounded.

## Voice/value wall (hard rule, restated for this track)

Resource-token balances must never enter vote calculations, review
quorum, validator weight, personhood score, witness selection, merge
authority, or constitutional amendment. No crate that will eventually
provide anonymous payment may ever be imported by governance-counting
code (`mini-forge::governance`, `mini-chain` voting) — this is the same
Directive 16 wall CLAUDE.md already enforces for `mini-value`/
`mini-bounty`/`mini-treasury`, and this track inherits it unconditionally
rather than re-deriving it. A provider may earn settlement value from
carrying traffic; that must never translate into governance authority.

## Proposed (not yet built) crate boundaries

Named here so a future implementation has a stable target, not created
in this PR:

- `mini-resource-token` — token types, blind issuance protocol, spend
  validation, denomination metadata.
- `mini-resource-redemption` — atomic spent set, provider batch
  redemption, settlement receipts.
- `mini-resource-wallet` — local token management, withdrawal,
  denomination selection, spend reservation.

`mini-resource-pricing` stays exactly what it is today: pure quoting
logic, no keys, no issuance, no transfers.

## No new cryptography

Directive 14 applies in full. Blind-signature/anonymous-credential
schemes are genuinely novel cryptographic engineering surface this
workspace has never touched — nothing here composes or invents a
primitive; this document names research paths (an external, already-
reviewed Privacy Pass implementation for the first non-monetary
prototype; GNU Taler evaluated as an external rail; Coconut reserved for
threshold research) rather than selecting or building anything. Per the
report's own phase 6, external cryptographic review of whichever
blind-signature integration is eventually chosen is required before any
implementation proceeds past valueless test credits, and per phase 9, no
real MINI may back a token before that review, an accounting review, and
a legal review all separately complete.

## What's required before any code PR

Mirroring the report's own phase ordering (§29): a Phase 0 doctrine
document (this one) → Phase 1 non-monetary test token types with no
blindness claim → Phase 2 a real blind-issuance prototype behind one
reviewed external implementation, still valueless → Phase 3 integration
with one low-risk resource (private-index query or a fixed relay byte
bucket), still no real settlement → Phase 4 provider batch redemption →
Phase 5 adversarial simulation → Phase 6 external cryptographic review →
Phase 7 a closed valueless pilot → Phase 8 economic/legal classification
→ Phase 9 a limited MINI-backed pilot only after all of the above. None
of Phases 1-9 are started by this PR.
