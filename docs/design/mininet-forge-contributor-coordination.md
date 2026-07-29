# Forge-native contributor coordination

**Status:** preliminary implementation report; D-0407 is proposed pending
Founder review and merge; issue [#266](https://github.com/mininet-labs/Mininet/issues/266)
tracks the code batch.

This is the first usable bridge from the Beta contributor routes to Mininet's
self-hosted Forge. It is deliberately a coordination layer, not a new
governance layer. The objects are signed with the existing `mini-objects`
builder and stored by `mini-store`, so they can move through the existing
`mini sync` path without GitHub being the authority.

## What is implemented

| Object / command | What it records | What it does not grant |
|---|---|---|
| `mini team propose` | A signed working-group charter proposal using the existing charter schema vocabulary; new proposals are labelled `proposed` or `incubating`. | No delegation, active team authority, maintainer appointment, or independence claim. |
| `mini task create` | A signed task brief linked to a Forge project and optionally a charter: route, descriptive risk, paths, acceptance evidence, and non-goals. | No assignment, competence proof, employer credential, or release eligibility. |
| `mini task suggest` | A bounded, deterministic local filter over verified task briefs by route/path. | No automatic recruitment, hidden profile matching, or canonical personnel directory. |
| `mini task claim` | A signed contributor-selected role, path scope, optional exact base, and expiry. | No ownership, priority, payment entitlement, review weight, or authority. |
| `mini task review` | A signed technical handoff bound to an exact reviewed object, with peer/external/AI evidence class, findings, evidence, and limitations. | No Forge approval. AI evidence has zero approval weight. |
| `mini task show` | A verified task graph containing its claims and technical handoffs. | No inference that a passing handoff is a merge, release, audit, or owner adoption decision. |

The commands support `--json` output for object IDs and exact-state links. A
typical local flow is:

```text
mini team propose <project> ...
mini task create <project> ...
mini task suggest --route rust --path crates/mini-forge/src/lib.rs
mini task claim <task-id> --role rust --path crates/mini-forge/** --expires-ms <future-ms>
mini task review <task-id> --claim <claim-id> --head <exact-object-id> ...
mini task show <task-id>
```

README's crate-range and contributor entry points are updated in this batch.
The generated `docs/_generated/` repo map is intentionally not committed here:
accepted D-0376 reserves generated-nav refreshes for a separate maintenance PR.
`python tools/mininet_nav.py map` was run against this tree as a read-only
navigation check.

The claim is explicit and participant-controlled. A client may make the
suggestion step feel seamless, but acceptance is not inferred from a profile,
employer, GitHub team, payment, commit count, or AI output.

## Preliminary decisions and alternatives

These are engineering decisions for the first slice, not a silent activation
of the proposed working-group governance RFCs.

1. **Forge objects are the durable coordination boundary.** This supplements
   D-0100's Git-era work-claim registry and composes existing signed object,
   store, identity-root verification, and sync primitives. It does not make
   GitHub history canonical and does not perform the Forge cutover described by
   RFC-0005.

2. **Suggestions are local and advisory.** The first matcher is a deterministic
   route/path filter over task briefs already present in a participant's store.
   This keeps personal skills, availability, privacy mode, and employment data
   out of the public object graph until issue #263 answers what may be exposed.
   The tradeoff is weaker global discovery; a later privacy-preserving index can
   be proposed separately.

3. **Claims expire and reviews bind exact state.** Expiry limits stale
   coordination evidence, while an exact reviewed-object link prevents a
   finding from being silently reattached to another revision. There is no
   claim-revocation or conflict-arbitration object in this slice; conflicting
   signed evidence remains visible and requires a future policy decision.

4. **Technical review is separate from approval.** `mini task review` never
   calls `mini pr approve` and has no approval bit. The existing Forge approval,
   canonical merge, release eligibility, installation, and typed owner adoption
   remain separate events.

### Alternatives considered

| Model | Benefit | Cost / failure mode | Preliminary disposition |
|---|---|---|---|
| GitHub-team-first | Familiar recruitment and notifications. | Platform admins, employers, or team labels can become shadow authority; work disappears or becomes ambiguous when GitHub disappears. | Rejected as the long-term source of truth; retained only as a temporary adapter. |
| Local-only task files | Strong privacy and simple offline UX. | No signed cross-home evidence; task handoffs cannot be independently checked after exchange. | Useful for private preferences, insufficient for shared Forge coordination. |
| Forge-native signed marketplace | Durable exact-state evidence and cross-client discovery. | More metadata leakage, spam, revocation, hidden-control, and policy risk; can accidentally become a personnel registry. | Destination for later policy work, not activated by this slice. |
| Automatic assignment | Low friction for newcomers. | Infers consent, skill, identity, availability, and authority; creates a single matcher that can capture routing. | Not implemented. Suggestions require explicit participant acceptance. |

## Team and direct-democracy boundary

The charter object uses the existing lifecycle vocabulary. The proposal command
accepts only `proposed` or `incubating`; later lifecycle labels can be read as
imported state but cannot be self-asserted by this command. A `proposed` or
`incubating` charter can organize work and advisory review. It is not a
delegation. The code does not implement role selection, terms, conflict
disclosure, inactivity suspension, revocation, appeals, cross-group
integration, or the formation gate requiring two independent persistent
participants before autonomous authority. Those are open policy/runtime
questions in #263 and the subordinate RFC-0003 material.

For group decision-making, the safe current interpretation remains:

```text
peer evidence -> external/public challenge -> authorized exact-state review
-> canonical release eligibility -> each owner's typed adoption choice
```

Public evidence is not a vote, a GitHub reaction, a payment, or forced
installation. A working group cannot amend frozen invariants, constitutional
meaning, money semantics, personhood claims, release policy, or owner-adoption
rights by itself. Use "identity root" for the current cryptographic unit; this
code does not prove unique personhood.

## Threats and failure points

- **Employer or funder capture:** no employer, payment, group size, or
  organization root appears in the object or suggestion rule.
- **Identity-root Sybil pressure:** signatures establish an identity root and
  delegated device provenance only; they do not prove a unique human.
- **Review cartel or stale claim:** reviews bind an exact object, claims carry
  an expiry, and conflicting records are not silently merged. Revocation and
  independence policy remain future work.
- **AI self-approval:** AI is a review-evidence classification only and has no
  path into Forge quorum counting.
- **Privacy leakage:** task briefs are public signed metadata when shared;
  contributor preferences are not stored or published by this slice. Signature
  privacy is not anonymity.
- **GitHub disappearance:** the object graph and CLI do not require a GitHub
  account. Existing `mini sync` exchange is the transport path; the live
  multi-machine coordination exercise is still a follow-up.

## What is not built or audited

This batch does not build or claim:

- automatic recruitment, a canonical contributor/personnel/employer directory,
  or private cross-device matching;
- role delegations, group activation, group quorum, direct-democracy policy,
  public-confirmation policy, or Forge cutover;
- claim revocation, conflict arbitration, anti-spam, Sybil resistance, or
  independence proofs;
- a Forge UI/daemon, Android/mobile task surface, or background sync;
- an external cryptography, legal, governance, mechanism-design, privacy,
  personhood, or security audit;
- production readiness, release eligibility, installation, owner adoption,
  value custody, or consensus authority.

The tests prove local Rust object encoding/verification and a two-home shared
store CLI flow. They do not prove real hardware, a live multi-machine network,
external review, unique humans, or canonical governance.

## Outstanding questions and owners

- [#263](https://github.com/mininet-labs/Mininet/issues/263): minimum routing
  data, privacy modes, role continuity, conflict disclosure, group lifecycle,
  and the meaning of public confirmation. Founder/governance disposition is
  required before activating authority.
- [#262](https://github.com/mininet-labs/Mininet/issues/262): external-review
  closure matrix and evidence packet. Passing local tests or an AI review does
  not close any external gate.
- [#264](https://github.com/mininet-labs/Mininet/issues/264): Beta-facing
  contributor routes and handoff templates, currently in the separate draft
  documentation batch.
- RFC-0005 follow-up: independent Forge operators, identity rotation/recovery,
  matching GitHub/Forge state during dual-running, outage exercise, export/fork
  exercise, and a governed cutover decision.

If a future implementation needs a new authority, value edge, privacy claim,
or policy not covered by the canon, stop at that boundary and ask the Founder
or applicable governance authority. Do not extend this coordination object
vocabulary by implication.
