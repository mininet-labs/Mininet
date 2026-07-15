//! [`LookupPrivacyClass`]: MN-208's frozen taxonomy of lookup privacy
//! tiers (research report §3, "Taxonomy of Mininet lookup classes") — a
//! typed classification rather than caller judgment, so policy code can
//! reason about which tier a given lookup needs instead of guessing.
//!
//! Only [`LookupPrivacyClass::CapabilityScoped`]'s primitive
//! ([`crate::derive_lookup_label`] + [`crate::LocalIndex`]) is
//! implemented in this crate. The rest are named so policy and future
//! work have a stable vocabulary to target — see `docs/design/
//! private-lookup-and-dht-boundary.md` for what each tier still needs.

/// How much a lookup's own query is allowed to reveal, ordered from least
/// to most private. `#[non_exhaustive]` and `Ord`: policy code can
/// compare tiers (`class >= LookupPrivacyClass::CapabilityScoped`)
/// without this crate closing off future additions above `PrivatePIR`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum LookupPrivacyClass {
    /// Deliberately public content: public releases, public profile
    /// objects, public bootstrap data. May use a public DHT, a stable
    /// CID, and provider advertisement. Not implemented by this crate —
    /// `mini-net`'s existing peer/gossip layer is the public plane.
    Public,
    /// The lookup key itself is a capability-derived rotating label
    /// (this crate's [`crate::LookupLabel`]) rather than a plaintext
    /// object ID or identity — the tier this crate actually implements.
    CapabilityScoped,
    /// `CapabilityScoped`, plus the query is proxied/relayed so no single
    /// index service learns both the client's network address and the
    /// query (OHTTP-style role separation, research report §"role-
    /// separated query path"). Not implemented here — needs `mini-relay`
    /// wiring, future work.
    PrivateProxied,
    /// `PrivateProxied`, plus queries are batched with decoys and
    /// responses are bundled with multiple candidates so a fixed-size
    /// bundle doesn't reveal which single item the requester wanted.
    /// Not implemented here.
    PrivateBundled,
    /// Full cryptographic Private Information Retrieval: the index
    /// service itself cannot learn which record a query resolved to.
    /// Explicitly gated behind external cryptographic review — see
    /// CLAUDE.md's no-new-cryptography rule and D-0047. Not implemented
    /// here, and must not be claimed as implemented until that review
    /// exists.
    PrivatePIR,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privacy_classes_are_ordered_from_least_to_most_private() {
        assert!(LookupPrivacyClass::Public < LookupPrivacyClass::CapabilityScoped);
        assert!(LookupPrivacyClass::CapabilityScoped < LookupPrivacyClass::PrivateProxied);
        assert!(LookupPrivacyClass::PrivateProxied < LookupPrivacyClass::PrivateBundled);
        assert!(LookupPrivacyClass::PrivateBundled < LookupPrivacyClass::PrivatePIR);
    }
}
