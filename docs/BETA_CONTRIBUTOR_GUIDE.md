# Open Beta contributor guide

**Open Beta participation is now open.** Start with [`BETA_OPEN.md`](BETA_OPEN.md)
if you want the shortest practical route. This page explains how beta evidence,
implementation work, contribution recognition, Beta MINI, and the Forge cutover
fit together.

This is an orientation and evidence guide, not a grant of authority. Canonical
invariants and accepted bootstrap decisions control if this page is ever stale.

## Current boundary

The project is in the final **Pre-Go-Live** phase, not production. PR #333 landed
the BLE multi-hop transport/Android code slice and CI can build it, but real
physical-device evidence, product wiring, broader adversarial/lifecycle testing,
and external/value-safety gates still matter. Passing Rust/Android CI is not a
real-phone acceptance test or an external cryptography audit.

Production value, treasury custody, production personhood claims, and unaudited
cryptographic paths are not activated by Open Beta. Current governance/consensus
claims must still distinguish verified identity roots from proven unique humans.

## Choose a route

| You can help with | Safe first route | Evidence to return |
|---|---|---|
| Try the product / reproduce a bug | [Beta test report](../../issues/new?template=beta-test-report.yml) | exact revision, evidence class, component, environment, steps, expected/observed, redacted artifacts, limits |
| One or more Android phones | follow `BETA_OPEN.md` one/two/multi-phone itineraries | physical-device evidence; device/OS facts needed to reproduce, no stable identifiers |
| Accessibility/usability | first-run, permissions, errors, recovery, assistive tech | exact state + task + observed barrier + proposed/verified improvement |
| Rust implementation/tests | choose an open scoped task and claim it | focused tests, exact state, non-goals, full PR ritual |
| Forge transition work | use the migration matrix in `FORGE_BETA_MIGRATION.md` | GitHub-independent campaign/finding/task/review/reward evidence |
| Build/release/reproducibility | current release/build issues and gates | exact build inputs, outputs/digests, independent reproduction limits |
| Documentation/research/threat modeling | open a scoped evidence issue | sources, alternatives, falsification conditions, unresolved questions |
| Security/external review | private security route where needed + `docs/gates/` | scope, exact reviewed state, findings, disposition, residual risk |
| Domain consultation | one bounded evidence question | technical evidence; status/employment never creates authority |

If unsure, choose the smallest reproduction, documentation, or test task. A useful
single report is a complete contribution; nobody has to build a persistent
profile or stay involved.

## Pre-Go-Live anonymity

`docs/governance/52_PRE_GO_LIVE_GOVERNANCE_PAUSE.md` controls this period:
Mininet-level bootstrap participation is canonically **anonymous** until Go-Live.

Therefore:

- GitHub usernames/emails are transport metadata, not Mininet identities;
- no legal name, Mininet pseudonym, DID, reputation handle, or continuity proof
  is required for testing/contribution;
- separate submissions must not be silently linked to construct a hidden profile;
- the Forge-native beta schema records fresh artifact-scoped `SubmissionTag` /
  `ClaimTag` values instead of contributor identities; and
- a Beta MINI destination/claim handle must not be posted publicly or promoted
  into a governance/reviewer/personhood credential.

A person may voluntarily identify themselves on GitHub; Mininet still does not
make that identity canonical during Pre-Go-Live.

## The contribution path

The target workflow is the same whether GitHub is available or not:

1. **Discover** a beta campaign or bounded task.
2. **Test/build/review** an exact state and preserve reproducible evidence.
3. **Submit** a structured finding or implementation artifact.
4. **Disposition** the finding append-only: accepted, duplicate, needs info,
   fixed, cannot reproduce, or rejected with rationale.
5. **Scope** actionable work as a Forge task brief.
6. **Claim** work explicitly with an expiring claim; a claim prevents collision,
   it is not an assignment or approval.
7. **Handoff** exact-state technical review evidence separately from governance or
   merge/release approval.
8. **Record** useful accepted work as an artifact-bound contribution receipt.
9. **Optionally grant Beta MINI** for testing/participation through a fresh
   one-contribution claim path.
10. **Replicate** the evidence through Mininet store/sync so GitHub becomes a
   mirror instead of a prerequisite.

Existing Forge-native `TaskBrief`, `WorkClaim`, task suggestions, and
`TechnicalReview` live in `mini-forge`. PR #334's `mini-beta` adds campaign,
finding, disposition, accepted-contribution, and beta-grant objects on the same
content-addressed object substrate.

## Beta MINI participation

Beta MINI lets people exercise economic UX and lets the beta recognize useful
participation without prematurely activating real value.

Two grant classes exist:

- **testing** — free test balance for product flows;
- **participation** — requires an accepted contribution receipt and can recognize
  testing, reproduction, device work, accessibility, docs, security/research,
  code/tests, review, reproducibility, or operational evidence.

The reference code in `crates/mini-beta` enforces explicit epochs, per-grant and
epoch supply bounds, duplicate-grant rejection, transfer accounting, and zeroed
epoch rollover. It has no dependency on production value/settlement/treasury/
chain/consensus/governance crates.

Beta MINI is resettable test value and creates **no automatic production-MINI
conversion right**. That separation is deliberate: promising conversion now
would create a pre-allocation/issuer obligation before production value passes
its substantive audits and before Forge governance is canonical.

Money still never buys voice. Balance, reward amount, employer status,
contribution count, or hardware ownership must not affect governance, review,
release, personhood, or merge authority.

## GitHub is an adapter, Forge is the destination

Use GitHub freely while it is useful, but do not design new beta processes that
only make sense on GitHub. See [`FORGE_BETA_MIGRATION.md`](FORGE_BETA_MIGRATION.md).

Before `forge_canonical = true`, the project must prove a complete GitHub-outage
loop: discover campaign -> submit finding -> disposition -> task -> claim ->
review evidence -> accepted contribution -> Beta MINI grant, all on Mininet
objects/store/sync with no GitHub account/API required.

The one-way Go-Live transition then ends bootstrap canonical integration; it must
not merely rename GitHub centralization as Forge governance.

## What to say in every report

Always state:

- exact commit/release/object state;
- evidence class (physical device, emulator, Rust/toolchain, research, external,
  accessibility, other);
- what you tested and did not test;
- environment and reproducible steps;
- expected and observed result;
- privacy/safety/recovery limitations;
- what is not built, not audited, not anonymous, or not enforced.

Never upload secrets, private keys, stable device identifiers, unnecessary
personal/location data, or a public Beta MINI claim/account handle. For
security-sensitive material, use the private security route.

## Further reading

- [`BETA_OPEN.md`](BETA_OPEN.md)
- [`BETA_STATUS.md`](BETA_STATUS.md)
- [`FORGE_BETA_MIGRATION.md`](FORGE_BETA_MIGRATION.md)
- [`design/beta-open-forge-transition.md`](design/beta-open-forge-transition.md)
- [Founder Directives](FOUNDER_DIRECTIVES.md)
- [Invariants](INVARIANTS.md)
- [Failure Book](FAILURE_BOOK.md)
- [Threat Model](THREAT_MODEL.md)
- [Decision Log](DECISION_LOG.md)
- [Governance Index](governance/00_GOVERNANCE_INDEX.md)
- [External-review gate index](gates/README.md)
