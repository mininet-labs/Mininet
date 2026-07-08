# Universal addressing — connecting to Mininet from almost any device without DNS

Founder direction (2026-07-08): *"domains are out of date, even .onion
dropped the idea, we need our own thing"* — and a request to think
seriously about how any device, anywhere, connects to Mininet without a
central naming authority.

This is a design note, not yet an implementation. It exists so `mini-net`
peer discovery ([roadmap #24](https://github.com/britak420/Mininet/issues/24))
and `mini-bootstrap`'s remaining transport wiring
([#23](https://github.com/britak420/Mininet/issues/23)) get built toward
a considered target instead of backing into a naming scheme by accident.

## The actual problem, named precisely

Every naming system trades off between three properties nobody has found
a way to have all of at once — this is usually called Zooko's Triangle:

1. **Secure** — the name can't be spoofed, hijacked, or seized by anyone
   who doesn't hold the underlying key.
2. **Decentralized** — no registry, registrar, or certificate authority
   anyone has to trust or petition.
3. **Human-meaningful** — memorable and typeable, not a 64-character hash.

DNS picks (1)+(3) at the cost of (2) — exactly the centralization
Directive 2 says to assume will eventually fail: a registrar, a registry
operator, or a government with jurisdiction over either can seize,
censor, or reassign a domain. Tor's `.onion` addresses pick (1)+(2) at the
cost of (3) — self-certifying (the address *is* a hash of the service's
public key, so it can't be spoofed, and needs no registry at all), but
unmemorable and easy to mistype, which is exactly the founder's complaint
and a large part of why onion services never reached mainstream usability.

**There is no scheme that wins all three globally.** Anyone claiming
otherwise is usually hiding a registry somewhere. The honest engineering
move is to stop trying to pick one global winner and instead give each
property to the layer that actually needs it.

## The three-layer answer

Mininet already has two of the three layers built. This note is mostly
about naming the third and how it composes with the first two.

### Layer 1 — secure, canonical addresses (already built)

Every entity already has a self-certifying address that needs no
registry:

- **Identities and devices**: `did:mini` SCIDs (`did-mini`) — derived
  from the inception event itself, re-verifiable by anyone with no
  central lookup (SPEC-01 §3/G8).
- **Content and objects**: content hashes / CIDs (`mini-crypto::multihash`,
  `mini_objects::ObjectId`) — the address *is* a hash of the content,
  verified end-to-end (`docs/audits/issue-29-cid-integrity-review.md`).

Both already satisfy (1) secure + (2) decentralized. Neither is
memorable, and neither needs to be — this is the "ground truth" address
space every other layer resolves down to.

### Layer 2 — discovery/routing (partially built)

A secure address alone doesn't tell you *where* to send a packet.
`mini-net::RoutingTable`/`GossipRouter` (D-0009, proven live over real
TCP by D-0042's demo) is a Kademlia-style DHT: given a self-certifying
address, look up which currently-reachable peers can route to it. This is
structurally the same problem Tor's rendezvous/introduction points solve
for onion services, or Kademlia solves for BitTorrent/IPFS — "how do I
reach this key" without any fixed IP, without DNS, and without the
resolver needing to be trusted (a lying DHT node can only fail to route
you, it can't hand you a fake identity that would pass Layer 1
verification).

**This is the actual mechanism behind "connect from almost any device":**
a new device needs (a) *any* path into the DHT (see Bootstrap below) and
(b) the target's Layer-1 address. Once both are in hand, discovery and
transport (`TcpBearer` today; BLE/local-Wi-Fi once
[#22](https://github.com/britak420/Mininet/issues/22) lands) do the rest
— no domain, no fixed server, no certificate authority anywhere in the
path.

### Layer 3 — human-memorable names (not built; this is the actual gap)

This is the layer the founder is pointing at, and it's the one layer
where a *global* solution genuinely doesn't exist without reintroducing a
registry. The answer that stays consistent with "no owner, no admin key"
(P3) applied to naming itself is a **local, personal petname system**
(the construction Zooko Wilcox-O'Hearn herself proposed as the resolution
to her own triangle):

- Every device keeps its **own** address book: memorable local nicknames
  mapped to Layer-1 addresses. "Alice" on your device is *your* label for
  a specific `did:mini` SCID — nobody else's client needs to agree, and
  nothing enforces global uniqueness, because nothing is global.
- Labels are learned the same way trust already flows in this system:
  direct exchange (exactly what the keystone demo already does — two
  devices meet, exchange verified identities; labeling the result "Alice"
  locally is a trivial addition, not new architecture), or **imported
  from a trusted contact's own address book** (if you trust Bob, you can
  see "Bob calls this one Carol" as a *suggestion*, never a claim anyone
  is forced to accept).
- Public, memorable "directories" (the founder's own "mininet.dev" is
  exactly this shape) are just ordinary Mininet content — a `PublicWall`
  or curated object anyone can publish, that anyone can choose to
  subscribe to as a filter. This is precisely constitution principle 10
  ("content rules live in user/community filters") applied to *naming*
  instead of moderation, and it means no protocol rule ever reserves or
  enforces a name — the founder publishing and vouching for a directory
  labeled "mininet.dev" makes it *popular*, never *official* in a way
  code enforces.

This trades global human-readability for something Directive 2 explicitly
prefers: nothing to seize, nothing to censor, nothing that "eventually
fails" because there was never a central point to fail in the first
place. The cost is real and worth naming honestly: two people can't
assume they mean the same "Alice" without either having met or sharing a
mutual, trusted introducer — exactly like real-world names work before
any government registry existed.

## Bootstrap: the one place a "first contact" problem is unavoidable

A brand-new device with nothing on it needs *some* way to reach its first
peer before any of the above helps. Two paths, both already consistent
with existing decisions, neither needing DNS:

- **Local-first (primary, already the frozen default):** Bluetooth/local
  Wi-Fi from any nearby already-running device — this is D-0012's
  Bluetooth-only bootstrap requirement, and `mini-bootstrap`'s
  capsule/want-list mechanics already assume exactly this path. A QR code
  or NFC tap carrying a Layer-1 address is the realistic "type an address
  on any device" UX — a camera and a nearby peer, not a keyboard and a
  registrar.
- **Internet fallback (for a device with no nearby peer):** a small,
  well-known set of bootstrap/rendezvous peers — analogous to Bitcoin's
  or IPFS's hardcoded bootstrap peer lists, or the seed nodes any
  Kademlia-based network needs. The critical distinction from DNS: these
  are **entry points into the DHT, never trust roots**. A malicious or
  seized bootstrap peer can refuse to connect you (an availability/
  censorship risk, worth taking seriously) but cannot hand you a forged
  identity or forged content that would pass Layer-1 self-certification —
  it can only ever fail closed, never lie successfully. A small,
  independently-run, and rotatable set of these (not a single company's
  server, not a domain anyone can seize) is the honest way to bound this
  risk; exact composition is a governance question, not a cryptography
  one.

## What this note is not deciding

This is deliberately a design note, not a frozen invariant — no
`docs/INVARIANTS.md` row is added here, because the *mechanism*
(self-certifying addresses + DHT discovery + local petnames + local-first
bootstrap) is settled by things already built or already frozen
elsewhere, but the *policy* details (bootstrap peer set composition and
rotation, whether/how petname imports get UI treatment, whether a
"canonical" public directory concept is worth building at the app layer)
are open and belong to whoever builds `mini-net` peer discovery
([#24](https://github.com/britak420/Mininet/issues/24)) and the eventual
client. Revisit as those land.
