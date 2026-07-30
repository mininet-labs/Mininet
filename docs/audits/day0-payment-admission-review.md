# Day-0 payment admission adversarial review

**Scope:** proposed D-0416
**Review:** internal Codex attack pass, not independent approval

## Addressed attacks

| ID | Attack | Result |
|---|---|---|
| PA-01 | Claim frame declares an enormous field | Field length is checked before slicing/allocation. |
| PA-02 | Truncated, trailing, wrong-domain, or unknown-suite frame | Decoder fails closed. |
| PA-03 | Flood one payer or the whole pool | Independent global, byte, and per-payer limits. |
| PA-04 | Admit two conflicting claims at one payer sequence | Second slot occupant is refused. |
| PA-05 | Split aggregate overspend across many sequences | Pending amount reservation cannot exceed finalized payer balance. |
| PA-06 | Replay finalized/rejected claim | Canonical view rejects it during admission/revalidation. |
| PA-07 | Cross-network replay | Signed network ID must equal finalized ledger network. |
| PA-08 | Arrival order changes candidate block | Candidate sort uses payer, sequence, and digest. |
| PA-09 | Finality changes balance underneath pool | Revalidation evicts stale or newly unaffordable claims. |
| PA-10 | Admission silently becomes ownership | API returns only local admission; finality path is unchanged. |

## Critical open findings

| ID | Severity | Finding |
|---|---|---|
| PA-11 | Critical | No authenticated internet submission or claim re-gossip exists. |
| PA-12 | Critical | Transparent claim admission exposes financial metadata. |
| PA-13 | High | No fee, proof-of-work, stake, personhood, or other Sybil-resistant spam cost exists. |
| PA-14 | High | Proposers can omit admitted claims; no inclusion-list or censorship proof exists. |
| PA-15 | High | Pool state is volatile and not crash-persistent. |
| PA-16 | High | Local validity uses a device clock and cannot be canonical time evidence. |
| PA-17 | High | Canonical rejection records remain unbounded and lack compact proofs. |

## Evidence limit

Tests establish codec round-trip, signature preservation, truncation/trailing
rejection, policy bounds, conflict/duplicate handling, aggregate reservation,
deterministic order, wire admission, and canonical revalidation. They do not
prove internet availability, privacy, spam economics, fair inclusion,
production throughput, or external security.
