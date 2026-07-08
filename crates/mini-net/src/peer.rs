//! Ephemeral, session-scoped peer identifiers.
//!
//! A [`PeerId`] is a **transport-routing** identifier only — who to route a
//! gossip message toward in the wide-area overlay. It is generated fresh per
//! session from [`mini_crypto::random_32`] and is never derived from, or
//! bound to, a `did:mini` identity root. Treating it as a stable
//! cross-session identifier would recreate exactly the identity leak
//! `mini-bearer`'s anonymous channel handshake was designed to avoid
//! (D-0015 [FREEZE]); this type exists to make that distinction load-bearing
//! in the type system, not just in a doc comment.

use crate::error::{NetError, Result};

/// A 256-bit routing identifier in the same XOR keyspace as the content
/// addressed by every peer's advertised objects. Not a cryptographic key and
/// not an identity — purely a position in the routing overlay.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PeerId(pub [u8; 32]);

impl PeerId {
    /// A fresh, unpredictable peer id for this session. Callers must not
    /// persist this across sessions or derive it from any identity material
    /// — a new one is generated every time a node joins the overlay.
    pub fn generate() -> Result<Self> {
        let bytes = mini_crypto::random_32().map_err(|_| NetError::Entropy)?;
        Ok(PeerId(bytes))
    }

    /// XOR distance to another peer id — Kademlia's metric: symmetric,
    /// zero iff the two ids are equal, and satisfies the triangle
    /// inequality, which is what makes prefix-bucketed routing converge.
    pub fn xor_distance(&self, other: &PeerId) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (o, (a, b)) in out.iter_mut().zip(self.0.iter().zip(other.0.iter())) {
            *o = a ^ b;
        }
        out
    }

    /// The Kademlia bucket index for a peer at the given distance from this
    /// id: the position (0..=255) of the highest set bit in the XOR
    /// distance, i.e. how many of the leading bits the two ids share.
    /// `None` when the distance is all-zero (the two ids are equal — a peer
    /// is never bucketed against itself).
    pub fn bucket_index(&self, other: &PeerId) -> Option<usize> {
        let distance = self.xor_distance(other);
        let leading_zero_bits = distance
            .iter()
            .enumerate()
            .find(|(_, byte)| **byte != 0)
            .map(|(i, byte)| i * 8 + byte.leading_zeros() as usize)?;
        Some(255 - leading_zero_bits)
    }
}

impl core::fmt::Debug for PeerId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "PeerId({:02x}{:02x}{:02x}{:02x}…)",
            self.0[0], self.0[1], self.0[2], self.0[3]
        )
    }
}
