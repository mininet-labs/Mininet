#!/usr/bin/env python3
"""Complete F6 semantic validation for public mutable URL/profile records."""
from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:120]!r}")
    p.write_text(text.replace(old, new), encoding="utf-8")


path = "crates/mini-search-federation-net/src/query.rs"
replace_once(
    path,
    """fn validate_profile(profile: &RankingProfile) -> Result<()> {
    validate_multihash(&profile.id.0)
}

fn validate_url(url: &CanonicalUrl) -> Result<()> {
    if url.host.as_str().len() > MAX_HOST_BYTES
        || url.path.len() > MAX_PATH_BYTES
        || url
            .query
            .as_ref()
            .is_some_and(|query| query.len() > MAX_URL_QUERY_BYTES)
    {
        return Err(NetError::LimitExceeded);
    }
    Ok(())
}
""",
    """fn validate_profile(profile: &RankingProfile) -> Result<()> {
    validate_multihash(&profile.id.0)?;
    profile.validate().map_err(|_| NetError::Protocol)
}

fn validate_url(url: &CanonicalUrl) -> Result<()> {
    if url.host.as_str().len() > MAX_HOST_BYTES
        || url.path.len() > MAX_PATH_BYTES
        || url
            .query
            .as_ref()
            .is_some_and(|query| query.len() > MAX_URL_QUERY_BYTES)
    {
        return Err(NetError::LimitExceeded);
    }
    let reconstructed = CanonicalUrl::new(
        url.scheme,
        url.host.clone(),
        url.port,
        url.path.clone(),
        url.query.clone(),
    )
    .map_err(|_| NetError::Protocol)?;
    if reconstructed != *url {
        return Err(NetError::Protocol);
    }
    Ok(())
}
""",
)
replace_once(
    path,
    """        let mut invalid_score = base;
        invalid_score.relevance_score_bps = WeightBps::MAX.value() + 1;
""",
    """        let mut invalid_url = base.clone();
        invalid_url.url.path = "relative".to_string();
        assert_eq!(
            Msg::QueryResponse {
                results: vec![invalid_url]
            }
            .encode(),
            Err(NetError::Protocol)
        );

        let mut invalid_profile = profile.clone();
        invalid_profile.version = 0;
        assert_eq!(
            Msg::QueryRequest {
                query: "hello".to_string(),
                profile: invalid_profile,
                max_results: 8,
            }
            .encode(),
            Err(NetError::Protocol)
        );

        let mut invalid_score = base;
        invalid_score.relevance_score_bps = WeightBps::MAX.value() + 1;
""",
)
replace_once(
    "docs/design/f6-private-query-transport.md",
    """- the same field, score, count, URL, jurisdiction, and multihash bounds run before encoding and after decoding, so a provider cannot emit a message its own peer would reject solely because local metadata was oversized;
""",
    """- the same field, score, count, canonical-URL, ranking-profile-version, jurisdiction, and multihash validation runs before encoding and after decoding, so a provider cannot emit a message its own peer rejects because local public fields were oversized, mutated, or semantically invalid;
""",
)
replace_once(
    "docs/THREAT_MODEL.md",
    """| **F6 outbound/decode bound asymmetry, profile substitution, or filtered-result reinsertion** | Every request/response is validated before encoding and after decoding; response fields, score components, result counts, and multihashes share one bound set, clients require the requested profile, and ranked responses reject every non-displayable availability state. | **Closed for F6 framing, profile attribution, and ranker-filter preservation.** Provider honesty and query-content privacy remain unsolved. |
""",
    """| **F6 outbound/decode asymmetry, profile substitution, mutated URL/profile fields, or filtered-result reinsertion** | Every request/response is validated before encoding and after decoding; canonical URLs and profile versions are reconstructed/validated, response fields, score components, result counts, and multihashes share one bound set, clients require the requested profile, and ranked responses reject every non-displayable state. | **Closed for F6 framing, profile attribution, and ranker-filter preservation.** Provider honesty and query-content privacy remain unsolved. |
""",
)
print("PR 296 profile and URL validation completed")
