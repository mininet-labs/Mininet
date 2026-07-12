# RFC-0002: Forge Governance Objects

**Status:** Proposed

## Abstract

This RFC defines a minimal signed object vocabulary for proposals, evidence, reviews, approvals, integration, canonicalization, releases, delegations, bounties, claims, disputes, and settlement. The model permits anonymous contribution, pseudonymous continuity, AI-produced evidence, and voluntary owner adoption while reserving canonical authority for explicit governance processes.

## Requirements

- immutable content-addressed states;
- canonical encoding;
- domain separation by network, project, and object type;
- exact-state review and approval;
- explicit delegation proof;
- AI evidence separated from authority;
- public identity not required for ordinary contribution;
- compensation separated from governance power;
- append-only correction, revocation, and equivocation evidence;
- GitHub-independent verification.

## Object registry

Initial object type identifiers:

```text
mininet.gov/change-proposal/v1
mininet.gov/evidence-bundle/v1
mininet.gov/technical-review/v1
mininet.gov/responsibility-acceptance/v1
mininet.gov/approval/v1
mininet.gov/integration-result/v1
mininet.gov/canonicalization-decision/v1
mininet.gov/delegation/v1
mininet.gov/revocation/v1
mininet.gov/release-proposal/v1
mininet.gov/release-decision/v1
mininet.gov/bounty-offer/v1
mininet.gov/claim-commitment/v1
mininet.gov/acceptance-attestation/v1
mininet.gov/claim-proof/v1
mininet.gov/payment-authorization/v1
mininet.gov/settlement-receipt/v1
mininet.gov/dispute/v1
mininet.gov/dispute-decision/v1
```

## Security considerations

The primary risks are stale authority, replay, mutable-state approval, identity-lineage duplication, metadata deanonymization, collusion, treasury redirection, and overclaiming personhood. Implementations must fail closed and preserve conflicting signed evidence.

## Privacy considerations

Signatures provide authenticity, not anonymity. Transport metadata, timing, payout linkage, and reputation correlation require separate protections. Privacy-preserving value and proof systems remain experimental until externally audited.

## Transition

GitHub representations may coexist during bootstrap, but the signed object chain becomes authoritative only through the accepted Forge transition process.
