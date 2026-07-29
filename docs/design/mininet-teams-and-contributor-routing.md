# Preliminary report: Mininet-native teams and guided contributor routing

**Status:** preliminary engineering proposal; not activated governance

**Decision record:** D-0101 (Proposed)

**Tracking:** [#263](../../issues/263) for policy and design questions;
[#264](../../issues/264) for the Beta-facing adapter; [#262](../../issues/262)
for external-review closure evidence.

This report answers a Beta-readiness question: how can employees,
consultants, independent contributors, testers, reviewers, maintainers, and
security participants find useful work and participate confidently on GitHub
now and in Mininet Forge later? It is deliberately a proposal. It does not
create a new authority class, activate a working group, or claim that a
matching service exists.

## Recommendation in one paragraph

Use an opt-in, local-first task router as the first product surface. A
participant declares only the skills, interests, availability, privacy mode,
and confidence they choose to share. The client suggests bounded task briefs;
the participant accepts a task explicitly. A Mininet-native working group is
represented by a signed, content-addressed charter with scoped, expiring, and
revocable role delegations. GitHub issue labels, teams, and Projects remain
temporary display and coordination adapters. They never become protocol
authority, and they never turn an employer, payment, organization root, or
AI output into a governance right.

This is the smallest path that makes recruitment feel guided without making
the protocol infer a person's identity, employment, competence, consent, or
authority from platform metadata.

## What this batch can safely create

The Beta adapter can create discoverable routes, task briefs, issue forms, and
evidence handoffs now. It does not create a GitHub organization-team roster or
grant team permissions: no founder-approved membership, role scope, conflict
process, or platform-team policy is recorded yet. The Forge-native charter
schema and example remain reusable design artifacts, but a real Mininet team
would need a signed charter and independent persistent participants before it
could represent anything beyond ordinary coordination. This boundary is an
open policy question in [#263](../../issues/263), not an implementation gap to
paper over.

## Canonical constraints already decided

| Constraint | Consequence for teams and recruitment |
|---|---|
| Directive 12, AI1 | AI may suggest, implement, test, or challenge. AI review carries zero approval weight and cannot appoint a maintainer. |
| P1 / Directive 16 | Payment, sponsorship, employer status, hardware, fame, or commit volume cannot buy voice, review standing, merge authority, or governance weight. |
| P2 honesty limitation | Current code counts distinct verified identity roots. It does not prove a unique human. Product copy and quorum explanations must say identity root. |
| INV-18-08 | An organization's root identity is never governance-eligible. A person may contribute through an employer; an organization cannot inherit the person's voice. |
| Working-group charter | A group has a domain, lifecycle, delegated scope, terms, appeals, conflict policy, and sunset review. One-person groups may organize work but cannot claim independent governance. |
| Cross-group council | Cross-domain changes require affected-domain evidence and integration handling; group size and funding do not create extra votes. |
| D-0100 | GitHub Projects and the work-claim registry coordinate work. A claim is not approval, legitimacy, canonicalization, or release authority. |
| Release and owner adoption | Public evidence may make an exact release eligible for adoption. Each owner still chooses whether to install or activate it through typed owner approval. |
| A1 / #262 | Passing tests, founder review, and AI review are not an external cryptography audit. Prototype value, custody, consensus, and personhood claims remain gated. |

These constraints come from the canon and the subordinate Governance Pack;
they are not new policy invented by this report.

## The participant experience

The intended path is:

`discover -> choose -> claim -> build/test -> peer review -> authorized review -> integration -> release -> owner adoption`

At Beta, the first three steps can be made smooth with GitHub templates and a
front-page guide:

1. **Discover:** choose a route such as Beta tester, Rust contributor,
   Android/device tester, documentation/research contributor, security-review
   coordinator, or governance/process researcher.
2. **Choose:** answer a short, privacy-preserving intake. The participant may
   be anonymous, pseudonymous, or public. The intake explains what the task
   needs and what it does not grant.
3. **Claim:** select an existing issue and branch. A work claim records the
   expected paths and Decision identifier so parallel contributors do not
   collide. Claiming is explicit; no automation silently assigns a person.
4. **Build/test:** follow the task brief and record the exact revision,
   environment, evidence, and limitations. Rust-only evidence is kept
   separate from Android or physical-device evidence.
5. **Review and handoff:** peer review evaluates the exact state. Authorized
   review, merge, release, installation, and adoption remain distinct events.

The future Forge client can perform the same routing without GitHub. It should
be able to suggest work from signed task briefs, domain paths, dependencies,
risk class, and declared evidence needs without becoming a canonical personnel
directory.

## Team and role model

The existing Governance Pack already names the useful roles:

- **Contributor:** submits code, research, tests, documentation, or evidence;
  legal identity is not required by default.
- **Reviewer:** evaluates a defined domain and exact state; persistent
  cryptographic continuity may be required for review continuity, without
  requiring a public legal identity.
- **Maintainer:** coordinates domain state, routing, and integration; this is
  responsibility, not ownership or political weight.
- **Integration representative:** carries a group's interface assumptions into
  cross-group integration.
- **Security steward:** coordinates sensitive findings and external audit
  work under an appropriate privacy boundary.

Every working group should use the existing
[`working-group-charter` schema](../../forge-native/schemas/working-group-charter.schema.json)
and lifecycle. A charter should identify:

- immutable group identifier, purpose, domain paths, and dependencies;
- autonomous implementation actions and reserved actions;
- reviewer/maintainer roles, terms, conflict handling, and appeals;
- evidence and reporting obligations;
- formation, suspension, rotation, split, merge, and retirement conditions.

An employee or consultant can participate in any of these roles. The employment
relationship belongs in a conflict/compensation disclosure when relevant; it
does not become a protocol credential. An organization may sponsor work or
operate an edge service, but its root is not governance-eligible and it cannot
transfer a person's voice to an employer, estate, client, or funding source.

## Alternatives considered

| Model | Description | Benefits | Costs and failure modes |
|---|---|---|---|
| GitHub-team-first | GitHub teams and Project fields are the recruitment directory; Mininet mirrors membership later. | Lowest implementation cost; familiar notifications and routing; easy Beta operations. | Platform dependence; team membership can look like authority; employer or admin control can become shadow governance; poor continuity if GitHub disappears. Rejected as the long-term source of truth. |
| Local-first guided router | The participant keeps preferences locally and receives suggestions for signed task briefs; only the accepted claim and necessary evidence leave the device. | Minimizes personal-data collection; works offline; preserves consent and sovereignty; does not require a canonical personnel directory. | Discovery is less global; task quality depends on well-written briefs; no automatic proof of competence; privacy-preserving cross-device matching is not yet designed. Recommended for Beta. |
| Forge-native signed marketplace | Forge stores signed contributor preferences, task briefs, delegations, group charters, reviews, and reports; clients query or locally filter them. | Durable across GitHub loss; exact-state evidence and delegation can be verified; supports large-scale routing. | More schemas, metadata exposure, revocation, spam, and privacy work; can accidentally become a canonical identity/employer registry; requires policy and external challenge before activation. Recommended as the destination, not a Beta prerequisite. |

The recommendation is a staged hybrid: GitHub templates and a local-first UX
now, with Forge-native objects only after #263 answers the policy questions
and the external challenge in #262 has a closure-ready scope.

## Group direct democracy and public confirmation

The phrase “direct democracy in groups” needs a bounded interpretation. The
existing pack permits a group to decide ordinary implementation matters inside
accepted specifications. It does not let a group amend frozen invariants,
constitutional meaning, money semantics, personhood claims, release policy, or
owner-adoption rights on its own.

Three interpretations were considered:

1. **Group-local decisions only.** Peers decide ordinary implementation work;
   cross-domain changes go to integration; constitutional changes use the
   higher process. This matches the current pack and is the safest Beta
   baseline.
2. **Public vote on every release.** Everyone confirms every update before it
   installs. This sounds democratic but confuses public review with owner
   adoption, creates an undefined voter-eligibility policy, and risks turning
   release availability into forced social coordination. Not recommended.
3. **Evidence, challenge, eligibility, adoption.** Peers review an exact
   proposal; external/public challenge records findings; authorized release
   evidence makes a release eligible; each device owner makes an explicit
   typed adoption choice. This composes the current review and release docs
   and is the recommended interpretation.

Under the recommendation, “public confirmation” means transparent evidence,
public challenge, and visible release state unless a future accepted policy
defines a narrower governance act. It does not mean a GitHub reaction, a
working-group count, a payment, or an install forced by a remote party.

## Questions that remain open

These require an owner or applicable governance decision; this report does not
answer them by fiat:

- What is the minimum data a router may expose, and what must remain local?
- Should a future Forge index contain signed preferences, or only signed task
  briefs that clients filter locally?
- Which reviewer/maintainer roles need persistent continuity, and what is the
  smallest revocable delegation that suffices?
- What evidence demonstrates that a group is independent enough to move from
  incubating to active when only one employer or one key lineage is available?
- How are employer, funder, client, and close-control conflicts disclosed while
  preserving contributor privacy?
- Is “public confirmation” limited to challenge and release transparency, or
  does a future policy require a separate exact-state governance decision?
- What is the smallest task/profile vocabulary that supports routing without a
  canonical personnel or organization registry?

Issue [#263](../../issues/263) is the decision record for these questions.

## Threats and mitigations

- **Employer or funder capture:** keep affiliation disclosure separate from
  identity and authority; rotate roles; count neither payment nor group size as
  governance weight.
- **Identity-root Sybil pressure:** never market routing or group membership as
  proof of a unique human; use the current identity-root terminology.
- **Review cartel or inactive delegation:** bind reviews to exact state, use
  expiring/revocable terms, preserve appeals and dissent, and reroute a
  suspended group.
- **Retaliation or privacy leakage:** allow anonymous/pseudonymous contribution
  where policy permits; collect only the data needed for the task; provide a
  private security path.
- **AI self-approval:** keep AI findings as evidence and exclude them from
  approval/quorum weight.
- **GitHub disappearance:** make the guide and templates adapters; keep task,
  review, release, and adoption concepts mapped to Forge-native objects.
- **False Beta confidence:** bind reports to exact revisions and device
  evidence, and state what was not tested, audited, or built.

## What is not built or audited

This report does not claim any of the following exists:

- a Mininet runtime working-group object or delegation engine;
- an automatic recruitment or task-matching service;
- a canonical personnel, employer, or organization registry;
- private cross-device matching or a public-confirmation protocol;
- a real Android/BLE Beta acceptance result;
- an external cryptography, legal, mechanism-design, or personhood audit;
- production value, custody, personhood, consensus, or release readiness.

The existing working-group schema/example is a Forge-native design artifact,
not activated enforcement. The external-review response template is a handoff
tool, not an audit result.

## References

- [`docs/governance/11_WORKING_GROUPS_AND_MAINTAINERS.md`](../governance/11_WORKING_GROUPS_AND_MAINTAINERS.md)
- [`docs/governance/33_WORKING_GROUP_CHARTER_AND_LIFECYCLE.md`](../governance/33_WORKING_GROUP_CHARTER_AND_LIFECYCLE.md)
- [`docs/governance/35_CROSS_GROUP_INTEGRATION_COUNCIL.md`](../governance/35_CROSS_GROUP_INTEGRATION_COUNCIL.md)
- [`docs/governance/36_SCALING_FROM_TWO_TO_THOUSANDS.md`](../governance/36_SCALING_FROM_TWO_TO_THOUSANDS.md)
- [`docs/governance/41_EXTERNAL_REVIEW_AND_PUBLIC_CHALLENGE.md`](../governance/41_EXTERNAL_REVIEW_AND_PUBLIC_CHALLENGE.md)
- [`docs/governance/07_RELEASE_AND_OWNER_ADOPTION.md`](../governance/07_RELEASE_AND_OWNER_ADOPTION.md)
- [`docs/governance/28_FORGE_NATIVE_GOVERNANCE_OBJECTS.md`](../governance/28_FORGE_NATIVE_GOVERNANCE_OBJECTS.md)
- [`forge-native/examples/working-group-charter.example.json`](../../forge-native/examples/working-group-charter.example.json)
- [`docs/INVARIANTS.md`](../INVARIANTS.md)
- [`docs/THREAT_MODEL.md`](../THREAT_MODEL.md)
- [`docs/FAILURE_BOOK.md`](../FAILURE_BOOK.md)
