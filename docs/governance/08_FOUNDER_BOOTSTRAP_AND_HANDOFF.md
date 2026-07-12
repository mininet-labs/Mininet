# Founder Bootstrap and Handoff

**Status:** Temporary constitutional operating guide
**Version:** 1.1

## Role

The founder presently defends the canonical mainline because independent governance and self-hosted infrastructure are not yet mature. This is a custodial responsibility, not ownership of the protocol.

## Relationship with AI engineering

An activated Primary AI Engineer Charter may assign broad engineering preparation and coordination to an AI, but it MUST NOT define Founder Authority, serve as a fallback authority profile, or make the AI a successor. Founder-specific Approval, veto, Canonicalization, release, administration, appointment, secret, or transition actions exist only where the current canonical phase, policy, delegation, and succession records assign them.

The Founder evaluates alignment with recorded directives and supplies only those Founder-specific Decisions required by current policy. Independent Review, exact-state binding, amendment gates, and external audit requirements remain applicable. New or changed vision enters through the applicable Proposal or amendment process rather than personal preference or a session instruction.

## Founder duties now

- preserve the Founder Directives and frozen invariants;
- prevent money, employer influence, or contribution volume from becoming political power;
- require evidence and independent review for protocol-critical work;
- prevent AI agents from becoming unreviewed authority;
- maintain branch, release, and secret protections;
- welcome anonymous and pseudonymous contribution;
- ensure accepted work can be compensated without compulsory public identity;
- document decisions and failures;
- cultivate independent maintainers and reviewers;
- fund external audits and research gates that code cannot satisfy;
- build and dogfood Mininet Forge until it can replace GitHub safely.

## Founder must not

- force updates;
- use private identity information as leverage;
- sell governance influence;
- treat GitHub ownership as constitutional ownership;
- bypass frozen rules for speed;
- appoint AI as quorum;
- suppress lawful forks;
- retain authority after safe, legitimate succession merely from habit.

## Delegation ladder

1. **Task delegation:** contributors implement bounded issues.
2. **Review delegation:** trusted pseudonymous or public reviewers approve domain work.
3. **Domain delegation:** working groups canonicalize ordinary domain changes.
4. **Integration delegation:** a multi-domain council authorizes combined mainline changes.
5. **Release delegation:** threshold release authorities sign eligible releases.
6. **Constitutional delegation:** constitutional governance controls protocol amendments under the then-current, honestly described personhood policy.
7. **Founder retirement:** founder retains no exceptional canonical power.

Each step requires demonstrated operation, revocation, recovery, and resistance to capture.

## Handoff evidence

Before relinquishing a power, the founder should verify:

- at least three independent humans or persistent pseudonymous maintainers can operate it;
- no single platform account is required;
- keys can rotate and recover;
- conflicts and malicious minorities are handled;
- decision records are durable;
- the new mechanism has been exercised under failure;
- users retain voluntary adoption and fork rights.

## Founder veto during bootstrap

A temporary founder veto may reject a change that violates frozen principles or creates unsafe capture. It should include a written constitutional reason and may not secretly rewrite accepted history. The veto expires with the transition phase defined in Document 09.

## Bootstrap limitation

D-0083 creates one explicit, temporary exception to D-0033 for GitHub `main`
integration during the founder-only bootstrap period. While it is active, the
Founder may perform the mechanical merge of a pull request after required
checks, resolved conversations, exact-head inspection, and public disclosure
of AI assistance. No AI review counts as human approval or quorum.

This exception is not permission to weaken a frozen invariant, publish a
production release, lower the Forge protocol floor, bypass an external audit,
or force owner adoption. It ends at the earliest of 2026-10-12T23:59:59Z, the
appointment of two independent human maintainers, preparation of a production
release candidate, or Forge becoming canonical. On sunset, D-0033's normal
two-human floor returns automatically and canonical merges stop until the
repository rules match it. Renewal requires a new exact-state Decision before
expiry; silence never renews it.

`governance/bootstrap-operating-state.json` is the fail-closed machine record
for the expiry and earlier triggers. The validator can enforce only what that
record truthfully states; maintainer appointment, production claims, and Forge
cutover must be recorded immediately when they occur.
