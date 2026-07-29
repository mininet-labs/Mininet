# Day-0 monetary chain adversarial review

**Scope:** proposed D-0414 exact implementation
**Reviewer:** internal Codex attack pass; not independent
**External audit:** required by A1 before real value

## Findings addressed in this proposal

| ID | Attack | Result |
|---|---|---|
| MC-01 | Replay a previously finalized issuance epoch | Rejected by strict sequential epoch state. |
| MC-02 | Calculate caps from a fabricated opening supply | Rejected unless it equals finalized computed circulating supply. |
| MC-03 | Modify one grant after planning | Full plan reconstruction differs and the block fails. |
| MC-04 | Put multiple epochs in one block to bypass boundary checks | More than one monetary epoch per block is rejected. |
| MC-05 | Accelerate vesting with proposer timestamp | Monetary code never reads block timestamp; cumulative finalized epoch duration is used. |
| MC-06 | Publish billions of Human Share identities | Chain plan carries one snapshot root/count and one aggregate position. |
| MC-07 | Overflow partial vesting near `u128` capacity | Quotient/remainder calculation avoids the intermediate wide product. |
| MC-08 | Finalize a monetary body under a false state root | Existing finalized execution rejects the header state-root mismatch. |

## Open blocking findings

| ID | Severity | Finding |
|---|---|---|
| MC-09 | Critical | Snapshot root/count authenticity and personhood are external inputs; a Sybil snapshot passes structural checks. |
| MC-10 | Critical | No private membership/nullifier claim spends the aggregate Human Share pool. |
| MC-11 | Critical | `PaymentClaim` finalization still does not debit or credit balances or reject insufficient funds. |
| MC-12 | Critical | Service and treasury allocations are structurally bounded but not authorized by evidence/receipt proofs. |
| MC-13 | Critical | No production genesis supply or ceremony is selected or authorized. |
| MC-14 | High | Finalized epoch durations could be advanced too quickly or stalled by validator/governance capture. |
| MC-15 | High | State grows by one position per Human Share epoch and optional grant; pruning/proofs are absent. |
| MC-16 | High | Canonical decoding, byte-size limits, and network transport for monetary plans are absent. |
| MC-17 | High | Beneficiary account ownership and key recovery are not connected to vesting positions. |
| MC-18 | High | No live migration exists for a cryptographic break affecting already-issued funds. |

## Evidence

The new tests cover large-population aggregate planning, equality and cap
recomputation, supply identities, lock progression, epoch replay, stale
opening supply, forged grants, near-capacity arithmetic, deterministic
commitments, and a real quorum-certificate-finalized monetary epoch.

Passing tests establish deterministic behavior within this model. They do not
establish personhood, economic calibration, elapsed physical time, consensus
deployment, privacy, custody, legal treatment, or production safety.

## Recommendation

Accept only as the supply/vesting-accounting layer below a future balance and
private-claim protocol. Do not describe P4 as fully enforced and do not expose
the aggregate pool as spendable value.
