//! Track F6 Phase 2: fold one remote peer's already-ranked
//! [`crate::WireResult`]s (from [`crate::remote_query`]) into a caller's own
//! local/pulled [`mini_search_federation::FederatedResult`]s, using the
//! exact same deterministic dedup/tiebreak policy
//! [`mini_search_federation::federate_query`] applies across its own
//! sources ([`mini_search_federation::merge_federated_results`], unmodified
//! and reused directly rather than reimplemented here).
//!
//! F6 Phase 1's own doc named this the deliberately deferred follow-up:
//! `federate_query`'s typed merge expects a real `Corpus`/
//! `DocumentContextTable`-backed [`mini_search_federation::FederationSource`],
//! not a flat list of remote-computed results, so a query response cannot
//! simply be handed to `federate_query` as another source. This module is
//! the missing conversion step: a [`crate::WireResult`] becomes a real
//! [`mini_query::ResultProvenance`] (rejecting any bps value a compliant
//! peer could never have produced -- see [`federated_result_from_wire`]),
//! tagged with the peer's [`mini_web_types::ProviderPseudonym`].
//!
//! **That tag is caller-asserted, not cryptographically verified.** A query
//! response carries no `Object`/signature wrapping (see `query`'s module
//! doc) and F6 provides no caller/provider authentication beyond the
//! channel itself (`docs/design/f6-private-query-transport.md`). A caller
//! names `remote_provider` from whatever out-of-band knowledge it already
//! has of who it dialed (an advertisement it resolved, a session it set up
//! itself) -- exactly as honest, and exactly as unverified, as every other
//! Track F provider label already is once results leave a single signed
//! object's custody.

use mini_query::ResultProvenance;
use mini_search_federation::{merge_federated_results, FederatedResult};
use mini_web_types::{
    CrawlObservationId, ProviderPseudonym, RankingExplanation, SearchResult, WeightBps,
};

use crate::error::{NetError, Result};
use crate::query::{validate_wire_result, AuthenticatedQueryResults, WireResult};

/// Convert one [`WireResult`] into a typed [`FederatedResult`] tagged with
/// `provider`. Rejects (`NetError::Protocol`) any `relevance_score_bps` or
/// `explanation` component above [`WeightBps::MAX`] -- values a compliant
/// [`crate::serve_query`] can never produce, since [`mini_query::search`]
/// only ever emits validated [`WeightBps`]. This conversion invokes the
/// same shared validator as the F6 wire codec because [`WireResult`] is public
/// and can be constructed locally without passing through the decoder. Invalid,
/// noncanonical, oversized, or non-displayable local/legacy inputs therefore
/// fail closed before entering the typed federated merge; the typed `WeightBps`
/// conversion below repeats the score check as defense in depth.
pub fn federated_result_from_wire(
    wire: WireResult,
    provider: ProviderPseudonym,
) -> Result<FederatedResult> {
    // `WireResult` is public and can be constructed locally or supplied by a
    // legacy caller without traversing the F6 decoder. Reuse the exact same
    // canonical URL, field, multihash, score, and displayability validator here
    // before the value enters the typed federated merge.
    validate_wire_result(&wire)?;
    let bps = |v: u16| WeightBps::new(v).map_err(|_| NetError::Protocol);
    let relevance_score_bps = bps(wire.relevance_score_bps)?;
    let explanation = RankingExplanation {
        lexical_bps: bps(wire.explanation[0])?,
        phrase_bps: bps(wire.explanation[1])?,
        link_bps: bps(wire.explanation[2])?,
        freshness_bps: bps(wire.explanation[3])?,
        originality_bps: bps(wire.explanation[4])?,
        diversity_bps: bps(wire.explanation[5])?,
    };
    let result = SearchResult {
        url: wire.url,
        title: wire.title,
        snippet: wire.snippet,
        relevance_score_bps,
        availability: wire.availability,
        ranking_profile: wire.ranking_profile,
        explanation,
    };
    Ok(FederatedResult {
        result: ResultProvenance {
            result,
            source_observation: CrawlObservationId(wire.source_observation),
            index_segment: wire.index_segment,
        },
        provider,
    })
}

/// Merge one remote peer's [`crate::remote_query`] results into a caller's
/// own local/pulled [`FederatedResult`]s, applying
/// [`mini_search_federation::merge_federated_results`]'s deterministic
/// dedup/tiebreak policy across the combined set. `remote_provider` labels
/// which peer supplied `remote` -- see the module doc's caveat that this is
/// caller-asserted, not cryptographically verified. Fails closed
/// (`NetError::Protocol`) on the first out-of-range wire result rather than
/// silently dropping it and returning a partial merge.
pub fn merge_remote_results(
    local: Vec<FederatedResult>,
    remote: Vec<WireResult>,
    remote_provider: ProviderPseudonym,
    max_results: usize,
) -> Result<Vec<FederatedResult>> {
    let mut combined = local;
    combined.reserve(remote.len());
    for wire in remote {
        combined.push(federated_result_from_wire(wire, remote_provider.clone())?);
    }
    Ok(merge_federated_results(combined, max_results))
}

/// Merge authenticated remote results without accepting a caller-selected
/// provider label. The label is carried by [`AuthenticatedQueryResults`], which
/// can only be produced by the named-peer query path on an authenticated
/// transport connection.
pub fn merge_authenticated_remote_results(
    local: Vec<FederatedResult>,
    remote: AuthenticatedQueryResults,
    max_results: usize,
) -> Result<Vec<FederatedResult>> {
    let (provider, results) = remote.into_parts();
    merge_remote_results(local, results, provider, max_results)
}

#[cfg(test)]
mod tests {
    use mini_crypto::{HashAlgorithm, Multihash};
    use mini_web_types::{
        AvailabilityState, CanonicalUrl, IndexSegmentId, NormalizedHost, RankingProfileId, Scheme,
    };

    use super::*;

    fn digest(seed: &[u8]) -> Multihash {
        Multihash::of(HashAlgorithm::Blake3, seed)
    }

    fn url(path: &str) -> CanonicalUrl {
        CanonicalUrl::new(
            Scheme::Https,
            NormalizedHost::new("example.org").unwrap(),
            None,
            path,
            None,
        )
        .unwrap()
    }

    fn wire_result(path: &str, score: u16) -> WireResult {
        WireResult {
            url: url(path),
            title: "title".to_string(),
            snippet: "snippet".to_string(),
            relevance_score_bps: score,
            availability: AvailabilityState::Available,
            ranking_profile: RankingProfileId(digest(b"profile")),
            explanation: [score, 0, 0, 0, 0, 0],
            source_observation: digest(b"obs"),
            index_segment: IndexSegmentId(digest(b"segment")),
        }
    }

    fn provider(seed: &[u8]) -> ProviderPseudonym {
        ProviderPseudonym(digest(seed))
    }

    #[test]
    fn a_valid_wire_result_converts_and_round_trips_its_fields() {
        let wire = wire_result("/a", 4_000);
        let result = federated_result_from_wire(wire.clone(), provider(b"p1")).unwrap();
        assert_eq!(result.result.result.url, wire.url);
        assert_eq!(
            result.result.result.relevance_score_bps.value(),
            wire.relevance_score_bps
        );
        assert_eq!(result.result.source_observation.0, wire.source_observation);
        assert_eq!(result.provider, provider(b"p1"));
    }

    #[test]
    fn a_locally_constructed_filtered_result_cannot_bypass_f6_validation() {
        let mut wire = wire_result("/a", 100);
        wire.availability =
            AvailabilityState::Restricted(mini_web_types::RestrictionReason::UserFilter);
        assert_eq!(
            federated_result_from_wire(wire, provider(b"p1")),
            Err(NetError::Protocol)
        );
    }

    #[test]
    fn an_out_of_range_relevance_score_is_rejected() {
        let wire = wire_result("/a", WeightBps::MAX.value() + 1);
        assert_eq!(
            federated_result_from_wire(wire, provider(b"p1")),
            Err(NetError::Protocol)
        );
    }

    #[test]
    fn an_out_of_range_explanation_component_is_rejected() {
        let mut wire = wire_result("/a", 100);
        wire.explanation[3] = WeightBps::MAX.value() + 1;
        assert_eq!(
            federated_result_from_wire(wire, provider(b"p1")),
            Err(NetError::Protocol)
        );
    }

    #[test]
    fn merging_deduplicates_a_url_present_in_both_local_and_remote_by_score() {
        let local =
            vec![federated_result_from_wire(wire_result("/a", 1_000), provider(b"local")).unwrap()];
        let remote = vec![wire_result("/a", 9_000), wire_result("/b", 500)];
        let merged = merge_remote_results(local, remote, provider(b"remote"), 10).unwrap();

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].result.result.url, url("/a"));
        assert_eq!(merged[0].provider, provider(b"remote"));
        assert_eq!(merged[0].result.result.relevance_score_bps.value(), 9_000);
        assert_eq!(merged[1].result.result.url, url("/b"));
    }

    #[test]
    fn merging_respects_max_results_across_the_combined_set() {
        let local =
            vec![federated_result_from_wire(wire_result("/a", 1_000), provider(b"local")).unwrap()];
        let remote = vec![wire_result("/b", 900), wire_result("/c", 800)];
        let merged = merge_remote_results(local, remote, provider(b"remote"), 2).unwrap();
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].result.result.url, url("/a"));
        assert_eq!(merged[1].result.result.url, url("/b"));
    }

    #[test]
    fn an_invalid_remote_result_fails_the_whole_merge_rather_than_dropping_silently() {
        let local =
            vec![federated_result_from_wire(wire_result("/a", 1_000), provider(b"local")).unwrap()];
        let remote = vec![wire_result("/b", WeightBps::MAX.value() + 1)];
        assert_eq!(
            merge_remote_results(local, remote, provider(b"remote"), 10),
            Err(NetError::Protocol)
        );
    }
}
