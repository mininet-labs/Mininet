//! Federation crate errors.

use mini_objects::ObjectError;
use mini_store::StoreError;

pub type Result<T> = core::result::Result<T, FederationError>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FederationError {
    /// The wire payload was truncated, malformed, or not the canonical
    /// encoding of what it claims to be.
    BadEncoding,
    /// A field exceeded this crate's bound before allocation.
    LimitExceeded,
    /// The object is not the expected object type.
    WrongObjectType,
    /// The object's payload was encrypted rather than public.
    NotPublicPayload,
    /// Underlying `mini-objects` signing/encoding failure.
    Object(ObjectError),
    /// Underlying `mini-lexical-index` decode failure (index segment
    /// canonical-form violation).
    LexicalIndex(mini_lexical_index::LexicalIndexError),
    /// Underlying `mini-store` failure.
    Store(StoreError),
    /// Underlying `mini-query` failure from a per-provider `search` call
    /// during federated merging (Track F3).
    Query(mini_query::QueryError),
}

impl From<ObjectError> for FederationError {
    fn from(e: ObjectError) -> Self {
        FederationError::Object(e)
    }
}

impl From<mini_lexical_index::LexicalIndexError> for FederationError {
    fn from(e: mini_lexical_index::LexicalIndexError) -> Self {
        FederationError::LexicalIndex(e)
    }
}

impl From<StoreError> for FederationError {
    fn from(e: StoreError) -> Self {
        FederationError::Store(e)
    }
}

impl From<mini_query::QueryError> for FederationError {
    fn from(e: mini_query::QueryError) -> Self {
        FederationError::Query(e)
    }
}

impl core::fmt::Display for FederationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FederationError::BadEncoding => write!(f, "bad encoding"),
            FederationError::LimitExceeded => write!(f, "limit exceeded"),
            FederationError::WrongObjectType => write!(f, "wrong object type"),
            FederationError::NotPublicPayload => write!(f, "payload is not public"),
            FederationError::Object(e) => write!(f, "object: {e:?}"),
            FederationError::LexicalIndex(e) => write!(f, "lexical index: {e:?}"),
            FederationError::Store(e) => write!(f, "store: {e:?}"),
            FederationError::Query(e) => write!(f, "query: {e:?}"),
        }
    }
}

impl std::error::Error for FederationError {}
