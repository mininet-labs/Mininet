# Issue #44 — consensus edge-case attack review

Status: implemented and tested proposal. This review covers timestamp,
replay, and fee-conversion manipulation together because all three become
unsafe when a caller-supplied context is treated as protocol truth.

## Timestamp manipulation

`BlockHeader::timestamp_ms` is signed and hashed, but a signature only proves
which proposer supplied a value. It does not prove wall-clock time. Consensus
therefore uses deterministic logical time: at height `h`, the only valid
timestamp is `h`. `ConsensusNode::validate_proposal` rejects even a correctly
signed designated-proposer value whose timestamp differs.

This means block timestamps cannot bias proposer selection, rewards, fees, or
timelocks. A future real-time protocol must introduce a separately specified
and tested consensus clock; it must not relax this rule by trusting one
proposer's operating-system clock.

Evidence: `proposer_wall_clock_cannot_control_consensus_time`.

## Replay

Votes are domain-separated signed commitments to all consensus context that
changes their meaning: phase, height, round, and block hash. Proposal
signatures bind height, round, valid round, block hash, and proposer root.
The node ignores finalized heights, buffers future heights, verifies the
designated proposer, and the round counts a validator root at most once.

The new adversarial vote test changes each context field independently while
reusing the original signature; every replay fails verification. Existing
round/node tests cover duplicate delivery, stale heights, signature forgery,
wrong proposers, and equivocation.

Chain identity is the canonical genesis/history and validator-root context,
not a platform repository or deployment name. Two deployments with identical
canonical genesis and history are the same protocol history for this purpose.

Evidence: `signed_vote_cannot_be_replayed_in_another_context` and the existing
`mini-consensus` round/node adversarial suite.

## Fee manipulation

The governed rate history is append-ordered and historical lookup is stable.
Two fail-open arithmetic cases remained:

1. a zero rate converted every positive target into a zero fee;
2. a valid `u128` multiplication could exceed `u64` after scaling and the cast
   silently truncated it.

`PriceHistory::add_entry` and `fee_in_micro_mini` now reject zero rates.
`fee_in_micro_mini` returns `Result<u64>` and rejects unrepresentable quotes.
`PriceHistory::fee_at` performs historical lookup and checked conversion in
one operation, reducing accidental use of a current rate for historical work.

Governance still decides rate values and effective times. This code makes the
decision deterministic and fail-closed; it does not claim to provide a market
oracle or protect governance itself from capture. Those broader questions
remain in #43 and #49.

Evidence: `zero_rate_is_rejected_at_ingress_and_at_quote_time`,
`fee_overflow_is_rejected_instead_of_truncated`, and
`fee_at_binds_lookup_and_checked_arithmetic`.

## Closure boundary

This closes #44's caller-context attack review and the concrete defects found
by it. It does not close economic governance capture (#43), fee-oracle design
(#49), state sync (#45), or external cryptographic review gates.
