//! Error type for `mini-porep`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PorepError {
    /// `seal()` was given data that is empty or not a whole number of
    /// [`crate::seal::NODE_SIZE`]-byte nodes.
    InvalidDataLength { len: usize },
    /// `SealParams::new` was given zero layers -- there is no depth to
    /// seal through with no layers at all.
    ZeroLayers,
    /// An [`crate::audit::AuditChallenge`] named a node index outside the
    /// replica's node count.
    NodeOutOfRange { index: usize, node_count: usize },
    /// An [`crate::audit::AuditChallenge`] named a layer beyond the
    /// replica's sealed depth.
    LayerOutOfRange { layer: u32, num_layers: u32 },
    /// A Merkle inclusion proof could not be produced for an index that
    /// should have been in range -- an internal consistency failure, never
    /// expected in practice.
    MerkleProofFailed,
}

impl fmt::Display for PorepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PorepError::InvalidDataLength { len } => write!(
                f,
                "data length {len} is not a positive multiple of the node size"
            ),
            PorepError::ZeroLayers => write!(f, "sealing requires at least one layer"),
            PorepError::NodeOutOfRange { index, node_count } => {
                write!(
                    f,
                    "node index {index} is out of range for {node_count} nodes"
                )
            }
            PorepError::LayerOutOfRange { layer, num_layers } => write!(
                f,
                "layer {layer} is out of range for a replica sealed with {num_layers} layers"
            ),
            PorepError::MerkleProofFailed => {
                write!(
                    f,
                    "could not produce a Merkle inclusion proof for an in-range index"
                )
            }
        }
    }
}

impl std::error::Error for PorepError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, PorepError>;
