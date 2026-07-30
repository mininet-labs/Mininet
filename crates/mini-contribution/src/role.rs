//! Which role a reward-split payee played in a completed delivery.

/// Whether a reward-split payee created the content or served it. The two
/// are deliberately distinguished: a manifest's signed author (the
/// creator) is not necessarily who served the bytes for any one delivery
/// (the seeder).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryRole {
    /// The content manifest's signed author.
    Creator,
    /// Whoever served the bytes for this particular delivery.
    Seeder,
}
