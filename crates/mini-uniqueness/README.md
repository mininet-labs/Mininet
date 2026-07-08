# mini-uniqueness

Three-signal personhood/uniqueness fusion, per the founding whitepaper (§5)
and `docs/DECISION_LOG.md` D-0035 point 2 — superseding D-0034 point 2's
"left to us" framing now that the whitepaper specifies a concrete design.

## The three signals

- **(a) Social-vouching graph.** `vouch`/`verify` build mutual, signed vouch
  attestations between identity roots — the same two-party attestation
  pattern `mini-presence` uses, minus the proximity requirement (vouching
  may ride any transport, including a relay). `graph` propagates trust
  outward from a small trusted seed set for a bounded number of rounds
  (SybilRank-style): a Sybil cluster's internal edges don't help it score
  higher, only edges *into* the trusted region do.
- **(b) On-device behavioral/location entropy.** **Not implemented.**
  `confidence::BehavioralEntropySource` is the seam; the whitepaper calls
  this the most research-intensive component ("has not yet been shipped
  anywhere") and explicitly requires human authorship and external audit,
  not AI-authored code (D-0035 point 5). `NoEntropySource` is the correct,
  permanent implementation until that work exists.
- **(c) Physical-presence attestation.** Already `mini_presence::PresenceVerdict`
  — the whitepaper's named *strongest* signal.

## Fusion and decay

`confidence::fuse_confidence` combines whatever signals are available into
one 0..=100 score, decaying each by its own evidence age first — confidence
"must be continuously re-earned," not computed once. The weights and decay
curve are a tunable first cut, not a value the whitepaper specifies.

## Honest limits

Seed-set governance (who counts as a trusted seed, and how that set's
influence dilutes as the graph grows), the acceptance threshold at which
confidence unlocks value/governance, and calibration against a real network
are all left as caller-supplied parameters or future work — this crate
provides the verified, tested primitives, not a calibrated production
defense.

## Build & test

```sh
cargo test -p mini-uniqueness
```

License: CC0-1.0 (public domain).
