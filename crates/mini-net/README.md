# mini-net

Mininet's own wide-area peer discovery and gossip broadcast — the layer
`mini-bearer`'s local, identity-free bearers (BLE / local Wi-Fi / relay)
anticipated for finding peers beyond direct proximity and propagating
messages across them (D-0009). Two proven designs, adapted rather than
depended on (D-0034 point 3, "adapt the design, not the dependency"):

- `routing::RoutingTable` — a Kademlia-style bucketed routing table: peers
  stored by shared leading bits with the local id, O(log n) lookups
  without any node holding a full peer list.
- `gossip::GossipRouter` — dedup-flooding broadcast: forward a message the
  first time it's seen, drop every repeat, the same shape gossipsub's
  message cache uses.

`peer::PeerId` is a **transport-routing** identifier only, generated fresh
per session — never a stable identity. See that type's docs for why.

## Live demo: gossip over a real network (D-0042)

```sh
# terminal 1
cargo run -p mini-net --example gossip_live_demo -- hub 9000 2
# terminal 2
cargo run -p mini-net --example gossip_live_demo -- leaf 127.0.0.1:9000 alice --send "hello mininet"
# terminal 3
cargo run -p mini-net --example gossip_live_demo -- leaf 127.0.0.1:9000 bob --expect 1
```

Three genuinely separate OS processes — not simulated parties in one
process — talking over real `mini_bearer::TcpBearer` TCP connections. A
message sent by `alice` travels over a real socket to the hub, gets
deduped and forwarded by the same `GossipRouter` this crate ships, and
arrives at `bob` over a second real socket. Works unmodified across
separate machines on the same network — just use a real IP instead of
`127.0.0.1`. See the example's own doc comment for exactly what this
does and doesn't prove (hub-and-spoke, not a mesh; no peer discovery; no
encryption at this layer — that's `mini_bearer::Channel`'s job).

This crate's own library code stays deliberately transport-agnostic —
`mini-bearer` is a dev-dependency used only by the example above, not by
`RoutingTable`/`GossipRouter` themselves.

## Honest limits

This crate is the routing/broadcast *logic*, still not a fully running
network stack:

- **Real transport** now exists for the gossip half (`TcpBearer`, proven
  live by the demo above) but is not wired into `RoutingTable`/peer
  discovery yet, and TCP is a stand-in for local-Wi-Fi/relay connectivity,
  not BLE.
- **Bucket-refresh-by-liveness-ping** is not implemented — a stale bucket
  entry isn't detected and evicted yet.
- **Randomized gossip fanout** is not implemented — `fanout_peers` is
  deterministic (closest-first) for this slice, documented in
  `gossip.rs` as a known hardening gap: real gossip networks randomize
  fanout specifically to resist an attacker positioning itself as every
  honest peer's "closest" neighbor and silently dropping traffic (an
  eclipse attack).

The algorithms are deterministic and fully unit-tested without any socket;
the live demo above is the first time any of this crate's logic has
carried a message over an actual network connection.

## Build & test

```sh
cargo test -p mini-net
```

License: CC0-1.0 (public domain).
