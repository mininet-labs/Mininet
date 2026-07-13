//! # mini-consensus
//!
//! The piece every other consensus crate in this tree has deliberately left
//! for later: the **networked round protocol** that actually carries
//! [`mini_chain`]'s finality math and [`mini_execution`]'s state machine off
//! a single machine.
//!
//! [`mini_chain`]'s own docs name its non-goals precisely — "proposer
//! rotation, round timeouts/view-change, vote gossip/networking, and
//! state-machine execution — the actual networked consensus protocol."
//! [`mini_execution`] inherits that boundary, applying a `(header, body, qc)`
//! triple handed to it "from *somewhere* (a real network, eventually)." This
//! crate is that *somewhere*: separate processes, on separate sockets,
//! propose a settlement block, exchange signed [`mini_chain::Vote`]s over a
//! real transport, form a [`mini_chain::QuorumCertificate`] out of what they
//! receive, and each feed it independently into their own
//! [`mini_execution::LedgerChain`] — reaching bit-identical state without ever
//! sharing memory or a filesystem.
//!
//! The round engine is a faithful implementation of the published, peer-
//! reviewed Tendermint algorithm (Buchman/Kwon/Milosevic, arXiv:1807.04938,
//! Algorithm 1 — what CometBFT runs): **multi-round with view-change**, so a
//! crashed or silent proposer no longer stalls a height, and **signed
//! proposals**, so only a round's designated proposer can get a value
//! considered.
//!
//! ## What this crate is
//!
//! - [`wire`] — a canonical, domain-tagged, length-prefixed, *bounded* codec
//!   for the two messages a round puts on the wire: a signed [`wire::Proposal`]
//!   (round, `valid_round`, block header + body, and the proposer's signature)
//!   and a signed [`mini_chain::Vote`]. Same hand-rolled discipline as
//!   `did_mini`'s codec and every content hash in this tree — no serialization
//!   framework on the security-critical path, the exact same bytes on every
//!   platform. [`wire::sign_proposal`]/[`wire::verify_proposal`] are the typed
//!   proposal-authentication primitive (never a generic `sign(bytes)`).
//! - [`round`] — [`round::Round`], a pure, deterministic, **order-insensitive,
//!   multi-round** Tendermint driver: proposer rotation ([`round::proposer_for`]),
//!   the propose/prevote/precommit steps, `nil` votes ([`round::NIL`]),
//!   `lockedValue`/`validValue` locking, POLC re-proposal, the `f+1` round
//!   skip, and round timeouts surfaced as clock-free [`round::Action::ScheduleTimeout`]
//!   intents. Driven by messages and timer fires arriving in *any* order, it
//!   emits the votes this node should broadcast and, once `>2/3` distinct
//!   validators precommit the same block, a verified
//!   [`mini_chain::QuorumCertificate`]. No sockets, no threads, no clock —
//!   fully unit-tested in isolation.
//! - [`node`] — [`node::ConsensusNode`], the integration point: a [`round::Round`]
//!   per height wired to a [`mini_execution::LedgerChain`]. It authenticates
//!   every incoming proposal ([`round::proposer_for`] + [`wire::verify_proposal`]),
//!   signs its own votes and proposals, honors round timers, and advances to
//!   the next height only behind a real quorum certificate its own
//!   [`mini_execution`] layer re-verifies. This is where "consensus decided
//!   block N" becomes "the canonical ledger advanced to height N."
//! - [`net`] — [`net::TcpMesh`], a real full-mesh transport over
//!   [`mini_bearer::TcpBearer`] (real sockets), plus [`net::run_to_height`],
//!   the loop that drives a [`node::ConsensusNode`] across a live network —
//!   polling messages and firing the widening per-round timers the node asks
//!   for — until a target height finalizes. This is the multi-process/
//!   multi-machine layer the crate exists for.
//!
//! ## Honest limits (read before trusting this with anything)
//!
//! Safety — never two conflicting decisions at one height — is implemented in
//! full via Tendermint locking, and finality is still exactly
//! [`mini_chain::verify_finality`]'s `>2/3`-distinct-roots rule. The remaining
//! gaps are liveness/DoS, transport security, and deployment, not correctness:
//!
//! - **Single-hop vote broadcast, not full gossip.** The host broadcasts each
//!   vote once; it does not re-gossip past rounds' votes. The crash-recovery
//!   path (a silent proposer) does not depend on that; the POLC-re-proposal
//!   path (paper line 28) does, so it is only as robust as the links are
//!   lossless. The *transport* no longer drops traffic to a merely-slow peer
//!   (see [`net::TcpMesh`]'s non-blocking buffered links), but a genuinely
//!   dropped or partitioned message is still not re-delivered.
//! - **No equivocation slashing.** A validator that double-signs is counted
//!   at most once per root (P2) and cannot manufacture a quorum; the attempt
//!   *is* detected, verified, and recorded ([`verify_equivocation`],
//!   [`EquivocatorRegistry`]), but nothing yet acts on that record — no
//!   exclusion from a future validator epoch, no economic penalty, no
//!   governance-visible strike.
//! - **Static validator set.** The set is fixed for a run; on-chain
//!   validator-set changes are separate, later work.
//! - **[`net::TcpMesh`] is transport, not discovery.** It assumes every
//!   peer's address is known and the mesh is fully connected (or connected
//!   via [`net::TcpMesh::establish_topology`]'s partial-mesh support) before
//!   consensus starts — `mini-net`'s overlay routing/gossip is the layer
//!   that replaces that assumption; still separate, later work. Every link
//!   *is* now confidential and tamper-evident: each one runs a
//!   [`mini_bearer::Channel`] handshake before any consensus byte crosses
//!   the wire (D-0206), the same construction `mini-sync`/`mini-cli`'s
//!   `sync connect`/`listen` already use. `Channel`'s handshake is
//!   deliberately anonymous, though — it proves nothing about *which*
//!   validator is on the other end. Consensus payloads still carry the real
//!   identity (every vote and proposal is a real `did:mini` signature), so
//!   a tampering, lying, or merely silent peer can stall the protocol but
//!   never forge a finalized block — do not put a bare mesh on a hostile
//!   network expecting anything beyond confidentiality and liveness under
//!   an honest majority.
//! - **Not gated behind D-0047.** No new cryptography: this composes
//!   `mini-chain`'s existing vote/finality verification, `did_mini`'s
//!   delegation/signing, and `mini-settlement`'s claim verification. The only
//!   new content is deterministic protocol bookkeeping and message framing.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod consequence;
mod error;
mod evidence;
mod node;
mod round;
mod wire;

pub mod net;

pub use consequence::{EquivocatorRegistry, RecordOutcome};
pub use error::{ConsensusError, Result};
pub use evidence::{verify_equivocation, EquivocationEvidence};
pub use node::{ConsensusNode, Emit, NodeConfig};
pub use round::{proposer_for, Action, Round, Step, NIL};
pub use wire::{sign_proposal, verify_proposal, ConsensusMessage, Proposal, MAX_MESSAGE_BYTES};
