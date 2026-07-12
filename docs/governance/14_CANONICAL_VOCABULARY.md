# Canonical Governance Vocabulary

**Status:** Normative terminology profile  
**Version:** 0.3

## Relationship to the ontology

This document remains the concise compatibility vocabulary for the v0.2 pack. `16_PROTOCOL_ONTOLOGY.md` is the fuller normative ontology. Where wording differs, the ontology controls unless a higher authority controls.

## Rule

The terms below have one meaning throughout this governance pack. Platform-specific terms may be used in examples, but must not replace the constitutional concept.

## People, agents, and continuity

**Participant** — Any human, organization, pseudonym, anonymous session, or automated agent interacting with Mininet development.

**Anonymous participant** — A participant whose actions are not linked by protocol requirement to a persistent identity. Anonymous participation may submit work and receive a privacy-preserving payment, but cannot accumulate continuity-dependent authority without choosing a persistent key.

**Pseudonymous identity** — A persistent cryptographic identity that need not reveal a legal or public identity. It may accumulate evidence, reputation, delegated authority, and payment history.

**Public identity** — A pseudonymous or organizational identity voluntarily linked to a public person or institution. Public attribution grants no correctness privilege.

**AI agent** — An automated system that may create proposals, analysis, tests, attacks, or review evidence. It does not satisfy a human approval or governance quorum unless a future constitutional amendment explicitly creates a different class of authority.

## Work and evidence

**Change Proposal** — A content-bound request to alter code, specifications, research, policy, documentation, or operational configuration.

**Exact proposal state** — The precise digest or commit reviewed. Material modification invalidates approvals that do not bind the new state.

**Evidence** — Verifiable support for a claim, including tests, proofs, provenance, benchmarks, threat analysis, independent review, interoperability results, and external audit reports.

**Review** — Analysis of a proposal. Review may be performed by a human or AI and may produce useful evidence.

**Approval** — An authorized decision accepting a precise proposal state. AI review is not an approval. The author does not count as an independent approver of their own work.

**Adversarial review** — A deliberate attempt to falsify assumptions, break invariants, discover attack paths, or find integration failure.

**Integration** — Combining accepted component work and testing the combined state. Feature completeness is not integration completeness.

## Authority and legitimacy

**Authority** — Scoped permission to perform an action. Authority is not evidence of correctness and must be reviewable and revocable.

**Responsibility acceptance** — A signed or otherwise durable statement by an authorized human or persistent pseudonymous governor that they reviewed the exact proposal state and accept the defined governance responsibility. It does not require public legal identity.

**Reputation** — Accumulated evidence associated with an identity. Reputation may inform delegation but must not silently become money-weighted political power.

**Legitimacy** — The reason a change is accepted as part of Mininet's continuous canonical history.

**Canonicalization** — The authorized state transition by which a proposal becomes part of canonical project history. During bootstrap this is represented by protected `main`; later it is represented by governed Forge objects and chain continuity.

**Canonical History** — The continuous, verifiable record of legitimate Mininet evolution. A hosting platform may mirror it but does not own it.

**Founder** — The current bootstrap guardian. This is a temporary authority role, not permanent ownership of Mininet.

**Maintainer** — A persistent public or pseudonymous identity delegated review or integration authority for a defined scope.

**Working Group** — A governed domain body with a charter, scope, maintainers, review policy, succession mechanism, and conflict rules.

## Releases and user choice

**Governed Release** — A release candidate that satisfies the applicable source, review, build, provenance, transparency, freshness, and governance gates.

**Availability** — A release or optional service can be discovered or downloaded. Availability is not activation.

**Owner Approval** — The owner's explicit authorization naming the exact release or policy to be activated.

**Owner Adoption** — The local act of activating an approved, re-verified release. Governance may recommend or warn; it may not force activation.

**Refusal** — A valid owner outcome in which an available release is not adopted.

**Fork** — A technically free continuation from copied code or history. A fork does not automatically inherit Mininet's canonical legitimacy, identity continuity, treasury, release chain, or community recognition.

## Identity honesty

**Identity root** — A verified `did:mini` root. It must not be described as a verified unique human.

**Personhood** — Evidence sufficient under then-current constitutional policy to treat one participant as one unique human for a defined purpose. This remains an unresolved and evolving mechanism until implemented and externally examined.

## Platform mappings

| Constitutional concept | GitHub bootstrap | Mininet Forge target |
|---|---|---|
| Change Proposal | Pull request | Signed proposal object |
| Review | Review/comment/check | Signed review/finding object |
| Approval | Required PR approval | Governed approval object |
| Integration | Integration branch/merge queue | Integration proposal and combined evidence |
| Canonicalization | Protected merge into `main` | Governed branch-head transition |
| Build evidence | CI checks/artifacts | Signed execution result and provenance |
| Governed Release | Protected release workflow | Governed release object and transparency record |
| Owner Adoption | Installer approval | Local owner-approved activation |
