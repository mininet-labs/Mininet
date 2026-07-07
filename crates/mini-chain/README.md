# mini-chain

The finality-verification core of a custom Rust chain adapting a proven
Tendermint/CometBFT-style BFT design (Founder Decision A1, `docs/DECISION_LOG.md`
D-0008): **equal validator power per verified identity root, never stake**
(P1/P2 [FREEZE]).

**What this batch implements:** `ValidatorSet` (equal weight, no weight field
anywhere to misuse), `BlockHeader` (canonical, self-certifying content hash),
`Vote`/`sign_vote`/`verify_vote` (a validator device's signed commitment,
gated on `did_mini::Capabilities::VOTE` — this crate is that capability's
**first real consumer**, after it sat undocumented-in-code since SPEC-01 §6),
and `QuorumCertificate`/`verify_finality` (`>2/3` distinct validator roots
precommitting the same block at the same height/round is what makes a
Tendermint-style chain final instantly, without probabilistic confirmations).

**Honest scope:** this is the finality *math*, not the networked protocol.
Proposer rotation, round timeouts/view-change, vote gossip, and
state-machine execution are `pending` — this crate answers one question,
offline and precisely: *given this candidate set of votes, is this block
final?* The same relationship `mini-forge`'s attestation-counting already
has to the eventual chain ("the chain replaces the counting, not the
objects"). Value settlement, the release registry, and constitution-guard
enforcement (`docs/ROADMAP.md` Pack 9) build on this later.

```sh
cargo test -p mini-chain
```

License: CC0-1.0 (public domain).
