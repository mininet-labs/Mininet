# Extreme-environment operation — DTN design constraints

Gates [roadmap #28](https://github.com/britak420/Mininet/issues/28).
**Founder action required: find delay-tolerant-networking (DTN) or
satellite-networking domain expertise.** Engineering can reason about the
protocol implications once someone with that background sets the actual
latency/connectivity constraints — guessing at them produces a design
that's wrong for the regime it wasn't built for.

## Why "extreme environment" isn't one problem

The issue groups disaster, satellite, and interplanetary operation
together, but these are genuinely different latency/connectivity regimes,
and a design tuned for one can be actively wrong for another:

| Regime | Round-trip latency | Connectivity pattern |
|---|---|---|
| Normal internet | milliseconds | continuous |
| Satellite internet (LEO, e.g. Starlink-class) | tens–low hundreds of ms | mostly continuous |
| Disaster mesh | intermittent | minutes to hours of partition |
| Lunar relay | ~2.5 seconds | scheduled windows, predictable |
| Mars relay | 6–44 minutes (varies with orbital position) | scheduled windows, highly variable |
| Deep-space | much longer, highly variable | scheduled, sparse |

A domain expert needs to tell engineering which of these regimes are
actually in scope for near-term work (disaster mesh and LEO satellite are
plausible near-term; lunar/Mars relay are "think in centuries," Directive
13 territory, not next-quarter work) so effort isn't spent generalizing
prematurely for a regime nobody will hit for decades.

## What's already compatible, so the expert isn't starting from zero

Mininet's existing architecture is already delay-tolerant-friendly by
accident of its other design choices, not because DTN was explicitly
designed for yet:

- Local-first bootstrap (D-0012) and content-addressed storage mean a
  partitioned region can keep operating internally without any connection
  to the rest of the network.
- `mini-settlement` (D-0055, this session's work) already separates
  "signed pending claim" from "canonical finality" — exactly the
  distinction a delay-tolerant money model needs, since Directive 5
  already assumes outages are normal, not exceptional.

## What needs real design work once the expert engages

A `DelayTolerantTransport` seam, in the same trait-first style as
`mini_bearer::Bearer` and `mini_presence::RangingSource`:

```rust
pub trait DelayTolerantTransport {
    fn enqueue_bundle(&self, bundle: Bundle) -> Result<BundleId, DtnError>;
    fn receive_bundle(&self) -> Result<Option<Bundle>, DtnError>;
    fn custody_transfer(&self, bundle_id: BundleId) -> Result<CustodyReceipt, DtnError>;
}
```

modeled on the actual Bundle Protocol (RFC 9171) the DTN research
community already uses, rather than inventing a new store-and-forward
scheme from scratch — this is exactly the kind of "compose existing
reviewed work, don't invent new primitives" discipline Directive 14
already asks for elsewhere.

## The one design rule already clear without needing the expert

**Content, messages, repository sync, and attestations can be
delay-tolerant. Money finality cannot pretend latency doesn't exist.**
`mini-settlement`'s `SettlementState::PendingCanonical` already has
nowhere to go but "wait" when the canonical ledger is unreachable — that
is correct behavior under partition, not a bug to fix. For genuinely
long-latency regimes (lunar/Mars), the likely eventual answer is **local
settlement zones** — locally-canonical state anchored to periodic
canonical-chain checkpoints when a relay window opens — but that is a
real design problem for the domain expert to scope, not something to
improvise here.

## Questions the domain expert should answer

- Which regimes (disaster mesh vs. LEO satellite vs. lunar/Mars) are
  actually near-term priorities, vs. which should stay "think in
  centuries" and out of scope for now?
- Does RFC 9171 Bundle Protocol fit Mininet's existing content/object
  model cleanly, or does it need adaptation?
- For the in-scope regimes, what's the right custody-transfer and
  retry/expiry model, and how does it interact with
  `mini-settlement::PaymentClaim::valid_until_ms`'s existing expiry
  semantics?

## What closes this gate

A design document from the engaged expert scoping which regimes are
in-scope and what the transport/custody model should look like for them,
turned into a new roadmap issue (or issues) for the actual implementation
— this file is the constraint-gathering step, not the design itself.
