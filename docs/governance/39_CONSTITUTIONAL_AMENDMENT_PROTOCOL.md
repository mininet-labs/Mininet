# Constitutional Amendment Protocol

**Status:** Normative

**Version:** 1.0

## Normative interpretation

This document is subordinate to `docs/FOUNDER_DIRECTIVES.md`, frozen invariants, and accepted decisions in the canonical Mininet repository. Where this package differs from canonical project authority, the canonical repository prevails until a governed amendment explicitly adopts the new rule.

Identity disclosure is never presumed. Anonymous, pseudonymous, and public participation remain valid unless a narrowly scoped role requires continuity or delegated authority. Persistent cryptographic accountability does not imply legal-name disclosure.

## Purpose

This specification defines how Mininet may change its governance without allowing temporary majorities, infrastructure operators, wealth, or emergency actors to silently rewrite constitutional commitments.

## Amendment classes

| Class | Scope | Minimum process |
|---|---|---|
| Editorial | Meaning-preserving corrections | exact-state review, ordinary documentation approval |
| Operational | Repository, workflow, or delegation procedure | working-group review, integration review, cooling period |
| Constitutional | Sovereignty, legitimacy, voting, owner consent, forking, identity, treasury separation | formal amendment proposal, independent review, broad governance decision, extended cooling period |
| Frozen-invariant change | Any proposal affecting a frozen invariant | prohibited unless canonical constitutional authority already defines a lawful unfreezing process |

Classification MUST be based on effect, not title or file location.

## Required amendment object

An amendment proposal MUST bind:

- exact parent constitution digest;
- exact proposed text digest;
- amendment class;
- affected directives, invariants, decisions, specifications, and code surfaces;
- rationale and alternatives;
- security, privacy, economic, minority, and owner-consent analysis;
- migration and rollback plan;
- proposed activation condition;
- review and decision records.

## Constitutional process

1. **Publication.** The immutable proposal is published without authority to activate itself.
2. **Classification.** Independent reviewers confirm its amendment class.
3. **Adversarial review.** At least one review argues against adoption and tests capture, coercion, ambiguity, and failure cases.
4. **Public reasoning period.** Participants may submit evidence anonymously or pseudonymously.
5. **Exact-state approval.** Every approval names the exact amendment digest.
6. **Cooling period.** Constitutional changes cannot activate immediately after decision.
7. **Finality check.** The activation record verifies the approved digest, authority, threshold, time conditions, and absence of superseding decisions.
8. **Migration.** Existing authority, data, keys, and rights transition according to the published plan.
9. **Post-activation review.** Governance records unexpected effects without retroactively altering history.

## Entrenchment

The following principles MUST receive the strongest available protection:

- money does not purchase political authority;
- owner adoption is voluntary;
- legal identity is not a general condition of participation;
- anonymous and pseudonymous contribution remains possible;
- AI evidence does not become self-authorizing governance power;
- free forking and exit remain available;
- no permanent founder, administrator, or platform dependency is introduced.

## Emergency limitation

An emergency actor MAY pause a narrowly defined process when irreversible harm is imminent, but MUST NOT amend the Constitution, transfer treasury ownership, force adoption, reveal identities, or convert temporary authority into permanent authority.

## Fork protection

A rejected minority MAY preserve the proposal and form a fork. The canonical network MAY reject its legitimacy, but MUST NOT prevent possession of code, data, identity keys, or independently controlled assets.
