# Transparent balance ledger adversarial review

**Scope:** proposed D-0415
**Review:** internal Codex attack pass, not independent approval
**Production gate:** A1 remains open

## Addressed attacks

| ID | Attack | Result |
|---|---|---|
| BL-01 | Spend more than payer owns | Claim is not finalized and balance/sequence remain unchanged. |
| BL-02 | Split an overspend across claims in one block | Canonical order applies affordable claims only; conservation is rechecked. |
| BL-03 | Replay or replace a finalized sequence | Existing high-water/digest rule rejects it. |
| BL-04 | Overflow recipient balance | Checked arithmetic fails the state transition. |
| BL-05 | Self-transfer creates or destroys value | Balance is unchanged; only the signed sequence finalizes. |
| BL-06 | Hide supply in an implicit genesis account | Explicit allocations must exactly equal genesis circulating supply. |
| BL-07 | Duplicate genesis account overwrites value | Duplicate allocation fails genesis construction. |
| BL-08 | Invalid/oversized payee amplifies state or burns funds | Recipients must parse as the supported verifying-key suite and remain within 4,096 bytes. |
| BL-09 | Spend newly issued but still locked MINI | Balances plus unallocated track circulating supply only. |
| BL-10 | Two honest nodes disagree | Sorted balances and exact amounts enter the existing finalized state commitment. |
| BL-10A | Conservation scan grows linearly per block | Consensus tracks allocated total for O(1) checks; full-map recomputation remains an audit path. |
| BL-17 | Replay a valid payment on another deployment | The signed claim and executor both bind one exact 32-byte network identifier. |
| BL-21 | Omitted overspend remains pending in wallets | Canonical rejection outcome is state-committed and returned by reconciliation. |

## Critical open findings

| ID | Severity | Finding |
|---|---|---|
| BL-11 | Critical | Transparent account graph and amounts expose financial metadata. |
| BL-12 | Critical | There is no production genesis allocation or ownership ceremony. |
| BL-13 | Critical | Issuance pools cannot yet be privately and uniquely claimed into balances. |
| BL-14 | Critical | Service/treasury evidence cannot yet authorize credits. |
| BL-15 | Critical | No network proposer/mempool path transports claims to consensus. |
| BL-16 | Critical | Existing privacy primitives are not integrated or externally audited. |
| BL-18 | High | No fee or resource-exhaustion price exists; block claim count is the main CPU bound. |
| BL-19 | High | Full account state lacks proofs, snapshots, pruning, and weak-device sync. |
| BL-20 | High | Live post-quantum migration for account ownership is unresolved. |
| BL-22 | High | Canonical rejection records currently grow monotonically; no authenticated pruning or compact historical proof exists. |

## What tests prove

Tests demonstrate exact allocation, debit/credit, overspend behavior,
canonical aggregate-spend ordering, self-transfer behavior, replay handling,
supply conservation, deterministic commitments, and real
quorum-certificate-finalized transfer/reconciliation.

They do not prove privacy, economic safety, personhood, consensus deployment,
network availability, production genesis legitimacy, or external security.

## Recommendation

Accept as a transparent correctness reference and prerequisite for audited
private transactions. Do not expose it as production-private currency or treat
unallocated issuance as an account balance.
