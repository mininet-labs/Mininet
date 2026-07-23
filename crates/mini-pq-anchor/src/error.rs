//! Errors for `mini-pq-anchor`.

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, PqAnchorError>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PqAnchorError {
    /// [`crate::provision_anchor`] was handed a key that is not
    /// [`mini_crypto::SignatureSuite::MlDsa65`] -- this crate only ever
    /// provisions PQ anchors, never classical ones.
    NotMlDsa65,
    /// A label exceeded [`crate::anchor::MAX_LABEL_BYTES`].
    LabelTooLong,
    /// An inventory was asked to hold more than
    /// [`crate::inventory::MAX_ANCHORS_PER_OWNER`] anchors for one owner.
    TooManyAnchorsForOwner,
}

impl core::fmt::Display for PqAnchorError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PqAnchorError::NotMlDsa65 => {
                write!(f, "anchor key is not an ML-DSA-65 key")
            }
            PqAnchorError::LabelTooLong => write!(f, "anchor label exceeds the maximum length"),
            PqAnchorError::TooManyAnchorsForOwner => {
                write!(f, "owner already holds the maximum number of PQ anchors")
            }
        }
    }
}

impl std::error::Error for PqAnchorError {}
