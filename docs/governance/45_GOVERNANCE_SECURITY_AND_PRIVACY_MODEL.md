# Governance Security and Privacy Model

**Status:** Normative security model

**Version:** 1.0

## Normative interpretation

This document is subordinate to `docs/FOUNDER_DIRECTIVES.md`, frozen invariants, and accepted decisions in the canonical Mininet repository. Where this package differs from canonical project authority, the canonical repository prevails until a governed amendment explicitly adopts the new rule.

Identity disclosure is never presumed. Anonymous, pseudonymous, and public participation remain valid unless a narrowly scoped role requires continuity or delegated authority. Persistent cryptographic accountability does not imply legal-name disclosure.

## Protected properties

Governance security protects:

- exact-state decision integrity;
- authority scope and expiration;
- auditability without compulsory legal identity;
- contributor and payout unlinkability where explicitly claimed and implemented;
- owner consent;
- fork and exit rights;
- resilience to platform, founder, working-group, treasury, and AI capture.

## Threat actors

Threat actors include founders, maintainers, contributors, AI agents, employers, funders, builders, repository hosts, network peers, auditors, governments, attackers, and colluding subsets of any group.

No role is trusted merely by title.

## Privacy levels

| Level | Meaning |
|---|---|
| Public | Identity and action intentionally linked |
| Persistent pseudonymous | Stable cryptographic identity; legal identity undisclosed |
| Unlinked contribution | Contribution not intentionally linked to other activity |
| Selectively disclosed | Holder proves chosen facts to chosen parties |
| Anonymous claim | Protocol aims to avoid public linkage between contribution and claimant |

A signature proves control of a key; it does not by itself prove anonymity, uniqueness, humanity, or legal identity.

## Metadata caution

Timing, network routing, build infrastructure, issue discussion, payment settlement, writing style, and cross-platform reuse may deanonymize participants. Governance documentation MUST NOT describe application-layer pseudonyms as full anonymity unless the complete operational path is assessed.

## Coercion and retaliation

The system SHOULD permit evidence submission, review, whistleblowing, and bounty participation without compulsory public attribution. Safety mechanisms MUST avoid creating a universal identity registry that becomes a coercion target.

## Security claims

Unaudited cryptography MUST remain labelled experimental. Governance acceptance MUST NOT transform an unreviewed construction into a security fact.
