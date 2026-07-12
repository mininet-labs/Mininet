# Cross-Group Integration Council

**Status:** Normative coordination specification

## 1. Purpose

The Integration Council protects interfaces and system-wide invariants when no single working group has complete context.

It is not an upper house that owns all development.

## 2. Composition

Each active technical group may delegate an integration representative. Representatives are bound to exact terms and may be replaced. Additional security, release, and constitutional representatives may be required for sensitive changes.

AI systems may prepare integration analysis but do not satisfy the deciding quorum.

## 3. Scope

The Council coordinates:

- cross-crate and cross-protocol interfaces;
- changes spanning several working groups;
- integration branches or Forge integration objects;
- system-wide test plans;
- release readiness;
- ownership of orphaned boundaries;
- conflict resolution between valid domain decisions;
- migration sequencing.

It does not override a domain group on isolated implementation details without showing a cross-domain effect.

## 4. Integration proposal

A cross-group proposal MUST identify:

- affected interfaces;
- domain approvals;
- unresolved disagreements;
- combined evidence;
- migration/compatibility plan;
- rollback or fork behavior;
- release implications.

## 5. Quorum

Quorum SHOULD count affected independent domains, not raw participant numbers. A group with many contributors does not gain multiple votes merely through size or funding.

High-risk changes require security and constitutional review in addition to affected-domain approval.

## 6. Deadlock

Deadlock resolution proceeds through:

1. narrowed interface proposal;
2. independent technical mediation;
3. time-bounded experiment or competing implementation;
4. governance escalation for constitutional trade-offs;
5. legitimate fork where incompatible values remain.

Delay alone MUST NOT silently approve a change.

## 7. Evidence

Council decisions MUST bind to exact integration results and combined tests. Individual feature approvals cannot be reused after incompatible integration changes.
