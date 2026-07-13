//! Real multi-process/multi-machine transport for consensus: a full mesh of
//! [`mini_bearer::TcpBearer`] connections (real sockets), plus the loop that
//! drives a [`ConsensusNode`] over it until a target height finalizes.
//!
//! This is the layer that takes consensus off a single machine for the first
//! time. Everything above it ([`crate::round`], [`crate::node`]) is pure and
//! transport-agnostic; this module is where those decisions actually cross a
//! process boundary on a wire.
//!
//! ## Honest limits
//!
//! - **Confidential and tamper-evident, but not peer-authenticated.** Every
//!   link now runs a [`mini_bearer::Channel`] handshake (ephemeral X25519 +
//!   HKDF-SHA256 + ChaCha20-Poly1305, forward-secret, no new cryptography —
//!   the same construction `mini-sync`/`mini-cli`'s `sync connect`/`listen`
//!   already use) before any consensus byte crosses the wire, so an on-path
//!   observer can no longer read votes/proposals in cleartext or forge a
//!   frame the AEAD tag won't catch (roadmap #44's sibling finding, the
//!   founder's 2026-07-12 review's `5.3`/`5.4` "wire authenticated encrypted
//!   channels into consensus now" ask). `Channel`'s handshake is, by its own
//!   design, anonymous — it proves nothing about *which* validator is on the
//!   other end, only that both ends share a fresh, private, authenticated
//!   session. The *consensus payload* is still what carries real identity
//!   (every vote/proposal is a real `did:mini` signature, re-verified on
//!   receipt and again at apply time), so a tampering, lying, or merely
//!   silent peer can still stall the protocol but can never forge a
//!   finalized block. No discovery, so a malicious *first* connection from
//!   an unknown address is still possible — this closes eavesdropping and
//!   tampering, not Sybil connections.
//! - **No discovery, no NAT traversal, no reconnect.** Every peer's address
//!   must be known up front and the mesh is built once, before consensus
//!   starts. It need not be *fully connected*, though: [`TcpMesh::establish_topology`]
//!   builds an arbitrary edge set, and [`run_to_height`] dedup-floods
//!   (re-gossips) every message across those edges, so any **connected** graph
//!   is live — a vote reaches a non-adjacent peer via relay. Overlay peer
//!   *discovery* (`mini-net`) and a bearer that redials are still separate,
//!   later work; so is state-sync for a node that was down and missed a whole
//!   height (re-gossip only re-delivers messages still circulating).
//! - **Best-effort, non-blocking broadcast.** Every link is a non-blocking
//!   socket with a bounded per-link outbound buffer. A broadcast queues bytes
//!   and flushes whatever the socket accepts right now; a slow or wedged peer
//!   simply lets its buffer fill and then drops further frames — it can
//!   **never back-pressure or block an honest node** (the gap the round-0
//!   slice's blocking sends left open). Safety never depends on any single
//!   message arriving.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

use mini_bearer::{encode_frame, Channel, FrameReader, Initiator, Responder};
use mini_chain::ValidatorOracle;

use crate::consequence::EquivocatorRegistry;
use crate::error::{ConsensusError, Result};
use crate::node::{ConsensusNode, Emit};
use crate::wire::ConsensusMessage;

/// AEAD associated data for every consensus frame sealed over a link's
/// [`Channel`] — domain separation so a ciphertext produced for this purpose
/// can never be replayed as if it meant something else, the same discipline
/// `mini-sync`'s `SYNC_AAD` already follows for its own channel traffic.
const CONSENSUS_AAD: &[u8] = b"mini-consensus/channel/v1";

/// Bound on how long the initial (blocking) `Channel` handshake may take
/// before a link gives up — a peer that connects but never completes the
/// handshake must not hang `TcpMesh::establish`/`establish_topology`
/// forever. Generous for a real network; instant on loopback.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// AEAD tag (16 bytes, ChaCha20-Poly1305) plus the `u32` frame length prefix
/// [`encode_frame`] adds — the fixed overhead a sealed plaintext gains before
/// it reaches the wire. Used to size-check *before* calling [`Channel::seal`],
/// never after: [`Channel`] requires messages to be processed in the exact
/// order they were sealed, so sealing a message and then discarding it (e.g.
/// for lack of buffer room) would silently desynchronize the two sides'
/// counters and permanently break every later frame on the link. Checking
/// capacity first means a message is either sealed *and* queued, or neither.
const CIPHERTEXT_FRAME_OVERHEAD: usize = 16 + 4;

/// Hard cap on a single link's outbound buffer. A peer that stops reading
/// fills this and then has further frames dropped — the bound is what turns a
/// wedged peer from a source of back-pressure into a harmless best-effort
/// drop. Generous enough that a briefly-slow honest peer never loses traffic.
const MAX_LINK_OUTBOUND_BYTES: usize = 8 * 1024 * 1024;

/// How many recently-seen message ids the re-gossip dedup remembers. Bounded
/// so a flood of distinct messages cannot grow it without limit (the same
/// stance `mini-net`'s `GossipRouter` takes toward its own seen-cache).
const MAX_SEEN_MESSAGES: usize = 65_536;

/// A content id for a consensus message: the BLAKE3 digest of its canonical
/// wire bytes. Re-encoding is deterministic, so two copies of the same message
/// — whoever relayed it — hash identically and dedup.
fn message_id(msg: &ConsensusMessage) -> [u8; 32] {
    mini_crypto::HashAlgorithm::Blake3.digest(&msg.to_wire_bytes())
}

/// A bounded set of recently-seen message ids for dedup-flooding gossip.
/// Returns whether an id is *new* (should be forwarded and processed) or a
/// repeat (dropped). Oldest ids are evicted once the cap is reached — the same
/// "forward once, then drop duplicates" shape gossipsub's message cache uses.
#[derive(Debug)]
struct SeenCache {
    seen: std::collections::HashSet<[u8; 32]>,
    order: std::collections::VecDeque<[u8; 32]>,
    capacity: usize,
}

impl SeenCache {
    fn new(capacity: usize) -> Self {
        SeenCache {
            seen: std::collections::HashSet::new(),
            order: std::collections::VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    /// Record `id`. Returns `true` the first time (forward it), `false` on a
    /// repeat (already gossiped — drop it).
    fn insert(&mut self, id: [u8; 32]) -> bool {
        if !self.seen.insert(id) {
            return false;
        }
        self.order.push_back(id);
        if self.order.len() > self.capacity {
            if let Some(old) = self.order.pop_front() {
                self.seen.remove(&old);
            }
        }
        true
    }
}

/// Read-chunk size for draining a socket per syscall.
const READ_CHUNK_BYTES: usize = 16 * 1024;

/// One non-blocking, buffered, encrypted TCP link to a peer. Sends never
/// block: bytes are framed into `outbound` and flushed as far as the socket
/// will take them, with the remainder kept for the next flush. Reads are
/// non-blocking too. Every payload crossing the wire is sealed/opened
/// through `channel`, established by a blocking handshake in [`Link::new`]
/// before the socket ever switches to non-blocking mode.
#[derive(Debug)]
struct Link {
    stream: TcpStream,
    reader: FrameReader,
    channel: Channel,
    /// Pending outbound bytes, already sealed and frame-encoded; `out_pos`
    /// is how many from the front have been written.
    outbound: Vec<u8>,
    out_pos: usize,
}

impl Link {
    /// Complete a [`Channel`] handshake over `stream` (blocking, bounded by
    /// [`HANDSHAKE_TIMEOUT`]) and switch to non-blocking mode for ordinary
    /// operation. `is_initiator` must match the same dial/accept asymmetry
    /// [`TcpMesh::establish_topology`] already uses (the dialer is the
    /// handshake initiator, the accepter is the responder) — both sides
    /// must agree, or the handshake deadlocks each waiting for the other's
    /// hello.
    fn new(stream: TcpStream, is_initiator: bool) -> Result<Self> {
        let _ = stream.set_nodelay(true);
        stream
            .set_read_timeout(Some(HANDSHAKE_TIMEOUT))
            .map_err(mini_bearer::BearerError::from)?;
        let channel = if is_initiator {
            let (initiator, hello) = Initiator::start()?;
            handshake_send(&stream, &hello)?;
            let response = handshake_recv(&stream)?;
            initiator.finish(&response)?
        } else {
            let hello = handshake_recv(&stream)?;
            let (channel, response) = Responder::respond(&hello)?;
            handshake_send(&stream, &response)?;
            channel
        };
        stream
            .set_nonblocking(true)
            .map_err(mini_bearer::BearerError::from)?;
        Ok(Link {
            stream,
            reader: FrameReader::new(),
            channel,
            outbound: Vec::new(),
            out_pos: 0,
        })
    }

    /// Seal one message for the peer, then queue it and make a non-blocking
    /// flush attempt. Drops the message (best-effort) if the outbound buffer
    /// is already at capacity, so a peer that stopped reading can never grow
    /// us without bound or block us — sized *before* sealing (see
    /// [`CIPHERTEXT_FRAME_OVERHEAD`]'s doc), so a dropped message is never
    /// sealed at all and the channel's ordered counters never desync.
    fn queue(&mut self, frame: &[u8]) {
        let pending = self.outbound.len() - self.out_pos;
        if pending + frame.len() + CIPHERTEXT_FRAME_OVERHEAD > MAX_LINK_OUTBOUND_BYTES {
            return;
        }
        let Ok(ciphertext) = self.channel.seal(frame, CONSENSUS_AAD) else {
            return; // oversized — never happens for our bounded messages
        };
        let Ok(encoded) = encode_frame(&ciphertext) else {
            return; // unreachable given the size check above
        };
        self.outbound.extend_from_slice(&encoded);
        self.flush();
    }

    /// Write as much buffered data as the socket accepts right now. Partial
    /// writes and `WouldBlock` leave the remainder for later; a hard error
    /// discards the buffer (the link is effectively gone) without propagating.
    fn flush(&mut self) {
        while self.out_pos < self.outbound.len() {
            match self.stream.write(&self.outbound[self.out_pos..]) {
                Ok(0) => break,
                Ok(n) => self.out_pos += n,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => {
                    self.outbound.clear();
                    self.out_pos = 0;
                    return;
                }
            }
        }
        if self.out_pos == self.outbound.len() {
            self.outbound.clear();
            self.out_pos = 0;
        } else if self.out_pos > READ_CHUNK_BYTES {
            // Reclaim the written prefix so the buffer doesn't grow unbounded
            // while a peer drains slowly.
            self.outbound.drain(..self.out_pos);
            self.out_pos = 0;
        }
    }

    /// Flush pending outbound, then drain every complete frame available right
    /// now (non-blocking) into `out`, opening each through `channel` first.
    fn service(&mut self, out: &mut Vec<ConsensusMessage>) {
        self.flush();
        loop {
            match self.reader.next_frame() {
                Ok(Some(frame)) => {
                    // `Channel::open` requires strict in-order processing; a
                    // single failure (garbage, a desynced/replayed peer)
                    // permanently desyncs this link's counters, so there is
                    // nothing recoverable left to do but stop servicing it —
                    // the same fate a peer that goes silent already has.
                    // Safety never depends on this: consensus payloads are
                    // still independently self-authenticating.
                    let Ok(plaintext) = self.channel.open(&frame, CONSENSUS_AAD) else {
                        break;
                    };
                    if let Ok(msg) = ConsensusMessage::from_wire_bytes(&plaintext) {
                        out.push(msg);
                    }
                    continue;
                }
                Ok(None) => {}
                Err(_) => break,
            }
            let mut buf = [0u8; READ_CHUNK_BYTES];
            match self.stream.read(&mut buf) {
                Ok(0) => break, // peer closed
                Ok(n) => {
                    if self.reader.push(&buf[..n]).is_err() {
                        break;
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }
}

/// A full-mesh set of real TCP links to every other node. Peer identity is
/// **not** tracked per link on purpose: consensus messages self-identify (a
/// vote carries its signer's `did:mini`, a proposal its proposer), so the
/// transport only needs to move bytes to everyone, not know who is who.
#[derive(Debug)]
pub struct TcpMesh {
    links: Vec<Link>,
}

impl TcpMesh {
    /// Build the local node's links into an already-bound mesh.
    ///
    /// Connection convention (deterministic, deadlock-free): node `i` dials
    /// every node `j > i` and accepts one inbound connection from every node
    /// `j < i`. Every node therefore ends with exactly `n - 1` links. Because
    /// a TCP `connect` to a bound listener completes in the kernel's accept
    /// backlog without waiting for the peer's `accept()` call, no ordering of
    /// dials-vs-accepts across the nodes can deadlock.
    ///
    /// Every listener in the mesh must already be bound (its address present
    /// in `addrs`) before any node calls this — the caller binds all
    /// listeners first, then hands each node its own listener and the shared
    /// address list.
    pub fn establish(
        local_index: usize,
        addrs: &[SocketAddr],
        listener: &TcpListener,
    ) -> Result<Self> {
        // A full mesh: adjacent to every other node.
        let neighbors: Vec<usize> = (0..addrs.len()).filter(|&j| j != local_index).collect();
        Self::establish_topology(local_index, addrs, listener, &neighbors)
    }

    /// Build the local node's links for an arbitrary **partial** topology:
    /// `neighbors` is the set of peer indices this node shares an edge with (a
    /// full mesh is just "everyone else", which [`TcpMesh::establish`] passes).
    ///
    /// The same deadlock-free convention as the full mesh, restricted to edges
    /// that exist: node `i` dials each neighbor `j > i` and accepts one inbound
    /// connection for each neighbor `j < i`. The topology must be *consistent*
    /// (if `i` lists `j`, then `j` must list `i`) or a node will wait forever
    /// for an accept that never comes — the caller owns that; a well-formed
    /// undirected graph always satisfies it.
    ///
    /// A partial mesh only stays *live* if the graph is **connected**: a vote
    /// reaches a non-adjacent node only because [`run_to_height`] re-gossips
    /// (dedup-floods) every message it has not seen before across its own
    /// edges. On a disconnected graph a partition simply cannot hear each
    /// other, exactly as on any real network.
    ///
    /// Every link now also completes a blocking [`mini_bearer::Channel`]
    /// handshake before this function returns for it (see [`Link::new`]):
    /// the dialer is always the handshake initiator, the accepter always the
    /// responder, matching the dial/accept asymmetry itself. Unlike a bare
    /// TCP `connect` (which returns once the kernel's backlog accepts it,
    /// without waiting for the peer's own `accept()` call), a handshake
    /// genuinely waits on the peer's application-level response — but since
    /// dialing only ever targets *higher*-indexed peers, the wait graph is
    /// still acyclic (the highest-indexed node dials nobody and reaches its
    /// accept loop immediately), so this remains deadlock-free, just no
    /// longer instant-return.
    pub fn establish_topology(
        local_index: usize,
        addrs: &[SocketAddr],
        listener: &TcpListener,
        neighbors: &[usize],
    ) -> Result<Self> {
        let mut links = Vec::with_capacity(neighbors.len());
        // Dial each higher-indexed neighbor -- we are the handshake initiator.
        let mut higher: Vec<usize> = neighbors
            .iter()
            .copied()
            .filter(|&j| j > local_index)
            .collect();
        higher.sort_unstable();
        for j in higher {
            links.push(Link::new(connect_with_retry(&addrs[j])?, true)?);
        }
        // Accept one inbound connection for each lower-indexed neighbor --
        // we are the handshake responder.
        let accept_count = neighbors.iter().filter(|&&j| j < local_index).count();
        for _ in 0..accept_count {
            let (stream, _) = listener.accept().map_err(mini_bearer::BearerError::from)?;
            links.push(Link::new(stream, false)?);
        }
        Ok(TcpMesh { links })
    }

    /// Queue a message to every peer and flush what the sockets accept now.
    /// Best-effort and non-blocking: a peer that is slow or gone has its frame
    /// buffered (up to a cap, then dropped), never blocking this node.
    pub fn broadcast(&mut self, msg: &ConsensusMessage) -> Result<()> {
        let bytes = msg.to_wire_bytes();
        for link in &mut self.links {
            link.queue(&bytes);
        }
        Ok(())
    }

    /// Flush pending outbound and drain whatever has arrived on any link right
    /// now, without blocking. Frames that fail to decode are dropped (a peer
    /// that speaks garbage cannot crash this node); closed links are skipped.
    pub fn poll(&mut self) -> Vec<ConsensusMessage> {
        let mut out = Vec::new();
        for link in &mut self.links {
            link.service(&mut out);
        }
        out
    }
}

/// Send one handshake message (blocking, length-prefixed via [`encode_frame`]
/// — the exact framing [`mini_bearer::TcpBearer`] uses, applied directly to
/// the raw stream since a [`Link`] does not construct a `TcpBearer` of its
/// own). `stream` must still be in blocking mode (before [`Link::new`]
/// switches it to non-blocking).
fn handshake_send(mut stream: &TcpStream, msg: &[u8]) -> Result<()> {
    let encoded = encode_frame(msg)?;
    stream
        .write_all(&encoded)
        .map_err(mini_bearer::BearerError::from)?;
    Ok(())
}

/// Receive one handshake message (blocking, bounded by whatever read timeout
/// the caller already set on `stream` — [`Link::new`] sets
/// [`HANDSHAKE_TIMEOUT`] before calling this).
fn handshake_recv(mut stream: &TcpStream) -> Result<Vec<u8>> {
    let mut reader = FrameReader::new();
    loop {
        if let Some(frame) = reader.next_frame()? {
            return Ok(frame);
        }
        let mut buf = [0u8; READ_CHUNK_BYTES];
        let n = stream
            .read(&mut buf)
            .map_err(mini_bearer::BearerError::from)?;
        if n == 0 {
            return Err(ConsensusError::Transport(mini_bearer::BearerError::Closed));
        }
        reader.push(&buf[..n])?;
    }
}

fn connect_with_retry(addr: &SocketAddr) -> Result<TcpStream> {
    let mut last = None;
    for _ in 0..50 {
        match TcpStream::connect(addr) {
            Ok(stream) => return Ok(stream),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    Err(ConsensusError::Transport(mini_bearer::BearerError::from(
        last.unwrap_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::TimedOut, "connect retries exhausted")
        }),
    )))
}

/// The base timeout unit. Each step's timeout for a round is this scaled by
/// `round + 1`, so a partitioned or proposer-starved height keeps widening its
/// timeouts until it makes progress — the standard Tendermint liveness knob.
/// Tuned short here for loopback tests; a real deployment would pick larger,
/// network-appropriate values.
const TIMEOUT_BASE: Duration = Duration::from_millis(300);

/// A pending consensus timer the mesh driver is holding.
#[derive(Debug)]
struct Timer {
    fires_at: Instant,
    height: u64,
    round: u32,
    step: crate::round::Step,
}

/// Drive `node` over `mesh` until its finalized height reaches
/// `target_height`, returning [`ConsensusError::Stalled`] if `timeout`
/// elapses first. Unlike the round-0-only predecessor, a silent or crashed
/// proposer no longer stalls the height: the node's [`Emit::ScheduleTimeout`]s
/// are armed here as real timers, and firing them drives the height to the
/// next round and a fresh proposer (Tendermint view-change).
///
/// The body a node proposes when it is a height's proposer comes from the
/// `body_source` it was built with (see [`crate::NodeConfig`]).
///
/// Any [`Emit::Equivocation`] the node reports is independently re-verified
/// and recorded in `equivocators` — no longer silently dropped (founder
/// review's `consensus-evidence` P0 finding).
pub fn run_to_height<O>(
    node: &mut ConsensusNode<O>,
    mesh: &mut TcpMesh,
    target_height: u64,
    timeout: Duration,
    equivocators: &mut EquivocatorRegistry,
) -> Result<()>
where
    O: ValidatorOracle,
{
    let deadline = Instant::now() + timeout;
    let mut timers: Vec<Timer> = Vec::new();
    let mut seen = SeenCache::new(MAX_SEEN_MESSAGES);

    // Kick off round 0 of the first height.
    let start_emits = node.start()?;
    handle_emits(
        start_emits,
        mesh,
        &mut timers,
        &mut seen,
        equivocators,
        node.oracle(),
    );

    loop {
        if node.finalized_height() >= target_height {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(ConsensusError::Stalled);
        }

        let mut did_work = false;

        for msg in mesh.poll() {
            did_work = true;
            // Dedup-flood re-gossip: the first time this node sees a message,
            // it re-broadcasts it across its own edges (so a non-adjacent peer
            // hears it via relay) and processes it; a repeat is dropped. This
            // is what makes a *partial* mesh — any connected graph — live, and
            // what lets a peer that missed a directly-sent vote still get it.
            if !seen.insert(message_id(&msg)) {
                continue;
            }
            let _ = mesh.broadcast(&msg);
            let emits = node.on_message(msg)?;
            handle_emits(
                emits,
                mesh,
                &mut timers,
                &mut seen,
                equivocators,
                node.oracle(),
            );
        }

        // Fire any elapsed timers. Stale ones (for a finished height) are no-ops
        // inside the node; we simply drop them here either way.
        let now = Instant::now();
        let mut still_pending = Vec::with_capacity(timers.len());
        let ready: Vec<Timer> = {
            let mut ready = Vec::new();
            for t in timers.drain(..) {
                if t.fires_at <= now {
                    ready.push(t);
                } else {
                    still_pending.push(t);
                }
            }
            ready
        };
        timers = still_pending;
        for t in ready {
            did_work = true;
            let emits = node.on_timeout(t.height, t.round, t.step)?;
            handle_emits(
                emits,
                mesh,
                &mut timers,
                &mut seen,
                equivocators,
                node.oracle(),
            );
        }

        if !did_work {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

fn handle_emits<O: ValidatorOracle>(
    emits: Vec<Emit>,
    mesh: &mut TcpMesh,
    timers: &mut Vec<Timer>,
    seen: &mut SeenCache,
    equivocators: &mut EquivocatorRegistry,
    oracle: &O,
) {
    for emit in emits {
        match emit {
            Emit::Broadcast(msg) => {
                // Mark our own outgoing message as seen so a copy flooded back
                // by a peer is deduped rather than re-flooded again.
                seen.insert(message_id(&msg));
                // Best-effort; a dropped link is not fatal (gossip semantics).
                let _ = mesh.broadcast(&msg);
            }
            Emit::ScheduleTimeout {
                height,
                round,
                step,
            } => {
                timers.push(Timer {
                    fires_at: Instant::now() + TIMEOUT_BASE * (round + 1),
                    height,
                    round,
                    step,
                });
            }
            Emit::Committed { .. } => {}
            // Detected double-signing is independently re-verified and
            // recorded — no longer silently dropped (founder review's
            // `consensus-evidence` P0 finding). This is a role-only
            // consequence: it does not remove the root from `mesh`'s or
            // `node`'s static validator set, only makes the evidence
            // durably queryable for whatever consumes it next.
            Emit::Equivocation(evidence) => {
                equivocators.record(&evidence, oracle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_peer_that_never_reads_cannot_block_us_or_grow_our_buffer_past_the_cap() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        // The server completes a real handshake (Responder) so the client's
        // Channel is genuinely established, then is dropped without ever
        // servicing the link again -- a wedged/hung peer whose receive
        // window fills up.
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            Link::new(stream, false).unwrap()
        });

        let client_stream = TcpStream::connect(addr).unwrap();
        let mut link = Link::new(client_stream, true).unwrap();
        let _server_link = server.join().unwrap();

        // Offer far more than the cap plus any kernel send buffer. With the old
        // blocking send this loop would wedge once the peer's window filled;
        // that it completes at all is the liveness assertion.
        let frame = vec![0u8; 1024 * 1024]; // 1 MiB each
        for _ in 0..64 {
            link.queue(&frame); // 64 MiB offered to a peer reading none of it
        }

        let pending = link.outbound.len() - link.out_pos;
        assert!(
            pending <= MAX_LINK_OUTBOUND_BYTES,
            "a peer that never reads must not grow our outbound buffer past the cap \
             (pending {pending}, cap {MAX_LINK_OUTBOUND_BYTES})"
        );
    }

    #[test]
    fn queued_frames_cross_the_wire_as_ciphertext_never_plaintext() {
        // The core regression this task closes (founder review 2026-07-12,
        // 5.3/5.4: "wire authenticated encrypted channels into consensus
        // now"): before this change, a `Link` was a dumb cleartext pipe --
        // whatever bytes `queue()` was given crossed the wire verbatim,
        // length-prefixed only. Prove a distinctive marker no longer
        // appears anywhere in what actually crosses the socket, then prove
        // that isn't just corruption by recovering it intact through the
        // real channel.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            Link::new(stream, false).unwrap()
        });
        let client_stream = TcpStream::connect(addr).unwrap();
        let mut sender = Link::new(client_stream, true).unwrap();
        let mut receiver = server.join().unwrap();

        let plaintext =
            b"MARKER a real vote or proposal's signed bytes would look exactly this readable";
        sender.queue(plaintext);

        // Read whatever actually arrived on the raw socket, bypassing
        // Channel/Link entirely -- this is what a passive on-path observer
        // would see. Loopback is fast but not synchronous with a
        // non-blocking socket, so poll briefly.
        let mut raw = Vec::new();
        for _ in 0..200 {
            let mut buf = [0u8; 4096];
            match receiver.stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    raw.extend_from_slice(&buf[..n]);
                    break;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("unexpected read error: {e}"),
            }
        }
        assert!(!raw.is_empty(), "expected ciphertext to have arrived");
        assert!(
            !raw.windows(plaintext.len())
                .any(|window| window == plaintext.as_slice()),
            "the plaintext marker must never appear verbatim in what crosses the wire"
        );

        // Not corruption: decode the frame and open it through the real
        // channel, recovering the exact original bytes.
        let mut reader = FrameReader::new();
        reader.push(&raw).unwrap();
        let frame = reader
            .next_frame()
            .unwrap()
            .expect("a complete frame arrived");
        let recovered = receiver.channel.open(&frame, CONSENSUS_AAD).unwrap();
        assert_eq!(recovered, plaintext);
    }
}
