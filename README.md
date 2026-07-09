# Mininet

**A free peer-to-peer internet owned by its users, not by a company.**

Mininet is building a network where people own their identity, data, money,
voice, and infrastructure. Money can buy storage, reach, and the funding of
work — but never political power. Governance is one verified human, one equal
vote. There is no owner, no foundation, no admin key, no forced-update path,
no off switch, no law-enforcement backdoor, and no party that can unmask a
user.

> **This GitHub repository is only a temporary public mirror.** The long-term
> code forge is content-addressed, self-governed, reproducibly built, and
> owned by the network itself — GitHub is where the work is shown while it is
> built, never where it ultimately lives (see [`docs/ADDRESSING.md`](docs/ADDRESSING.md)
> and `mini-forge`).

## What no one can change

These are not promises of good behavior — they are structural, enforced in
code, and frozen. A full, code-mapped register is in
[`docs/INVARIANTS.md`](docs/INVARIANTS.md); the short version:

- **Money never buys a vote.** No balance maps to governance or validator
  weight, in either direction — a wall enforced by the dependency graph
  itself, not by policy ([`docs/design/bounty-and-review.md`](docs/design/bounty-and-review.md)).
- **One verified human, one equal vote.** Early arrival, wealth, and hardware
  buy nothing extra. *(Today the system counts verified identity **roots**,
  not yet verified humans — the honest gap is stated plainly below and at the
  top of `docs/INVARIANTS.md`.)*
- **No owner, no admin key, no kill switch, no forced update.** Nobody can
  seize the network, freeze it, unmask a user, or push software you didn't
  choose to run.
- **Offline money is a signed promise, never final ownership** until canonical
  consensus accepts it — so a network partition can never manufacture a
  double-spend ([`crates/mini-settlement`](crates/mini-settlement)).
- **Forking is always free; legitimacy is earned by continuity,** never owned
  by a repository or a trademark ([`docs/design/fork-legitimacy.md`](docs/design/fork-legitimacy.md)).

## What exists today — honestly

This repository is the **self-contained Rust core**: ~25 crates, no external
dependency on any single company's infrastructure to keep running. Nothing
here is ready for real people, real money, or real custody yet — and it says
so, everywhere, on purpose.

**Working, tested Rust:**
- `did:mini` self-sovereign identity + device delegation + lost-device
  recovery
- signed, content-addressed objects; local storage; social feeds; public
  walls
- BFT finality-verification core; governed release/update path (no forced
  update, no kill switch)
- a real TCP transport with a live three-process gossip demo

**Prototype cryptography — real code, founder-reviewed, NOT yet audited:**
- stealth addresses, linkable ring signatures, Bulletproofs confidential
  amounts (`mini-value`)
- FROST threshold custody (`mini-treasury`)
- Merkle/PDP storage proofs (`mini-spacetime`)
- anonymous developer-bounty claims (`mini-bounty`); offline settlement
  protocol (`mini-settlement`)

**Not ready yet, and openly tracked:**
- a mobile or desktop app anyone can install
- BLE / local-radio transport (needs real phone hardware)
- full networked consensus and a live chain
- external cryptography audit — the single largest gate before any real value
- FROST distributed key generation is implemented and tested (Pedersen DKG
  + committee resharing) but not yet externally audited
- a solved, privacy-preserving personhood/liveness proof (open research, not
  engineering debt)
- adversarial testing at real-world scale

The work that **more code cannot finish** — external audits, legal review,
real-hardware testing, and open research decisions — is named explicitly, so
a finished-looking GitHub repo is never mistaken for a launch-ready network:
[`docs/gates/`](docs/gates/) and tracking issue [#99](../../issues/99).

## Start here

Pick the door that fits you:

| You are… | Start with |
|---|---|
| **A curious person** — what is this, and why should it exist? | [`docs/HUMAN_START.md`](docs/HUMAN_START.md) |
| **A developer** — build it, run the demos, find your way around | [`docs/DEVELOPER_START.md`](docs/DEVELOPER_START.md) |
| **An auditor or skeptic** — where are the invariants, threats, and honest gaps? | [`docs/AUDITOR_START.md`](docs/AUDITOR_START.md) |
| **A contributor** — how work is reviewed and merged | [`CONTRIBUTING.md`](CONTRIBUTING.md) |

Beneath everything else in this repository — read before opening the code —
is [`docs/FOUNDER_DIRECTIVES.md`](docs/FOUNDER_DIRECTIVES.md): the seventeen
principles the whole project is filtered through, written so that a century
from now, someone facing a problem no document anticipated can still reason
the way the founders would have.

## The canonical documents

Mininet preserves its *reasoning* as first-class infrastructure, not just its
code — because a network meant to outlive its creators has to explain itself
to people who will never meet them:

1. [`docs/FOUNDER_DIRECTIVES.md`](docs/FOUNDER_DIRECTIVES.md) — *why the
   project exists and what it values.*
2. [`docs/INVARIANTS.md`](docs/INVARIANTS.md) — *what can never be broken*,
   each row traced Directive → Invariant → Source → enforcing code + test.
3. [`docs/DECISION_LOG.md`](docs/DECISION_LOG.md) — *why each choice was made,
   and when it was superseded* (append-only; `D-0001`–`D-0057` so far).
4. [`docs/FAILURE_BOOK.md`](docs/FAILURE_BOOK.md) — *what was tried and
   rejected, and why* — read before re-proposing something.
5. [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) — *what could kill the
   project at civilization scale*, and which invariant, if any, is the
   defense.

Living detail: [`docs/STATUS.md`](docs/STATUS.md) (what's actually built, by
domain), [`docs/gates/`](docs/gates/) (external legitimacy gates),
[`docs/audits/`](docs/audits/) (review deliverables),
[`docs/design/`](docs/design/) (design notes). Find anything offline:
`python3 tools/mininet_nav.py map` (see [`docs/NAVIGATION.md`](docs/NAVIGATION.md)).

## License

Public domain (CC0-1.0). Fork it, build on it, run it — own it, together. A
population, not an organization.
