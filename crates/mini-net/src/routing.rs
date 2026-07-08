//! A Kademlia-style routing table: peers are bucketed by how many leading
//! bits they share with the local id, so lookups converge in
//! O(log n) hops without any node needing a full peer list.
//!
//! ## Honest limits (first slice)
//!
//! Real Kademlia refreshes buckets by pinging the least-recently-seen peer
//! before evicting it for a new candidate, which is what makes the table
//! resilient to a flood of short-lived Sybil peer ids. That liveness check
//! needs a real transport and is `pending`; this slice tracks presence only
//! — a full bucket simply refuses new peers rather than evicting anyone,
//! documented rather than silently wrong.

use crate::peer::PeerId;

/// Standard Kademlia bucket size: how many peers a single distance-bucket
/// holds before refusing new entries.
pub const BUCKET_SIZE: usize = 20;

/// One bucket per bit of the 256-bit id space.
const BUCKET_COUNT: usize = 256;

/// A local node's view of the wide-area overlay.
#[derive(Debug)]
pub struct RoutingTable {
    local: PeerId,
    buckets: Vec<Vec<PeerId>>,
}

impl RoutingTable {
    /// A fresh, empty routing table centered on `local`.
    pub fn new(local: PeerId) -> Self {
        RoutingTable {
            local,
            buckets: (0..BUCKET_COUNT).map(|_| Vec::new()).collect(),
        }
    }

    /// The local peer id this table is centered on.
    pub fn local(&self) -> PeerId {
        self.local
    }

    /// Record a peer as known. Returns `false` for the local id itself, for
    /// a peer already present, or when its bucket is at [`BUCKET_SIZE`]
    /// (see the module-level honest limit); `true` when it was newly added.
    pub fn insert(&mut self, peer: PeerId) -> bool {
        let Some(bucket_index) = self.local.bucket_index(&peer) else {
            return false;
        };
        let bucket = &mut self.buckets[bucket_index];
        if bucket.contains(&peer) {
            return false;
        }
        if bucket.len() >= BUCKET_SIZE {
            return false;
        }
        bucket.push(peer);
        true
    }

    /// Whether `peer` is currently known.
    pub fn contains(&self, peer: &PeerId) -> bool {
        match self.local.bucket_index(peer) {
            Some(bucket_index) => self.buckets[bucket_index].contains(peer),
            None => false,
        }
    }

    /// Total known peers across every bucket.
    pub fn len(&self) -> usize {
        self.buckets.iter().map(Vec::len).sum()
    }

    /// Whether the table currently knows no peers.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The `k` known peers closest to `target` by XOR distance, nearest
    /// first — the primitive both peer lookup and gossip fanout selection
    /// build on.
    pub fn closest_peers(&self, target: &PeerId, k: usize) -> Vec<PeerId> {
        let mut all: Vec<PeerId> = self.buckets.iter().flatten().copied().collect();
        all.sort_by_key(|p| target.xor_distance(p));
        all.truncate(k);
        all
    }
}
