use mini_bearer::BearerError;
use mini_objects::ObjectError;
use mini_sync::SyncError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, NetError>;

/// Why a bounded F1/F2 pull failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetError {
    /// Transport or channel failure.
    Bearer(BearerError),
    /// The underlying generic `mini-sync` exact-retrieval exchange failed
    /// (transport, protocol, or a peer limit).
    Sync(SyncError),
    /// Object decoding failure at the protocol layer.
    Object(ObjectError),
    /// A peer sent a malformed or out-of-order advertisement message.
    Protocol,
    /// A peer's advertisement or a caller's request exceeded a bound.
    LimitExceeded,
    /// The caller passed more distinct peers than `max_sources` allows for
    /// this session. Refused, not silently truncated.
    TooManySources,
}

impl core::fmt::Display for NetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NetError::Bearer(e) => write!(f, "bearer: {e}"),
            NetError::Sync(e) => write!(f, "sync: {e}"),
            NetError::Object(e) => write!(f, "object: {e}"),
            NetError::Protocol => write!(f, "malformed or out-of-order advertisement message"),
            NetError::LimitExceeded => write!(f, "federation net protocol limit exceeded"),
            NetError::TooManySources => {
                write!(
                    f,
                    "more peers were passed than this session's max_sources bound"
                )
            }
        }
    }
}
impl std::error::Error for NetError {}
impl From<BearerError> for NetError {
    fn from(e: BearerError) -> Self {
        NetError::Bearer(e)
    }
}
impl From<SyncError> for NetError {
    fn from(e: SyncError) -> Self {
        NetError::Sync(e)
    }
}
impl From<ObjectError> for NetError {
    fn from(e: ObjectError) -> Self {
        NetError::Object(e)
    }
}
