//! `mini-provider` -- typed vocabulary for the edge/provider layer.
//!
//! Founder Directive 18 (D-0400, D-0352): this crate is a LEAF. No core
//! crate may ever depend on it (INV-18-01), and it has zero governance or
//! humanness reach (INV-18-02, INV-18-03). It contains no network client,
//! no payment execution, no canonical provider registry (INV-18-04), and
//! no cryptographic signing yet.
//!
//! Wave 1 of the doctrine's confirmed sequencing (`docs/design/` FD-18
//! Part VI) is pure data only: [`ProviderDeclaration`], [`EngagementGrant`],
//! [`LocalProviderPolicy`], and the [`ProviderRanker`] discovery trait, with
//! structural well-formedness checks only -- never a judgment about
//! whether a provider is honest, licensed, or safe (that remains a human/
//! reviewer question, FD-18 Part I, T2). Binding a declaration/grant to a
//! real signed, content-addressed `mini_objects::Object`, verifying a
//! grant's `holder_commitment` against a presented secret, and any real
//! escrowed settlement flow are separate, later work
//! (`mini-engagement`, D-0402, and beyond).
//!
//! A network-wide equivalent of [`LocalProviderPolicy`] does not exist in
//! this crate and must never be added (INV-18-05): a network-level
//! disable switch for a service is indistinguishable in code from a
//! network-level disable switch for a person.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod declaration;
mod discovery;
mod error;
mod grant;
mod policy;

pub use declaration::{
    CustodyPosture, DataRequirement, DeathDisposition, ExitTerms, FreezePowers, JurisdictionClaim,
    ProviderDeclaration, ServiceClass, ServiceClassTag, MAX_DATA_REQUIREMENTS,
    MAX_DESCRIPTION_BYTES, MAX_JURISDICTION_CLAIMS,
};
pub use discovery::{CuratedList, LocalContext, ProviderRanker};
pub use error::{ProviderError, Result};
pub use grant::{AttestationKind, EngagementGrant, Permit, MAX_PERMITS};
pub use policy::LocalProviderPolicy;
