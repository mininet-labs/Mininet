# mini-bearer

Identity-agnostic transport and an anonymous, forward-secret encrypted channel for
Mininet. This is the layer the two-phone keystone demo forms its link over.

## Two layers, separated on purpose

- **Bearer** — a dumb pipe that moves opaque frames and knows nothing about
  identity. BLE, local Wi-Fi/hotspot, and an internet relay are all just bearers.
  This crate ships an in-process bearer (`pair()`) for deterministic tests, and
  now a real one: `TcpBearer` (D-0042) moves frames over an actual TCP socket —
  the first bearer in this crate that leaves one process. It's a stand-in for
  local-Wi-Fi/relay connectivity, not BLE; the real BLE/Wi-Fi radio adapters
  still need to be built and tested on real devices behind the same `Bearer`
  trait `TcpBearer` already proves out.
- **Channel** — an encrypted session over any bearer. An ephemeral X25519
  handshake (`Initiator` / `Responder`) carries DH/KDF/AEAD suite tags, binds the
  full hello transcript into HKDF-SHA256, and derives ChaCha20-Poly1305 traffic
  keys. It yields a confidential, forward-secret duplex using only
  `mini-crypto`'s vetted primitives.

## Security model — anonymous connection, valid payload

The handshake carries **no identities**, so the connection is anonymous and
unlinkable; a passive observer sees only ephemeral public keys. The channel gives
confidentiality + forward secrecy + a 32-byte **channel binding**, but *not*
endpoint authentication — deliberately (constitution P5).

Authenticity lives in the payload, not the pipe:

- `did:mini` KELs are self-certifying and signed — a MITM can't forge one.
- Genesis / release chunks are content-addressed — the hash validates the bytes.
- Presence attestations will sign a transcript that includes the channel binding,
  both nonces, time, and the range challenge. The binding is necessary context,
  it prevents channel-transcript substitution, but it does **not** defeat
  relay/wormhole attacks by itself; anti-relay comes from the full presence
  protocol and its round-trip distance bound.

Frame and channel size caps are enforced before buffering or AEAD allocation, so
a hostile nearby peer cannot force unbounded memory growth through this crate.

A future upgrade can add endpoint *pseudonym* authentication (a SIGMA/Noise-XX
step keyed by a per-session pairwise pseudonym) without changing this crate's
shape or the anonymity property.

## `TcpBearer` — a real network transport (D-0042)

```rust
// accepting side (e.g. the "hub" in mini-net's live gossip demo)
let listener = std::net::TcpListener::bind("127.0.0.1:9000")?;
let (stream, _addr) = listener.accept()?;
let mut bearer = mini_bearer::TcpBearer::from_stream(stream)?;

// dialing side
let mut bearer = mini_bearer::TcpBearer::connect("127.0.0.1:9000")?;
```

Same `Bearer` trait, same `encode_frame`/`FrameReader` framing this crate
already shipped for byte-stream bearers — `TcpBearer` is the first thing to
actually use them over a real socket. See `mini_net`'s
`examples/gossip_live_demo.rs` for a live, three-separate-process
demonstration of gossip traveling over real `TcpBearer` connections.

Honest limits, stated in the module docs too: no authentication or
encryption at this layer (that's still `Channel`'s job, layer it on top);
no NAT traversal or peer discovery (both ends need a reachable address
already); no reconnect/retry (a drop surfaces as `BearerError::Closed`,
the caller redials).

## Not yet

Real BLE and local-Wi-Fi/mDNS **radio** bearer adapters (device-specific,
need real hardware); a reliability/reassembly layer for bearers that drop
or reorder frames; a pairwise-pseudonym authenticated handshake variant;
rekeying for very long sessions. These build on the same trait and channel.

## Build & test

```sh
cargo test -p mini-bearer
```

License: CC0-1.0 (public domain).
