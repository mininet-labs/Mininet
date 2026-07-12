# Repository Enforcement Architecture

**Status:** Normative bootstrap specification  
**Version:** 0.4

## 1. Purpose

This specification maps Mininet governance requirements onto the temporary GitHub mirror without treating GitHub as the constitutional source of truth. Repository automation is a bootstrap enforcement adapter. It MUST fail closed on missing evidence where the repository can verify that evidence, and it MUST label unenforceable claims rather than pretending they are proved.

## 2. Enforcement layers

1. **Human-readable constitution** — Founder Directives, invariants, accepted decisions, governance specifications.
2. **Machine-readable policy** — path classes, review floors, evidence requirements, protected terms, exception records.
3. **Proposal metadata** — exact change classification, AI disclosure, affected invariants, tests, audit status and compensation relation.
4. **Repository controls** — rulesets, CODEOWNERS, permissions, protected environments and merge queue.
5. **CI validation** — schema checks, path-to-owner checks, required metadata and contradiction checks.
6. **Forge target** — signed proposal, review, approval, build and release objects enforcing equivalent rules without GitHub.

## 3. Constitutional precedence

Repository automation MUST NOT create authority beyond the higher documents. If automation disagrees with a frozen invariant, the automation is wrong. If two lower-level policies conflict, the more restrictive valid policy applies until governance resolves the conflict.

## 4. Change classes

| Class | Typical scope | Minimum review |
|---|---|---|
| `documentation` | prose without normative or security effect | one independent reviewer |
| `ordinary` | implementation not touching a protected domain | one independent reviewer during bootstrap |
| `protocol-critical` | identity, consensus, forge governance, update, release, treasury or constitutional enforcement | two distinct authorized reviewers |
| `cryptography-sensitive` | primitives, protocols, proofs, custody, privacy or key lifecycle | two qualified reviewers; external gate before production claims |
| `constitutional` | frozen invariant, constitutional interpretation or amendment | constitutional process; never ordinary merge authority |
| `emergency` | narrowly scoped response to active exploitation or integrity failure | emergency policy plus mandatory retrospective |

Path classification is a routing aid, not proof that a change is safe. Proposal authors and reviewers MUST raise classification when semantics require it.

## 5. Required proposal evidence

Every proposal MUST declare:

- immutable proposal state or commit digest;
- change class;
- affected directives, invariants and decisions, or an explicit `none` explanation;
- test evidence and negative tests;
- AI assistance and the accountable authorization path;
- dependency changes;
- security and privacy effects;
- release/adoption effect;
- compensation or bounty relation, which MUST NOT affect approval authority.

A proposal MAY preserve contributor anonymity. GitHub account metadata is platform metadata, not compulsory Mininet identity.

## 6. CI truth boundary

CI MAY verify formatting, schemas, files, metadata, tests, dependency policy and exact commit state. CI cannot establish that a reviewer is a unique human, that an external audit is competent, or that a pseudonym maps to a legal person. Such claims MUST remain evidence objects or governance decisions, not inferred facts.

## 7. Fail-closed rules

The governance check MUST fail when:

- required proposal metadata is absent;
- protected paths lack their declared review class;
- a Tier-F change omits invariant and decision references;
- AI-authored sensitive work claims AI approval as quorum;
- a forced-update or owner/admin path is declared or detected by explicit policy patterns without an approved constitutional exception;
- dependency exceptions have no owner or expiry;
- a normative governance document omits a stable ID or authority class.

## 8. Bootstrap limitations

GitHub branch protection counts GitHub reviews, not Mininet signed review objects. CODEOWNERS routes review but does not establish competence. Actions execute on centralized infrastructure. These mechanisms reduce accidental and unilateral change during bootstrap; they do not replace Forge governance.

## 9. Migration requirement

Every GitHub-specific control MUST have a target Forge equivalent recorded in `governance/policy.yml`. A control may be retired only after the Forge equivalent is implemented, adversarially tested, governed and recoverable without GitHub.
