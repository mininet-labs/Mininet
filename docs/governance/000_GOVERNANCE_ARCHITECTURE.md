# Architecture of Mininet Governance

**Status:** Normative architecture profile  
**Version:** 0.3  
**Authority:** Subordinate to `SPEC-00`, frozen invariants, and accepted decisions

## 1. Purpose

This document defines where governance rules belong, how they acquire authority, how they are translated into enforceable policy, and how Mininet can change collaboration platforms without changing its constitutional meaning.

Governance is treated as a protocol. It has actors, objects, state transitions, authorization rules, evidence, failure modes, recovery paths, and tests. Prose that cannot be mapped to those elements is explanatory rather than enforceable.

## 2. Architectural layers

Mininet governance is organized into eight layers:

1. **Constitutional authority** — `SPEC-00` and its canonical register.
2. **Frozen invariants** — rules that implementation and lower governance may not violate.
3. **Accepted decisions** — append-only choices made within constitutional limits.
4. **Ontology and normative language** — one meaning for reserved governance terms.
5. **State machines and governance specifications** — permitted transitions and required evidence.
6. **Operational policy** — repository, Forge, working-group, release, and treasury procedures.
7. **Enforcement mechanisms** — branch rules, signed objects, policy engines, CI, provenance, and local verification.
8. **Evidence and tests** — proof that the claimed enforcement exists and survives adversarial conditions.

A lower layer MUST NOT silently redefine a higher layer. If implementation conflicts with a higher layer, the conflict MUST be reported and either the implementation or the governing rule MUST be changed through its proper amendment process.

## 3. Sources of truth

During bootstrap, GitHub may be the operational place where canonical changes are recorded. GitHub is not constitutional authority. The intended durable source of truth is a content-addressed, cryptographically authorized, continuously auditable history capable of being reproduced independently of one host.

The current repository states that GitHub is a temporary public mirror and that Mininet Forge is intended to become content-addressed and self-governed. This architecture therefore distinguishes:

- **constitutional authority** from hosting;
- **canonical history** from one branch name;
- **proposal transport** from proposal legitimacy;
- **release availability** from owner adoption.

## 4. Governance objects

A complete governance implementation SHOULD support immutable, content-bound objects for:

- proposal;
- review finding;
- approval;
- responsibility acceptance;
- integration result;
- build execution result;
- provenance;
- governance decision;
- release proposal;
- release authorization;
- bounty offer;
- contribution claim;
- dispute;
- payment authorization;
- delegation;
- revocation;
- emergency action;
- constitutional amendment.

Every signed object MUST bind the exact state it evaluates or authorizes. A signature over one proposal state MUST NOT authorize later modifications.

## 5. Core separation of powers

Mininet governance separates five powers:

- **proposal power** — permission to submit work;
- **review power** — permission to produce analysis and evidence;
- **approval power** — permission to accept a defined state within a scope;
- **canonicalization power** — permission to modify canonical history;
- **adoption power** — the owner's local authority to activate software or policy.

No actor receives all five merely because they hold one. In particular:

- authors MUST NOT satisfy independent approval requirements for their own work;
- AI output MAY support review but MUST NOT satisfy a human or authorized governance quorum;
- governance MAY make a release canonical but MUST NOT force owner adoption;
- repository administrators MUST NOT be treated as constitutional authorities merely because a platform grants them technical access.

## 6. Identity, privacy, and continuity

Contribution MUST NOT require public legal identity. Participants MAY be anonymous, pseudonymous, public, organizational, or automated.

The required continuity depends on the action:

- submitting a proposal or bounty claim MAY be anonymous;
- accumulating reputation or delegated authority requires a persistent cryptographic identity;
- high-risk authority requires durable key continuity, revocation, and succession rules;
- public attribution is voluntary unless an external service is separately and explicitly chosen by the participant.

Mininet holds actions and authorities accountable through signatures, evidence, and scope. It does not make legal identity a universal prerequisite.

## 7. Platform adapters

GitHub and Mininet Forge are adapters implementing the same constitutional concepts.

| Constitutional object | GitHub bootstrap | Mininet Forge target |
|---|---|---|
| Change Proposal | Pull request | Signed proposal object |
| Exact state | Commit SHA | Content digest |
| Review | Review/check/comment | Signed review object |
| Approval | Required PR approval | Governed approval object |
| Integration | Integration branch or merge queue | Combined integration proposal |
| Canonicalization | Protected merge to `main` | Governed branch-head transition |
| Build evidence | Actions result and artifacts | Signed execution result and provenance |
| Release | Protected release workflow | Governed release object |
| Adoption | Local installer action | Local owner-approved activation |

A platform adapter is conformant only if it preserves the required state, authority, and evidence semantics.

## 8. Specification form

Normative governance specifications SHOULD use this structure:

1. Purpose
2. Scope
3. Definitions
4. Actors
5. Authorities
6. Preconditions
7. Inputs
8. State transition
9. Required evidence
10. Failure conditions
11. Recovery or rollback
12. Audit events
13. Privacy effects
14. Constitutional traceability
15. Test scenarios

## 9. Amendment classes

Changes are classified as:

- **constitutional** — changes participant sovereignty, political equality, owner consent, or the source of legitimacy;
- **protocol-governance** — changes governance objects, thresholds, delegations, or state transitions;
- **operational** — changes present platform procedures without changing constitutional meaning;
- **editorial** — clarifies without changing normative effect.

A lower-class process MUST NOT be used to smuggle in a higher-class change.

## 10. Machine-readable direction

Each governance document SHOULD eventually expose a machine-readable summary containing:

- document identifier and version;
- authority class;
- definitions introduced;
- actors and actions;
- invariants referenced;
- required evidence;
- supersedes and superseded-by links;
- implementation mappings;
- test scenario identifiers.

Machine-readable summaries aid validation but MUST NOT silently replace the human-readable normative text until a constitutional decision explicitly promotes a machine format.

## 11. Completion criterion

This architecture is successful when a future contributor can trace any governance action through:

`term -> authority -> state transition -> evidence -> enforcement -> test -> canonical record`

without depending on oral tradition, private chat, one platform, or one individual.
