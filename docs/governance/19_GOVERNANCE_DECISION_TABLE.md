# Governance Decision and Authority Table

**Status:** Normative baseline  
**Version:** 0.3

Symbols: **Y** permitted; **D** permitted only with explicit delegation; **E** evidence/advisory only; **N** prohibited; **O** owner-local authority.

| Action | Anonymous human | Persistent pseudonym | Public identity | AI agent | Maintainer | Working Group | Governance | Owner |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Submit proposal | Y | Y | Y | Y | Y | Y | Y | Y |
| Submit test/evidence | Y | Y | Y | Y | Y | Y | Y | Y |
| Receive bounty | Y* | Y | Y | D | Y | Y | N/A | Y |
| Accumulate continuity reputation | N | Y | Y | D | Y | Y | N/A | Y |
| Produce review findings | Y | Y | Y | E | Y | Y | Y | Y |
| Satisfy human approval | N | D | D | N | Y | D | Y | D |
| Approve own proposal independently | N | N | N | N | N | N | N | N |
| Canonicalize ordinary change | N | N | N | N | D | D | Y | N |
| Canonicalize constitutional change | N | N | N | N | N | N | Y | N |
| Authorize governed release | N | N | N | N | D | D | Y | N |
| Force owner activation | N | N | N | N | N | N | N | N |
| Approve local activation | N | N | N | N | N | N | N | O |
| Revoke delegated authority | N | N | N | N | D | D | Y | O** |
| Open dispute | Y | Y | Y | D | Y | Y | Y | Y |
| Vote where personhood is required | N*** | N*** | N*** | N | N*** | N*** | N*** | N*** |

\* Anonymous payment requires a compatible privacy-preserving claim and settlement path.  
\** Owners may revoke authority they personally delegated.  
\*** A persistent identity root is not sufficient proof of one unique human. Eligibility depends on the then-valid personhood mechanism.

## Independence rules

1. The author does not count as an independent approver.
2. Multiple accounts controlled by one authority MUST NOT count as independent quorum members.
3. Multiple AI agents under one human or organizational controller are not independent governance authorities.
4. A builder controlled by the release author SHOULD NOT count as an independent reproducibility witness where policy requires administrative independence.
5. A financial sponsor MAY fund work but MUST NOT gain political weight merely from funding.

## Risk classes

| Class | Typical changes | Minimum baseline during founder bootstrap |
|---|---|---|
| R0 Editorial | spelling, links, non-normative clarity | one authorized review or founder review |
| R1 Ordinary | local implementation with bounded effect | one independent approval and required CI |
| R2 Protocol | identity, networking, storage, execution semantics | two independent approvals where available, integration evidence, founder canonicalization |
| R3 Security-critical | cryptography, installer, update verification, consensus safety, treasury | specialist review, adversarial tests, explicit risk acceptance, applicable external gate |
| R4 Constitutional | frozen invariants, political equality, owner sovereignty, legitimacy | constitutional amendment process; never ordinary merge authority |

When only two qualified engineers exist, an R2/R3 author can receive one independent engineer approval; the second approval MUST come from the founder or another qualified independent reviewer. The author never fills the missing seat.

## Platform authority rule

GitHub administrators, organization owners, and Actions maintainers possess technical platform power. That power MUST be bounded by rulesets, logs, multiple recovery identities, and documented process. It MUST NOT be treated as inherent constitutional legitimacy.
