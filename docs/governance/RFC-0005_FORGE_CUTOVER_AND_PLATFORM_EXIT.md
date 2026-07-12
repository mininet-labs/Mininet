# RFC-0005: Forge Cutover and Platform Exit

**Status:** Proposed normative RFC

**Version:** 1.0

## Normative interpretation

This document is subordinate to `docs/FOUNDER_DIRECTIVES.md`, frozen invariants, and accepted decisions in the canonical Mininet repository. Where this package differs from canonical project authority, the canonical repository prevails until a governed amendment explicitly adopts the new rule.

Identity disclosure is never presumed. Anonymous, pseudonymous, and public participation remain valid unless a narrowly scoped role requires continuity or delegated authority. Persistent cryptographic accountability does not imply legal-name disclosure.

## Abstract

This RFC defines the evidence required to make Mininet Forge canonical and reduce GitHub to a mirror or disable it.

## Preconditions

Cutover MUST NOT occur until:

- independent operators reproduce canonical proposal and review history;
- exact-state merge decisions work without GitHub;
- release, provenance, delegation, revocation, bounty, dispute, and amendment objects are retrievable;
- identity rotation and recovery are tested;
- a GitHub outage exercise succeeds;
- export and fork workflows succeed;
- no single founder key is necessary for ordinary progress;
- the cutover decision itself is governed and content-addressed.

## Dual-running

During dual-running, every canonical event MUST map to the same source, proposal, review, and decision state in both systems. Any mismatch MUST halt automatic mirroring and require investigation.

## Authority precedence

The cutover decision MUST name the exact point after which Forge is authoritative. Before that point GitHub bootstrap rules prevail. After that point GitHub merges without a matching Forge decision are non-canonical.

## Platform exit

GitHub MAY become:

- a read-only public mirror;
- an independently reproducible disaster-recovery mirror;
- an archival snapshot;
- fully disabled.

The selected state MUST be reversible only through legitimate governance, not a platform administrator's unilateral action.
