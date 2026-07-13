# Issue #44 — consensus edge-case attack review

Status: closed across two decisions. D-0085 covered the initial review;
this decision (see `docs/DECISION_LOG.md`) tightens the timestamp rule from
monotonic to fully deterministic and fixes a fee-conversion overflow bug
found by an independent second review submitted as PR #121
(`agent/issue-44-consensus-edge-cases`). Both reviews cover the same root
cause: caller-supplied context treated as protocol truth is unsafe unless
independently verified.

## Timestamp manipulation

`BlockHeader::timestamp_ms` is signed and hashed, but a signature only
proves which proposer supplied a value — never that it reflects real time.
D-0085 first closed this with a monotonicity rule (each block's timestamp
must strictly exceed the previous one's). That rule has a residual gap: a
malicious proposer can still jump the timestamp arbitrarily far forward in
one step (e.g. straight to `u64::MAX`) and satisfy "strictly increasing."

This decision replaces monotonicity with deterministic logical time: at
height `h`, the only valid `timestamp_ms` is `h` itself.
`ConsensusNode::validate_proposal` and `LedgerChain::apply_finalized_block`
both reject even a correctly signed designated-proposer value whose
timestamp differs — the proposer has no discretion over this field at all,
not merely a bound on it. `LedgerChain::last_timestamp_ms()` is removed as
a public API; it is now redundant with `height()`.

This means block timestamps cannot bias proposer selection, rewards, fees,
or timelocks — not even by an attacker willing to accept "detectably weird"
values, since there are none available. A future real-time protocol must
introduce a separately specified and tested consensus clock; it must not
relax this rule by trusting one proposer's operating-system clock.

Evidence: `a_proposal_whose_timestamp_is_not_deterministic_is_prevoted_nil`,
`a_proposer_cannot_use_an_increasing_timestamp_to_evade_the_deterministic_check`
(`mini-consensus`), `a_timestamp_that_does_not_equal_the_block_height_is_rejected`
(`mini-execution`).

## Replay (domain confusion)

Already closed by D-0085's `VOTE_SIGN_DOMAIN` fix, which prepends a fixed
domain tag to every `Vote` transcript, matching the domain-separation
discipline `mini-consensus::wire`'s `Proposal` signing already used. Not
reopened by this decision.

## Fee manipulation

D-0085 closed the zero-rate gap (`PriceHistory::add_entry` and, as of this
decision, `fee_in_micro_mini` itself reject `micro_mini_per_micro_cent ==
0`). The independent second review found a distinct, previously-undetected
defect this decision closes: `fee_in_micro_mini`'s final step cast a `u128`
intermediate down to `u64` with `as`, which truncates silently on overflow
rather than failing. A sufficiently large target and price combination —
`fee_in_micro_mini(u64::MAX, u64::MAX)` is the adversarial case the new
test proves — produced a wrong, wrapped-around fee instead of an error.

`fee_in_micro_mini` now returns `Result<u64>`, using `u64::try_from` to
reject an unrepresentable quote as `ValueError::FeeOverflow` instead of
silently truncating it. A new `PriceHistory::fee_at` convenience method
binds historical price lookup and checked conversion into one call, so a
caller cannot accidentally apply today's rate to historical work by
threading `at_ms` through only one of two separate calls.

Governance still decides rate values and effective times; this code makes
the conversion deterministic and fail-closed, and does not claim to solve
economic governance capture (#43) or fee-oracle design (#49).

Evidence: `fee_overflow_is_rejected_instead_of_silently_truncated`,
`a_zero_rate_is_rejected_at_quote_time_too_not_only_at_ingress`,
`fee_at_binds_historical_lookup_and_checked_conversion_in_one_call`.

## Attribution

The deterministic-timestamp design and the fee-overflow finding were
independently identified in PR #121, opened in parallel with D-0085's own
PR #119 against the same pre-D-0085 `main`. Rather than merging two
diverging fixes for the same already-shipped mechanism, PR #121's stronger
timestamp design and its overflow finding were reconciled into this
decision on top of the already-merged D-0085; PR #121 itself was closed as
superseded once this landed. See PR #121's description and this decision's
`docs/DECISION_LOG.md` entry for the full record.

## Closure boundary

This closes #44's caller-context attack review and every concrete defect
found by either review. It does not close economic governance capture
(#43), fee-oracle design (#49), state sync (#45), or external cryptographic
review gates.
