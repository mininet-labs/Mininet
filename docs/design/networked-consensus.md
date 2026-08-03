# Networked BFT consensus — `mini-consensus` (D-0200 through D-0207)

This document records the design of the first layer in this tree that runs
consensus across a process boundary: `mini-consensus`. It is the split the
founder directed (2026-07-11) — one track hardening the self-hosted
operational path, this track "advancing real multi-process/multi-machine
networking and consensus integration."

## The gap this closes

Three crates approached networked consensus and each stopped, by design, at
the same wall:

- **`mini-chain`** (D-0008) is the *finality math*: given a set of votes, is
  this block final (`>2/3` distinct validator roots precommitting it)? Its
  own docs name its non-goals precisely — "proposer rotation, round
  timeouts/view-change, vote gossip/networking, and state-machine execution
  — the actual networked consensus protocol."
- **`mini-execution`** (D-0061) is the deterministic *state machine*: given a
  `(header, body, qc)` triple "from *somewhere* (a real network,
  eventually)", it applies the block behind a re-verified certificate.
- **`mini-net`** is routing/gossip *logic* with no running transport;
  **`mini-bearer`** (D-0015) is a real TCP transport but a dumb pipe.

Nothing produced the `(header, body, qc)` triple, agreed among separate
processes, over a wire. `mini-consensus` is that missing *somewhere*.

## What shipped — the four layers

Four layers, each testable in isolation, thinnest-honest-slice first — the
same shape `mini-chain` used to land finality math before the network that
needs it. (The round engine below is the full multi-round Tendermint of
D-0201, not the original round-0 driver it superseded.)

1. **Wire (`wire.rs`).** A canonical, domain-tagged, length-prefixed,
   *bounded* codec for the two messages a round exchanges: a `Proposal`
   (block header + settlement body) and a signed `mini_chain::Vote`.
   Hand-rolled like `did_mini`'s codec — no framework on the security path,
   identical bytes on every peer, every read bounds-checked before it
   allocates. A signed vote's wire form lives in `mini-chain`
   (`Vote::to_wire_bytes`/`from_wire_bytes`) so the private signature field's
   invariant stays local even though votes now travel the network; the codec
   here composes that with the public header/claim fields. Decoding makes no
   trust decision — a well-framed hostile message decodes fine and is
   rejected later on its merits.

2. **Round (`round.rs`).** `Round` is a pure, deterministic,
   **order-insensitive**, **multi-round** driver — a faithful implementation
   of the published Tendermint consensus algorithm (Buchman/Kwon/Milosevic,
   arXiv:1807.04938, Algorithm 1), each `upon` rule citing the paper's line
   numbers (D-0201). `proposer_for(height, round, validators)` selects the
   proposer with no communication and rotates it per round, so a silent round-0
   proposer is replaced in round 1. It runs the full step machine
   (propose → prevote → precommit) with `lockedValue`/`lockedRound` +
   `validValue`/`validRound` locking, `nil` votes (the all-zero hash sentinel,
   never counted toward a real certificate), POLC re-proposal, and the
   `f+1`-higher-round skip — re-deriving every intent idempotently from
   accumulated state so reordering and duplication cannot corrupt it. Timeouts
   are `ScheduleTimeout` **intents**, not a clock: the state machine stays
   socket-free and clock-free and is fully unit-tested alone. It re-verifies
   every vote against the KEL oracle and counts one root at most once (P2).

3. **Node (`node.rs`).** `ConsensusNode` ties one `Round` per height to a
   `mini_execution::LedgerChain`. It validates a proposal's *value* against its
   *own* chain state before prevoting (right parent, honest `state_root` it can
   reproduce), signs its votes — including `nil` — with its delegated `VOTE`
   device (`mini_chain::sign_vote`, a typed request — never `sign(bytes)`),
   builds or re-proposes a value when it is the round's proposer (from a
   pluggable `BodySource`), feeds its own votes back into the round so it
   counts them, buffers future-height messages and replays them on advance,
   and — the load-bearing line — advances only through
   `LedgerChain::apply_finalized_block`, which re-verifies finality
   independently. "Consensus decided block N" becomes "the canonical ledger
   advanced to height N," and never otherwise.

4. **Net (`net.rs`).** `TcpMesh` is a real full mesh of **non-blocking,
   buffered** TCP links (peer identity deliberately untracked — messages
   self-identify by `did:mini` signature). Each link is a non-blocking socket
   with a bounded per-link outbound buffer (framed with `mini_bearer`'s
   `encode_frame`/`FrameReader`); a broadcast queues bytes and flushes what the
   socket accepts now, so a slow or wedged peer fills its buffer and then has
   frames dropped best-effort — it can **never back-pressure or block an honest
   node** (D-0203). Setup is deadlock-free by construction: node `i` dials every
   `j > i` and accepts from every `j < i`, and a TCP `connect` to a bound
   listener completes in the kernel backlog without waiting for the peer's
   `accept()`. `run_to_height` drives a node over the mesh — polling messages
   **and firing the widening per-round timers** the node asks for — until a
   target height finalizes.

**Proof it works off one machine, and survives a crash:**
`tests/networked_consensus.rs` holds two real-socket tests. The first runs four
validator nodes in four OS threads, each with an independent `LedgerChain`
sharing no memory and no filesystem, and asserts they converge — height by
height — on a bit-identical `LedgerState` commitment purely over the mesh
(Directive 4). The second runs a four-validator set with **one validator
permanently offline** (quorum is the remaining three): whenever the offline
node is a height's proposer, the three online nodes get no proposal, time out,
prevote/precommit `nil`, and **view-change to round 1** with a fresh proposer —
and still finalize every height in lockstep to identical state.


### Persistent authenticated state sync (D-0207)

D-0093's block-only catch-up is now joined by a complete-state recovery path.
`ConsensusSnapshot` carries exact canonical `LedgerState` bytes together with
the finalized header and QC. A receiver reconstructs a chain only after its own
static validator set/KEL oracle verifies the QC and the decoded state's
commitment equals the header root; the serving peer and local archive have zero
checkpoint authority.

`ConsensusArchive` stores a durable snapshot plus a count/byte-bounded contiguous
suffix under an OS file lock and replayable exact install journal. Live commits
and peer installs are persisted before the node swaps state; interrupted
replacement is replayed on reopen. `ConsensusNode::apply_state_sync` verifies an
entire blocks-or-snapshot response on a cloned chain, preventing partial apply.
The compatibility in-memory history is capped. A real encrypted-TCP test proves
a node that starts at genesis reaches a source archive's exact snapshot+suffix
tip and reopens to the same state commitment.

## Honest limits (the part that is not built)

Safety — never two conflicting decisions at one height — is implemented in
full via Tendermint locking, and **proposals are now signed** (D-0202): a node
accepts a proposal for `(height, round)` only if it is signed by a `VOTE`-
capable device of exactly `proposer_for(height, round)`, so a Byzantine node
can no longer front-run the designated proposer to waste a round. The
remaining gaps are liveness/DoS, transport-security, and deployment, not
correctness:

- **State sync is static-set and one-frame, not a universal checkpoint
  protocol.** D-0207 persists QC-bound exact snapshots and a bounded suffix,
  but verifies them against the caller's current static validator set. It does
  not prove historical validator-set transitions, solve long-range key
  compromise/weak subjectivity, or transfer states larger than the 8 MiB exact
  state / one bearer-frame response caps. Chunked Merkle state proofs and
  physical weak-device measurements remain later work.
- **Equivocation is detected but not punished (D-0204).** A validator that
  double-signs (two different votes for one `(height, round, phase)`) is now
  caught: it is counted at most once and its conflicting second vote is
  surfaced as verifiable `EquivocationEvidence`. But nothing *consumes* that
  evidence yet — there is no slashing, no ejection — and proposal-equivocation
  (two proposals for one round) is not yet collected. Detection, not
  enforcement.
- **Static validator set.** Fixed for a run; on-chain set changes are later
  work.
- **`TcpMesh` is transport, not discovery or security.** Cleartext, addresses
  known up front, no reconnect, no NAT traversal. Authenticated encryption is
  `mini_bearer::Channel`'s job; overlay discovery/gossip is `mini-net`'s.
- **Threads over loopback, not machines over the internet.** A real network
  transport exercising the real protocol, not a deployment.

## Next slices (in priority order)

1. **Chunked authenticated large-state sync and weakest-device evidence** —
   replace the one-frame ceiling with independently verifiable Merkle chunks,
   bounded resumability, and real phone CPU/memory/flash-pause measurements.
2. **Dynamic validator sets and historical checkpoint verification** — prove
   every set transition and define the explicit long-range/weak-subjectivity
   rule without installing a privileged checkpoint operator.
3. **Peer discovery, retry, and eclipse resistance** — route state-sync and
   live topology through `mini-net`, query independently chosen peers, and keep
   peer availability distinct from finality authority.
4. **Act on equivocation** — consume verified evidence through future role
   transitions; also collect proposal equivocation.
5. **Deployment hardening** — reconnect/background serving, pruning across
   upgrades, and multi-machine hostile-network drills.

None of these change what "final" means (frozen in `mini-chain`); they add the
security, robustness, and operational machinery layered around it. Persistent
QC-bound state sync and bounded pruning are **implemented in proposal D-0207**.
View-change
(the round-0 slice's largest gap) is **shipped** (D-0201), signed proposals
(view-change's largest residual) are **shipped** (D-0202), a non-blocking
buffered mesh (no dead-peer back-pressure) is **shipped** (D-0203),
equivocation *detection* with verifiable evidence is **shipped** (D-0204), and
message re-gossip over partial (connected) meshes is **shipped** (D-0205).
