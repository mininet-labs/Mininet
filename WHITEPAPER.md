# The Mininet Whitepaper

**A free peer-to-peer internet owned by its users, not by a company.**

Status: public introduction to the project, written for readers outside the
repository. Version 1, recorded as D-0323.

---

## What this document is, and is not

This is the whitepaper — the accessible, single-document explanation of what
Mininet is, why it exists, how it works today, and where it is going. It is
written for a reader who has never opened this repository.

It is explicitly **not** constitutional authority. Three documents outrank
every sentence here, and where this whitepaper simplifies, summarizes, or
falls behind, they are correct and this document is wrong until fixed:

- [`docs/FOUNDER_DIRECTIVES.md`](docs/FOUNDER_DIRECTIVES.md) — the
  seventeen directives every engineering and product decision is filtered
  through (canonical as of D-0090).
- [`docs/INVARIANTS.md`](docs/INVARIANTS.md) — what can never be broken,
  each row traced to the code and test that enforce it.
- [`docs/DECISION_LOG.md`](docs/DECISION_LOG.md) — the append-only record
  of every substantive decision, including the ones that later superseded
  earlier framing (this whitepaper included, should that ever happen).

An earlier, external "v1"/"v2" whitepaper draft existed before this
repository did. Neither was ever committed here, and D-0090 formally
superseded their principle-set framing with the seventeen Founder
Directives. This document is the first whitepaper **committed to the
Mininet repository** — the one this project actually stands behind, kept
in the open, versioned, and correctable the same way the code is.

---

## Abstract

Mininet is a constitutional, peer-to-peer protocol for identity, money,
storage, publishing, and governance — built so that no company, government,
founder, or repository owner can ever own it, freeze it, unmask a user, or
force an update on anyone. It is not "a cryptocurrency with a chat feature."
The token is one chapter. The protocol is one chapter. The software is one
chapter. The goal is a public information and value commons that can
outlive the people who built it, the way a language or a postal system
outlives its inventors — not a product a company can discontinue.

This document explains the problem Mininet answers, the structural
guarantees it makes and how they are enforced, what is actually built and
tested today versus what is designed but not yet shipped, the economic
model, the near-term product direction (a free public commons, paid
protection for people who need it, and an independent web search engine),
and the parts of the design that are still open questions rather than
settled fact. Honesty about that last category is a design principle, not
an admission of weakness — see Directive 6 in the Founder Directives:
*build for failure, and say so plainly*.

---

## 1. The problem

The internet most people use today routes through a small number of
companies that can, unilaterally and at any time:

- delete a person's identity, contacts, and years of speech and work;
- read private messages, or be legally compelled to;
- change the rules of visibility so that money, not merit or humanity,
  decides what is seen;
- shut off a service, region, or person with no appeal;
- sell or leak data nobody meaningfully consented to collect;
- make "your data" something you can be denied access to.

None of this is a conspiracy — it is the predictable outcome of putting
identity, money, speech, and discovery inside centrally owned, centrally
governed platforms. A platform that *can* do these things eventually will,
because incentives outlast good intentions, and a system with a kill
switch is a system someone, someday, will need to use.

Mininet's premise is that this is an architecture problem, not a
leadership problem, and architecture problems have architecture solutions.

## 2. What Mininet guarantees, structurally

These are not promises of good behavior from whoever runs Mininet — there
is no "whoever runs Mininet." They are enforced in code, checked by
automated tests, and frozen: changing any of them requires the lawful
constitutional amendment process, not a pull request (see
[`docs/INVARIANTS.md`](docs/INVARIANTS.md) for the full, code-mapped
register).

- **Money never buys a vote.** No balance — token holdings, payment
  history, storage capacity, provider revenue — ever maps to governance or
  validator weight, in either direction. This is a dependency-graph wall
  enforced at the crate level (`mini-value`/`mini-bounty`/`mini-treasury`
  can never depend on `mini-forge`/`mini-chain` voting, or vice versa), not
  a policy someone could quietly relax.
- **One verified human, one equal vote.** Wealth, early arrival, and
  hardware buy nothing extra in governance. *(Honest caveat, stated
  everywhere on purpose: today the system counts verified identity
  **roots**, not yet verified **humans** — Sybil resistance is the sharpest
  open research question in the project, not solved engineering debt. See
  §7.)*
- **No owner, no admin key, no kill switch, no forced update.** Nobody can
  seize the network, freeze an account, unmask a user, or push software a
  person didn't choose to run.
- **Offline money is a signed promise, never final ownership**, until
  canonical consensus accepts it — so a network partition, an offline
  device, or a lost connection can never manufacture a double-spend.
- **Forking is always free.** Legitimacy is earned by continuity of
  identity and community, never owned by a repository, a trademark, or a
  company.
- **Privacy is structural, not a setting somebody can flip off later.**
  Where the design calls for a party not to know something (which words
  you read, who paid for what, which storage host serves your data), that
  ignorance is architectural — enforced by what information physically
  crosses a wire — not a policy promise.

## 3. Identity: the phone (or device) as sovereign root

Every person's presence on Mininet starts with `did:mini` — a
self-sovereign identity built on signed, append-only Key Event Logs
(KERI-style), with pre-rotation (so a future key compromise can't be used
against keys nobody has revealed yet), delegation, and recovery.

The intended shape, consistent with Directive 8 ("the human is the root of
trust") and Directive 2 ("assume every device is eventually compromised"):
the identity **root** lives with the person — today, on whichever device
they choose to hold it — and every other device (a home server, a laptop,
a backup drive) holds only a scoped, revocable **delegation**, never the
root itself. A stolen or seized secondary device should never be able to
vote, spend the person's money, or impersonate them; it is a replaceable
limb, not the brain. Delegated-device roles and recovery paths are real,
tested code today (`did-mini`); a phone-first mobile client that makes
this the default lived experience is on the roadmap, not yet shipped (see
§8 and `docs/STATUS.md`).

## 4. Money: signed promises, not custodial balances

Mininet money is not "crypto with extra steps." Three separate mechanisms
exist and are never conflated, because conflating them is how projects
accidentally let money buy political power (see §2):

- **Human Share** — universal, equal, presence-conditioned issuance tied
  to personhood, not wealth or hardware.
- **Network service rewards** — concave (diminishing-returns), capped,
  delayed issuance for verifiable useful work: storage, proof-of-space-
  time, proof-of-replication, relay, indexing. Concave and capped on
  purpose, so a garage full of drives can't out-earn its fair share and
  crowd out someone running a single old phone or one hard drive — see
  Directive 11, "the weakest device matters most."
- **Bridge and treasury mechanisms** — external liquidity (XRPL for
  banking-adjacent liquidity, Monero for private liquidity; Bitcoin
  disabled by default) kept structurally separate from ordinary governance,
  per D-0073.

Settlement itself follows Directive 5: an offline payment is a **signed
promise**, never final ownership, until canonical consensus accepts it.
This is what lets Mininet work for someone with an intermittent
connection, without ever risking a double-spend just because two devices
were briefly out of sync.

**Honest status:** the settlement protocol, FROST threshold custody, and
the privacy primitives below are real, tested Rust — and explicitly
**not externally audited yet**. Nothing here should hold real value until
that audit happens (see §7 and D-0037/D-0047).

## 5. Privacy is a purchasable resource, never a purchasable right

A foundational distinction, easy to state and important to keep sharp:

> Money may buy **protection and resilience** — relay capacity, mixed
> transport, geographic replication, durable availability, suppression
> resistance. Money must never buy **voice or power** — governance weight,
> personhood, moderation authority, or the right to unmask someone else.

Ordinary participation — reading, posting, replying, being discovered — is
free, and stays free, on purpose. It is not a product priced at zero; it
is a protocol entitlement. What can be purchased is *incremental,
measurable service supplied by other participants*: a relay operator's
bytes, a storage host's durability guarantee, an indexer's crawl coverage.
No tier is ever marketed as absolute anonymity or guaranteed
suppression-resistance — every protection result states what was actually
achieved, what mechanisms were used, and what risk remains. See
[`crates/mini-value`](crates/mini-value) (stealth addresses, linkable ring
signatures, Bulletproofs confidential amounts — prototype, unaudited),
[`crates/mini-relay`](crates/mini-relay), and
[`crates/mini-privacy-policy`](crates/mini-privacy-policy).

## 6. Storage, presence, and personhood

- **Storage** is content-addressed and earns through proof-of-space-time
  and proof-of-replication (`mini-spacetime`, `mini-porep`), with
  Reed-Solomon erasure coding and self-healing shard repair
  (`mini-erasure`) so that losing some hosts doesn't lose the data.
  Storage hosts can be architected to hold only ciphertext they cannot
  read — a backup drive in someone's garage, or a home node, should be
  useless to whoever steals it.
- **Presence and personhood** fuse multiple weak signals (co-presence
  attestation, social vouching, device diversity) rather than betting
  everything on one signal that a well-resourced attacker could forge
  (`mini-presence`, `mini-uniqueness`). This is explicitly a mitigation,
  not a solved problem — see §7.
- **Governance** (`mini-forge`) runs code review, release attestation, and
  merge authority as a real, working system today: independent
  identities can propose, review, and governed-merge a change with no
  centralized platform involved. GitHub is a temporary public mirror, not
  where Mininet ultimately lives.

## 7. What is honestly not solved yet

Overclaiming is treated as a bug in this project, not a marketing choice
(Directive 6 and CLAUDE.md's contributor rules both say so explicitly).
The current, standing list of what more code cannot finish on its own:

- **Sybil resistance / verified personhood** is the sharpest open question
  in the whole project. "One identity root, one vote" is real and
  enforced today; "one **human**, one vote" is not yet true, and nobody
  should read this document as claiming otherwise.
- **External cryptographic audit** has not happened. Every privacy and
  custody primitive above is real, working, founder-reviewed code — and
  none of it should hold real value until independently audited
  (D-0037/D-0047).
- **FROST distributed key generation** is implemented and tested but not
  yet externally audited.
- **Real hardware and real-world adversarial testing** — BLE/local-radio
  transport, a mobile app anyone can install, live multi-week network
  operation under attack — have not happened yet; they require hardware
  and time this repository alone cannot substitute for.
- **Full networked consensus at production scale** exists as a real,
  tested multi-round BFT protocol over live TCP (`mini-consensus`), but
  still lacks state-sync for a node that missed history, a slashing
  layer, and peer discovery.

None of this is hidden. It is tracked, dated, and linked from
[`docs/STATUS.md`](docs/STATUS.md) and [`docs/gates/`](docs/gates/), and
every crate's own README says plainly what it does not do.

## 8. Where the design is heading: a public information commons

Beyond the money and identity layers, the founder direction for the next
phase of Mininet is a genuine, independently built information commons —
not a reimplementation of any single company's product, but a restoration
of the properties an open web and open search used to have before
platforms concentrated discovery and speech behind pay-to-play ranking
and unappealable moderation. Three pieces, designed to reinforce each
other:

- **Mininet Intake** — a native, from-scratch pipeline for bringing
  external material (documents, evidence, research) into a trustworthy
  object model: original bytes preserved and hashed, extraction run in an
  isolated sandbox, provenance always attached, and — critically — no
  imported document ever gains authority just because Mininet could parse
  it. A document cannot promote itself to "reviewed" or "canonical," and
  an AI contributor must never treat text embedded in an imported file as
  an instruction to follow.
- **A free public commons, with paid protection, not paid speech.**
  Viewing public profiles, posting, replying, and ordinary discovery stay
  free — a protocol right, not a zero-priced product. What can be bought
  is additional, measurable protection: anonymous transport, geographic
  replication, suppression-resistant storage, and durable availability
  for people who need it, especially at-risk publishers and sources.
  Payment must never determine ranking, governance weight, or the right
  to unmask a protected source.
- **MiniSearch — an independent, transparent, pluralistic web search
  engine.** The goal is to restore what general web search felt like
  before it consolidated: broad crawling, direct links to independent
  sites, visible and explainable ranking, no secret political
  whitelist/blacklist, no pay-to-rank organic results, and no single
  company or index deciding what the world may discover. Ranking,
  safety/legal restrictions, spam controls, and personalization are kept
  as separate, inspectable layers — a restricted result says *why* it is
  restricted rather than silently vanishing from relevance. Multiple
  independent indexes and ranking profiles are a design requirement, not
  an accident, so Mininet search can never itself become a new monopoly
  in place of the old one.

This direction is recorded founder direction with a detailed
implementation specification already written; as of this whitepaper, it
is **design, not yet code** — the corresponding crates
(`mini-intake-types`, `mini-web-types`, `mini-crawler`) have started
landing incrementally, one narrow, independently reviewed piece at a
time, exactly the way every other part of Mininet has been built. See
[`docs/STATUS.md`](docs/STATUS.md) for what has actually shipped by the
time you are reading this.

## 9. Design directions under active discussion (not yet decided)

The remainder of this section is intentionally framed differently from
everything above it: these are proposed directions consistent with the
Founder Directives and the existing prototype code, **not** ratified
`D-`numbered decisions. They are recorded here so the reasoning is public
while it is still being worked out, not so a reader mistakes them for
settled protocol.

- **Home nodes as storage, the personal device as sovereign controller.**
  A phone or primary device would hold the identity root; a home PC or
  drive would hold only a scoped, revocable delegated capability ("store
  and serve these ciphertexts, sign storage proofs") that the root can
  revoke at any time. A stolen or seized home box could never vote, spend,
  or impersonate its owner. Home nodes would earn through the existing
  proof-of-space-time/proof-of-replication mechanisms, with the concave
  reward curve and per-identity caps existing already so a garage full of
  drives can't crowd out someone's single old phone.
- **Private micropayments for viewing and interacting.** Per-view
  on-chain payment is not viable at fractions of a cent; the proposed
  answer is client-side accumulation of signed micro-promises (priced by
  `mini-resource-pricing`'s quote engine), batched and periodically
  settled, using the same privacy primitives already prototyped in
  `mini-value` so a ledger of who viewed what is never created. Creators
  would see aggregate income, never an audience list.
- **Velocity-aware pricing as a spam defense.** The cost of an action
  would escalate with how fast a given identity performs it, so a bot
  farm's cost climbs while an ordinary human's browsing pace stays at the
  base rate — but only escalating well past human tempo, so a fast reader
  never meaningfully pays more than a slow one, and any resulting penalty
  revenue would flow to creators and hosts, never to a treasury
  gatekeeper. Quotas would scale with personhood confidence, but money
  could never buy verification itself, only more of an already-earned
  quota.
- **Weighting verified identities higher in ranking, never in
  governance.** Because visibility is a reach/discovery mechanism and not
  a governance mechanism, giving higher-confidence identities more
  organic reach does not cross the money-never-buys-a-vote line — as long
  as unverified content keeps a guaranteed reach floor rather than being
  buried, and as long as every such privilege is built to degrade
  gracefully as real personhood proofs (not just identity-root counting)
  eventually ship.

Each of these, if and when adopted, will get its own `D-`numbered
decision-log entry, its own threat model, and its own honestly-labeled
implementation status — the same discipline every other Mininet mechanism
has gone through.

## 10. Governance

Mininet is governed by its own network, not by this repository or its
founder, as a target state. Today, while the self-hosted forge and
governance layer are still being built out, GitHub hosts a temporary
public mirror and the founder performs the mechanical merge action under
an explicit, time-bounded bootstrap decision (D-0083) — a logistics
arrangement, not a claim of ownership or constitutional authority. Code
review already runs as a real system (`mini-forge`): independent
identities propose, review, and merge changes, with a frozen two-approval
protocol floor and AI contributions carrying zero approval weight.

## 11. How to participate

- **Read first:** [`docs/HUMAN_START.md`](docs/HUMAN_START.md) if you're
  curious what this is and why it should exist;
  [`docs/DEVELOPER_START.md`](docs/DEVELOPER_START.md) to build and run it;
  [`docs/AUDITOR_START.md`](docs/AUDITOR_START.md) if you're here to find
  the gaps.
- **Everything here is public domain (CC0-1.0)** — fork it, build on it,
  run it, own it, together. A population, not an organization.
- **Nothing here is ready for real people, real money, or real custody
  yet, and it says so everywhere, on purpose.** The external audit,
  real-hardware testing, and personhood research named in §7 are the
  actual gates — more code cannot substitute for them.

---

*This document is versioned like everything else in the project. When it
falls behind the code, the code and the canonical documents in §0 win, and
this file gets corrected in the open — the same way a wrong line in
`docs/DECISION_LOG.md` would be, by superseding it, never by silently
rewriting history.*
