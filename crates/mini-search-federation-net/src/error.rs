use mini_bearer::BearerError;
use mini_objects::ObjectError;
use mini_query::QueryError;
use mini_store::StoreError;
use mini_sync::SyncError;
use mini_transport_security::TransportSecurityError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, NetError>;

/// Why a bounded F1/F2/F2b pull, or source assembly, failed.
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
    /// Underlying `mini-store` failure while assembling a source from
    /// already-pulled objects.
    Store(StoreError),
    /// A peer sent a malformed or out-of-order advertisement message.
    Protocol,
    /// A peer's advertisement or a caller's request exceeded a bound.
    LimitExceeded,
    /// The caller passed more distinct peers than `max_sources` allows for
    /// this session. Refused, not silently truncated.
    TooManySources,
    /// [`crate::assemble_federation_source`]: the trusted id set contains no
    /// F2 index segment to assemble a source around.
    NoIndexSegment,
    /// [`crate::assemble_federation_source`]: the trusted id set contains
    /// more than one F2 index segment -- ambiguous which one this source is
    /// for. Callers with multiple segments from one peer call this once per
    /// segment with a narrower id set.
    AmbiguousIndexSegment,
    /// [`crate::assemble_federation_source`]: no F2b corpus bundle in the
    /// trusted id set declares the segment's own `IndexSegmentId`.
    NoMatchingCorpusBundle,
    /// F6 [`crate::serve_query`]: the underlying `mini-query` parse/rank
    /// pass failed.
    Query(QueryError),
    /// Optional named-peer authentication or authenticated-channel runtime
    /// failed. Anonymous CH1 querying remains a separate API.
    TransportSecurity(TransportSecurityError),
}

impl core::fmt::Display for NetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NetError::Bearer(e) => write!(f, "bearer: {e}"),
            NetError::Sync(e) => write!(f, "sync: {e}"),
            NetError::Object(e) => write!(f, "object: {e}"),
            NetError::Store(e) => write!(f, "store: {e}"),
            NetError::Protocol => write!(f, "malformed or out-of-order advertisement message"),
            NetError::LimitExceeded => write!(f, "federation net protocol limit exceeded"),
            NetError::TooManySources => {
                write!(
                    f,
                    "more peers were passed than this session's max_sources bound"
                )
            }
            NetError::NoIndexSegment => {
                write!(f, "no F2 index segment found in the trusted id set")
            }
            NetError::AmbiguousIndexSegment => write!(
                f,
                "more than one F2 index segment found in the trusted id set"
            ),
            NetError::NoMatchingCorpusBundle => write!(
                f,
                "no F2b corpus bundle in the trusted id set declares this segment's id"
            ),
            NetError::Query(e) => write!(f, "query: {e}"),
            NetError::TransportSecurity(e) => write!(f, "transport security: {e}"),
        }
    }
}
impl std::error::Error for NetError {}
impl From<TransportSecurityError> for NetError {
    fn from(e: TransportSecurityError) -> Self {
        NetError::TransportSecurity(e)
    }
}
impl From<QueryError> for NetError {
    fn from(e: QueryError) -> Self {
        NetError::Query(e)
    }
}
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
impl From<StoreError> for NetError {
    fn from(e: StoreError) -> Self {
        NetError::Store(e)
    }
}
