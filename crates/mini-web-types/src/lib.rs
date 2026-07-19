//! MiniSearch shared vocabulary (Track E foundation for D-0312).
//!
//! This crate intentionally contains only typed records and validation helpers:
//! no crawler, fetcher, parser, index, ranker, query service, network client, or
//! payment logic. It exists so later MiniSearch crates share the same boundary:
//! discovery, relevance ranking, availability restrictions, user filters, and
//! personalization are separate concepts in code, not prose promises.
//!
//! Two D-0312 rules are structural here:
//!
//! - default public search uses [`PersonalizationPolicy::None`];
//! - a restricted result carries an explicit [`AvailabilityState`] reason rather
//!   than hiding the restriction as a lower relevance score.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use mini_crypto::Multihash;

/// Stable content-addressed identity for a canonical URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlId(pub Multihash);

/// Stable identity for one crawl observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrawlObservationId(pub Multihash);

/// Stable identity for an immutable index segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSegmentId(pub Multihash);

/// Stable identity for a declared ranking profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankingProfileId(pub Multihash);

/// Search vocabulary errors.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WebTypeError {
    InvalidHost,
    InvalidPort,
    InvalidPath,
    InvalidQuery,
    InvalidStatusCode,
    InvalidWeight,
    InvalidProfileVersion,
    ResultRestrictionMismatch,
}

pub type Result<T> = std::result::Result<T, WebTypeError>;

/// URL schemes MiniSearch initially permits for public web crawling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Scheme {
    Http,
    Https,
}

/// A normalized host name. Stored lower-case and without a trailing dot.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NormalizedHost(String);

impl NormalizedHost {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let mut host = value
            .into()
            .trim()
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if host.is_empty()
            || host.len() > 253
            || host.starts_with('-')
            || host.ends_with('-')
            || host.contains("..")
            || !host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        {
            return Err(WebTypeError::InvalidHost);
        }
        if host.split('.').any(|label| {
            label.is_empty() || label.len() > 63 || label.starts_with('-') || label.ends_with('-')
        }) {
            return Err(WebTypeError::InvalidHost);
        }
        host.shrink_to_fit();
        Ok(NormalizedHost(host))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Canonical URL without fragments. Fragments are client navigation state, not
/// a separate crawled resource.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalUrl {
    pub scheme: Scheme,
    pub host: NormalizedHost,
    pub port: Option<u16>,
    pub path: String,
    pub query: Option<String>,
}

impl CanonicalUrl {
    pub fn new(
        scheme: Scheme,
        host: NormalizedHost,
        port: Option<u16>,
        path: impl Into<String>,
        query: Option<String>,
    ) -> Result<Self> {
        let path = path.into();
        if port == Some(0) {
            return Err(WebTypeError::InvalidPort);
        }
        if !path.starts_with('/')
            || path.contains('\\')
            || path.contains('#')
            || path.chars().any(|c| c.is_ascii_control())
            || path
                .split('/')
                .any(|segment| segment == "." || segment == "..")
        {
            return Err(WebTypeError::InvalidPath);
        }
        if query
            .as_deref()
            .is_some_and(|q| q.contains('#') || q.contains('\n') || q.contains('\r'))
        {
            return Err(WebTypeError::InvalidQuery);
        }
        let port = match (scheme, port) {
            (Scheme::Http, Some(80)) | (Scheme::Https, Some(443)) => None,
            (_, port) => port,
        };
        Ok(CanonicalUrl {
            scheme,
            host,
            port,
            path,
            query,
        })
    }

    /// Deterministic origin string for identity, sharding, and index keys.
    pub fn origin(&self) -> String {
        let scheme = match self.scheme {
            Scheme::Http => "http",
            Scheme::Https => "https",
        };
        match self.port {
            Some(port) => format!("{scheme}://{}:{port}", self.host.as_str()),
            None => format!("{scheme}://{}", self.host.as_str()),
        }
    }

    /// Deterministic path/query string. Fragments are never represented.
    pub fn path_and_query(&self) -> String {
        match &self.query {
            Some(query) => format!("{}?{query}", self.path),
            None => self.path.clone(),
        }
    }

    /// Deterministic canonical URL string. This is intentionally a formatter
    /// over already-validated parts, not a URL parser.
    pub fn canonical_string(&self) -> String {
        format!("{}{}", self.origin(), self.path_and_query())
    }
}

/// Bounded public-web media-type vocabulary. Later extraction crates may add
/// more variants without changing crawler/index semantics.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WebMediaType {
    Html,
    TextPlain,
    Markdown,
    Json,
    Pdf,
    Image,
    Other(String),
}

/// A crawler/index/search provider pseudonym. This is intentionally not a
/// governance identity, payment account, or ranking entitlement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPseudonym(pub Multihash);

/// What a crawler observed about a fetch attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrawlObservation {
    pub id: CrawlObservationId,
    pub requested_url: CanonicalUrl,
    pub final_url: CanonicalUrl,
    pub observed_at_ms: u64,
    pub status: FetchStatus,
    pub content_digest: Option<Multihash>,
    pub media_type: Option<WebMediaType>,
    pub byte_length: Option<u64>,
    pub redirect_chain: Vec<CanonicalUrl>,
    pub crawler: ProviderPseudonym,
}

/// Fetch outcome. A non-success status is not a ranking decision.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FetchStatus {
    Success(HttpStatus),
    RedirectLimitExceeded,
    Timeout,
    NetworkError,
    RobotsExcluded,
    UnsupportedScheme,
}

/// HTTP status code validated to the IANA range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HttpStatus(u16);

impl HttpStatus {
    pub fn new(code: u16) -> Result<Self> {
        if (100..=599).contains(&code) {
            Ok(HttpStatus(code))
        } else {
            Err(WebTypeError::InvalidStatusCode)
        }
    }

    pub fn code(self) -> u16 {
        self.0
    }
}

/// A result's availability after relevance retrieval but before display.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AvailabilityState {
    Available,
    Unavailable(UnavailabilityReason),
    Restricted(RestrictionReason),
}

impl AvailabilityState {
    pub fn is_displayable(&self) -> bool {
        matches!(self, AvailabilityState::Available)
    }
}

/// Non-policy unavailability.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnavailabilityReason {
    NotFetched,
    FetchFailed,
    Gone,
    UnsupportedContent,
}

/// Explicit restriction reasons. These are not relevance scores and must not be
/// silently folded into ranking.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RestrictionReason {
    RobotsExcluded,
    LegalRestriction { jurisdiction: String },
    Malware,
    Spam,
    UserFilter,
    SafetyWarning,
}

/// Ranking personalization policy. Public default is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PersonalizationPolicy {
    None,
    LocalUserControlled,
}

/// Integer basis-points weight. Keeps profiles deterministic and avoids floats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WeightBps(u16);

impl WeightBps {
    pub const ZERO: WeightBps = WeightBps(0);
    pub const MAX: WeightBps = WeightBps(10_000);

    pub fn new(value: u16) -> Result<Self> {
        if value <= Self::MAX.0 {
            Ok(WeightBps(value))
        } else {
            Err(WebTypeError::InvalidWeight)
        }
    }

    pub fn value(self) -> u16 {
        self.0
    }
}

/// Versioned declared-weight ranking profile. There is deliberately no payment,
/// stake, provider-balance, or governance-weight field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankingProfile {
    pub id: RankingProfileId,
    pub version: u16,
    pub lexical_weight: WeightBps,
    pub phrase_weight: WeightBps,
    pub link_weight: WeightBps,
    pub freshness_weight: WeightBps,
    pub originality_weight: WeightBps,
    pub diversity_weight: WeightBps,
    pub personalization: PersonalizationPolicy,
}

impl RankingProfile {
    pub fn public_default(id: RankingProfileId) -> Self {
        RankingProfile {
            id,
            version: 1,
            lexical_weight: WeightBps::new(4_500).unwrap(),
            phrase_weight: WeightBps::new(1_500).unwrap(),
            link_weight: WeightBps::new(1_000).unwrap(),
            freshness_weight: WeightBps::new(1_000).unwrap(),
            originality_weight: WeightBps::new(1_000).unwrap(),
            diversity_weight: WeightBps::new(1_000).unwrap(),
            personalization: PersonalizationPolicy::None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.version == 0 {
            return Err(WebTypeError::InvalidProfileVersion);
        }
        Ok(())
    }
}

/// Search result after retrieval/ranking plus explicit availability annotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub url: CanonicalUrl,
    pub title: String,
    pub snippet: String,
    pub relevance_score_bps: WeightBps,
    pub availability: AvailabilityState,
    pub ranking_profile: RankingProfileId,
    pub explanation: RankingExplanation,
}

impl SearchResult {
    /// Build a displayable result. Restricted/unavailable results must use the
    /// explicit non-display constructor so callers cannot accidentally hide a
    /// restriction inside the relevance score.
    pub fn displayable(
        url: CanonicalUrl,
        title: String,
        snippet: String,
        relevance_score_bps: WeightBps,
        ranking_profile: RankingProfileId,
        explanation: RankingExplanation,
    ) -> Self {
        SearchResult {
            url,
            title,
            snippet,
            relevance_score_bps,
            availability: AvailabilityState::Available,
            ranking_profile,
            explanation,
        }
    }

    pub fn with_availability(
        mut self,
        availability: AvailabilityState,
    ) -> std::result::Result<Self, WebTypeError> {
        if availability.is_displayable() || self.relevance_score_bps == WeightBps::ZERO {
            self.availability = availability;
            Ok(self)
        } else {
            Err(WebTypeError::ResultRestrictionMismatch)
        }
    }
}

/// Human/auditor-readable explanation of ranking components. The values are
/// component contributions, not authority or availability decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankingExplanation {
    pub lexical_bps: WeightBps,
    pub phrase_bps: WeightBps,
    pub link_bps: WeightBps,
    pub freshness_bps: WeightBps,
    pub originality_bps: WeightBps,
    pub diversity_bps: WeightBps,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_crypto::{HashAlgorithm, Multihash};

    fn digest(seed: &[u8]) -> Multihash {
        Multihash::of(HashAlgorithm::Blake3, seed)
    }

    fn profile_id() -> RankingProfileId {
        RankingProfileId(digest(b"profile"))
    }

    fn url() -> CanonicalUrl {
        CanonicalUrl::new(
            Scheme::Https,
            NormalizedHost::new("Example.Org.").unwrap(),
            None,
            "/search/index.html",
            Some("q=mininet".to_string()),
        )
        .unwrap()
    }

    fn explanation() -> RankingExplanation {
        RankingExplanation {
            lexical_bps: WeightBps::new(4_000).unwrap(),
            phrase_bps: WeightBps::new(500).unwrap(),
            link_bps: WeightBps::new(1_000).unwrap(),
            freshness_bps: WeightBps::new(0).unwrap(),
            originality_bps: WeightBps::new(500).unwrap(),
            diversity_bps: WeightBps::new(250).unwrap(),
        }
    }

    #[test]
    fn host_normalization_is_lowercase_and_trailing_dot_free() {
        let host = NormalizedHost::new("Example.Org.").unwrap();
        assert_eq!(host.as_str(), "example.org");
    }

    #[test]
    fn hostile_or_ambiguous_urls_are_rejected() {
        assert_eq!(
            NormalizedHost::new("../example"),
            Err(WebTypeError::InvalidHost)
        );
        assert_eq!(
            NormalizedHost::new("bad..example"),
            Err(WebTypeError::InvalidHost)
        );
        assert_eq!(
            CanonicalUrl::new(
                Scheme::Https,
                NormalizedHost::new("example.org").unwrap(),
                None,
                "relative",
                None
            ),
            Err(WebTypeError::InvalidPath)
        );
        assert_eq!(
            CanonicalUrl::new(
                Scheme::Https,
                NormalizedHost::new("example.org").unwrap(),
                None,
                "/../x",
                None
            ),
            Err(WebTypeError::InvalidPath)
        );
        assert_eq!(
            CanonicalUrl::new(
                Scheme::Https,
                NormalizedHost::new("example.org").unwrap(),
                Some(0),
                "/x",
                None
            ),
            Err(WebTypeError::InvalidPort)
        );
        assert_eq!(
            CanonicalUrl::new(
                Scheme::Https,
                NormalizedHost::new("example.org").unwrap(),
                None,
                "/x\n",
                None
            ),
            Err(WebTypeError::InvalidPath)
        );
    }

    #[test]
    fn http_status_is_range_checked() {
        assert_eq!(HttpStatus::new(200).unwrap().code(), 200);
        assert_eq!(HttpStatus::new(99), Err(WebTypeError::InvalidStatusCode));
        assert_eq!(HttpStatus::new(600), Err(WebTypeError::InvalidStatusCode));
    }

    #[test]
    fn public_ranking_profile_has_no_personalization_by_default() {
        let profile = RankingProfile::public_default(profile_id());
        assert_eq!(profile.version, 1);
        assert_eq!(profile.personalization, PersonalizationPolicy::None);
        profile.validate().unwrap();
    }

    #[test]
    fn ranking_weights_are_bounded_basis_points() {
        assert_eq!(WeightBps::new(10_000).unwrap().value(), 10_000);
        assert_eq!(WeightBps::new(10_001), Err(WebTypeError::InvalidWeight));
    }

    #[test]
    fn default_ports_are_normalized_out_of_canonical_urls() {
        let https = CanonicalUrl::new(
            Scheme::Https,
            NormalizedHost::new("Example.Org").unwrap(),
            Some(443),
            "/",
            None,
        )
        .unwrap();
        assert_eq!(https.port, None);
        assert_eq!(https.canonical_string(), "https://example.org/");

        let http = CanonicalUrl::new(
            Scheme::Http,
            NormalizedHost::new("Example.Org").unwrap(),
            Some(80),
            "/",
            None,
        )
        .unwrap();
        assert_eq!(http.port, None);
        assert_eq!(http.canonical_string(), "http://example.org/");
    }

    #[test]
    fn canonical_url_string_is_deterministic() {
        let url = CanonicalUrl::new(
            Scheme::Https,
            NormalizedHost::new("Example.Org.").unwrap(),
            Some(8443),
            "/search/index.html",
            Some("q=mininet".to_string()),
        )
        .unwrap();

        assert_eq!(url.origin(), "https://example.org:8443");
        assert_eq!(url.path_and_query(), "/search/index.html?q=mininet");
        assert_eq!(
            url.canonical_string(),
            "https://example.org:8443/search/index.html?q=mininet"
        );
    }

    #[test]
    fn restricted_results_must_be_explicit_not_silent_score_penalties() {
        let result = SearchResult::displayable(
            url(),
            "Example".to_string(),
            "snippet".to_string(),
            WeightBps::new(7_500).unwrap(),
            profile_id(),
            explanation(),
        );
        assert_eq!(
            result
                .clone()
                .with_availability(AvailabilityState::Restricted(RestrictionReason::Spam)),
            Err(WebTypeError::ResultRestrictionMismatch)
        );

        let hidden = SearchResult {
            relevance_score_bps: WeightBps::ZERO,
            ..result
        };
        assert!(hidden
            .with_availability(AvailabilityState::Restricted(
                RestrictionReason::LegalRestriction {
                    jurisdiction: "example".to_string()
                }
            ))
            .is_ok());
    }

    #[test]
    fn available_results_may_carry_a_nonzero_relevance_score() {
        let result = SearchResult::displayable(
            url(),
            "Example".to_string(),
            "snippet".to_string(),
            WeightBps::new(7_500).unwrap(),
            profile_id(),
            explanation(),
        )
        .with_availability(AvailabilityState::Available);
        assert!(result.is_ok());
    }

    #[test]
    fn unavailable_results_may_be_explicit_non_display_records() {
        let result = SearchResult::displayable(
            url(),
            "Example".to_string(),
            "snippet".to_string(),
            WeightBps::ZERO,
            profile_id(),
            explanation(),
        )
        .with_availability(AvailabilityState::Unavailable(
            UnavailabilityReason::NotFetched,
        ));
        assert!(result.is_ok());
    }

    #[test]
    fn crawl_observation_separates_fetch_status_from_content_identity() {
        let observation = CrawlObservation {
            id: CrawlObservationId(digest(b"observation")),
            requested_url: url(),
            final_url: url(),
            observed_at_ms: 1_784_355_200_000,
            status: FetchStatus::Success(HttpStatus::new(200).unwrap()),
            content_digest: Some(digest(b"html")),
            media_type: Some(WebMediaType::Html),
            byte_length: Some(1024),
            redirect_chain: Vec::new(),
            crawler: ProviderPseudonym(digest(b"crawler")),
        };
        assert!(matches!(observation.status, FetchStatus::Success(_)));
        assert!(observation.content_digest.is_some());
    }
}
