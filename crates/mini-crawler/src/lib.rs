//! Deterministic MiniSearch crawler planning.
//!
//! This crate is intentionally not a crawler runtime. It performs no network
//! I/O, DNS lookup, JavaScript execution, HTML parsing, storage, indexing,
//! ranking, payment, or governance logic. It is the small policy core that a
//! later runtime can call before it fetches anything.
//!
//! The first crawler invariant is boring on purpose: URL admission is
//! deny-by-default, bounded, same-host by default, robots-aware by explicit
//! caller input, and deterministic across peers that receive the same inputs.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use std::collections::{BTreeSet, VecDeque};

use mini_web_types::{CanonicalUrl, NormalizedHost, Scheme};

/// Crawler planning errors.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CrawlerError {
    EmptySeedSet,
    MultipleSeedHosts,
    InvalidLimit,
}

pub type Result<T> = std::result::Result<T, CrawlerError>;

/// Deterministic hard limits for one crawl plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrawlLimits {
    pub max_depth: u8,
    pub max_pending_urls: usize,
    pub max_seen_urls: usize,
    pub max_url_bytes: usize,
    pub allow_http: bool,
}

impl CrawlLimits {
    pub fn strict_single_host() -> Self {
        CrawlLimits {
            max_depth: 2,
            max_pending_urls: 1_000,
            max_seen_urls: 10_000,
            max_url_bytes: 2_048,
            allow_http: false,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.max_pending_urls == 0 || self.max_seen_urls == 0 || self.max_url_bytes == 0 {
            return Err(CrawlerError::InvalidLimit);
        }
        Ok(())
    }
}

/// Explicit non-fetch restriction inputs known before URL scheduling.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CrawlExclusions {
    robots_excluded_hosts: BTreeSet<NormalizedHost>,
    excluded_url_prefixes: BTreeSet<String>,
}

impl CrawlExclusions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn exclude_host(mut self, host: NormalizedHost) -> Self {
        self.robots_excluded_hosts.insert(host);
        self
    }

    pub fn exclude_url_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.excluded_url_prefixes.insert(prefix.into());
        self
    }

    pub fn host_is_excluded(&self, host: &NormalizedHost) -> bool {
        self.robots_excluded_hosts.contains(host)
    }

    pub fn url_is_excluded(&self, url: &CanonicalUrl) -> bool {
        let canonical = url.canonical_string();
        self.excluded_url_prefixes
            .iter()
            .any(|prefix| canonical.starts_with(prefix))
    }
}

/// One URL admitted to a crawl frontier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrawlRequest {
    pub url: CanonicalUrl,
    pub depth: u8,
    pub referrer: Option<CanonicalUrl>,
}

impl CrawlRequest {
    pub fn seed(url: CanonicalUrl) -> Self {
        CrawlRequest {
            url,
            depth: 0,
            referrer: None,
        }
    }

    pub fn discovered(url: CanonicalUrl, depth: u8, referrer: CanonicalUrl) -> Self {
        CrawlRequest {
            url,
            depth,
            referrer: Some(referrer),
        }
    }
}

/// Result of attempting to admit a URL to the frontier.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CrawlAdmission {
    Accepted,
    Rejected(CrawlRejectReason),
}

/// Explicit rejection reason. These are crawler-layer facts, not ranking
/// penalties and not user-silent search censorship.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CrawlRejectReason {
    HttpDisabled,
    CrossHost,
    DepthLimit,
    PendingLimit,
    SeenLimit,
    UrlTooLong,
    RobotsExcluded,
    Duplicate,
}

/// Deterministic, bounded frontier for one seed set.
#[derive(Debug, Clone)]
pub struct CrawlPlan {
    seed_hosts: BTreeSet<NormalizedHost>,
    limits: CrawlLimits,
    exclusions: CrawlExclusions,
    pending: VecDeque<CrawlRequest>,
    seen: BTreeSet<String>,
}

impl CrawlPlan {
    pub fn from_seeds(seeds: Vec<CanonicalUrl>, limits: CrawlLimits) -> Result<Self> {
        Self::from_seeds_with_exclusions(seeds, limits, CrawlExclusions::new())
    }

    pub fn from_seeds_with_exclusions(
        seeds: Vec<CanonicalUrl>,
        limits: CrawlLimits,
        exclusions: CrawlExclusions,
    ) -> Result<Self> {
        limits.validate()?;
        if seeds.is_empty() {
            return Err(CrawlerError::EmptySeedSet);
        }

        let seed_hosts: BTreeSet<NormalizedHost> =
            seeds.iter().map(|url| url.host.clone()).collect();
        if seed_hosts.len() > 1 {
            return Err(CrawlerError::MultipleSeedHosts);
        }

        let mut plan = CrawlPlan {
            seed_hosts,
            limits,
            exclusions,
            pending: VecDeque::new(),
            seen: BTreeSet::new(),
        };

        for seed in seeds {
            let _ = plan.admit(CrawlRequest::seed(seed));
        }

        Ok(plan)
    }

    pub fn admit(&mut self, request: CrawlRequest) -> CrawlAdmission {
        if matches!(request.url.scheme, Scheme::Http) && !self.limits.allow_http {
            return CrawlAdmission::Rejected(CrawlRejectReason::HttpDisabled);
        }
        if !self.seed_hosts.contains(&request.url.host) {
            return CrawlAdmission::Rejected(CrawlRejectReason::CrossHost);
        }
        if request.depth > self.limits.max_depth {
            return CrawlAdmission::Rejected(CrawlRejectReason::DepthLimit);
        }

        let canonical = request.url.canonical_string();
        if canonical.len() > self.limits.max_url_bytes {
            return CrawlAdmission::Rejected(CrawlRejectReason::UrlTooLong);
        }
        if self.exclusions.host_is_excluded(&request.url.host)
            || self.exclusions.url_is_excluded(&request.url)
        {
            return CrawlAdmission::Rejected(CrawlRejectReason::RobotsExcluded);
        }
        if self.seen.contains(&canonical) {
            return CrawlAdmission::Rejected(CrawlRejectReason::Duplicate);
        }
        if self.seen.len() >= self.limits.max_seen_urls {
            return CrawlAdmission::Rejected(CrawlRejectReason::SeenLimit);
        }
        if self.pending.len() >= self.limits.max_pending_urls {
            return CrawlAdmission::Rejected(CrawlRejectReason::PendingLimit);
        }

        self.seen.insert(canonical);
        self.pending.push_back(request);
        CrawlAdmission::Accepted
    }

    /// Admit a batch in caller order, returning one outcome per request.
    ///
    /// The batch is not atomic: accepted requests remain queued even if a
    /// later request is rejected. This makes queue and seen limits explicit
    /// and keeps the result reproducible for runtimes processing link batches.
    pub fn admit_batch<I>(&mut self, requests: I) -> Vec<CrawlAdmission>
    where
        I: IntoIterator<Item = CrawlRequest>,
    {
        requests
            .into_iter()
            .map(|request| self.admit(request))
            .collect()
    }

    pub fn pop_next(&mut self) -> Option<CrawlRequest> {
        self.pending.pop_front()
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn seen_len(&self) -> usize {
        self.seen.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(value: &str) -> NormalizedHost {
        NormalizedHost::new(value).unwrap()
    }

    fn url(scheme: Scheme, host: &str, path: &str) -> CanonicalUrl {
        CanonicalUrl::new(scheme, self::host(host), None, path, None).unwrap()
    }

    fn strict_plan(seed: CanonicalUrl) -> CrawlPlan {
        CrawlPlan::from_seeds(vec![seed], CrawlLimits::strict_single_host()).unwrap()
    }

    #[test]
    fn plan_requires_at_least_one_seed_and_nonzero_limits() {
        assert_eq!(
            CrawlPlan::from_seeds(Vec::new(), CrawlLimits::strict_single_host())
                .err()
                .unwrap(),
            CrawlerError::EmptySeedSet
        );

        let mut limits = CrawlLimits::strict_single_host();
        limits.max_pending_urls = 0;
        assert_eq!(
            CrawlPlan::from_seeds(vec![url(Scheme::Https, "example.org", "/")], limits)
                .err()
                .unwrap(),
            CrawlerError::InvalidLimit
        );

        assert_eq!(
            CrawlPlan::from_seeds(
                vec![
                    url(Scheme::Https, "example.org", "/"),
                    url(Scheme::Https, "other.example", "/")
                ],
                CrawlLimits::strict_single_host()
            )
            .err()
            .unwrap(),
            CrawlerError::MultipleSeedHosts
        );
    }

    #[test]
    fn seeds_are_admitted_in_input_order_but_seen_deduplicates_by_canonical_url() {
        let first = url(Scheme::Https, "example.org", "/a");
        let duplicate = url(Scheme::Https, "Example.Org.", "/a");
        let second = url(Scheme::Https, "example.org", "/b");

        let mut plan = CrawlPlan::from_seeds(
            vec![first.clone(), duplicate, second.clone()],
            CrawlLimits::strict_single_host(),
        )
        .unwrap();

        assert_eq!(plan.pending_len(), 2);
        assert_eq!(plan.seen_len(), 2);
        assert_eq!(plan.pop_next().unwrap().url, first);
        assert_eq!(plan.pop_next().unwrap().url, second);
        assert!(plan.pop_next().is_none());
    }

    #[test]
    fn crawler_is_https_only_by_default() {
        let seed = url(Scheme::Https, "example.org", "/");
        let mut plan = strict_plan(seed.clone());

        assert_eq!(
            plan.admit(CrawlRequest::discovered(
                url(Scheme::Http, "example.org", "/plain"),
                1,
                seed
            )),
            CrawlAdmission::Rejected(CrawlRejectReason::HttpDisabled)
        );
    }

    #[test]
    fn cross_host_discoveries_are_rejected_by_default() {
        let seed = url(Scheme::Https, "example.org", "/");
        let mut plan = strict_plan(seed.clone());

        assert_eq!(
            plan.admit(CrawlRequest::discovered(
                url(Scheme::Https, "other.example", "/"),
                1,
                seed
            )),
            CrawlAdmission::Rejected(CrawlRejectReason::CrossHost)
        );
    }

    #[test]
    fn depth_and_queue_limits_are_enforced_before_fetch() {
        let seed = url(Scheme::Https, "example.org", "/");
        let mut limits = CrawlLimits::strict_single_host();
        limits.max_depth = 1;
        limits.max_pending_urls = 1;
        let mut plan = CrawlPlan::from_seeds(vec![seed.clone()], limits).unwrap();

        assert_eq!(
            plan.admit(CrawlRequest::discovered(
                url(Scheme::Https, "example.org", "/deep"),
                2,
                seed.clone()
            )),
            CrawlAdmission::Rejected(CrawlRejectReason::DepthLimit)
        );
        assert_eq!(
            plan.admit(CrawlRequest::discovered(
                url(Scheme::Https, "example.org", "/queued"),
                1,
                seed
            )),
            CrawlAdmission::Rejected(CrawlRejectReason::PendingLimit)
        );
    }

    #[test]
    fn robots_exclusions_are_explicit_admission_rejections() {
        let seed = url(Scheme::Https, "example.org", "/");
        let exclusions = CrawlExclusions::new()
            .exclude_url_prefix("https://example.org/private")
            .exclude_host(host("blocked.example"));
        let mut plan = CrawlPlan::from_seeds_with_exclusions(
            vec![seed.clone()],
            CrawlLimits::strict_single_host(),
            exclusions.clone(),
        )
        .unwrap();

        assert_eq!(plan.pending_len(), 1);
        assert_eq!(
            plan.admit(CrawlRequest::discovered(
                url(Scheme::Https, "example.org", "/private/a"),
                1,
                seed
            )),
            CrawlAdmission::Rejected(CrawlRejectReason::RobotsExcluded)
        );

        let blocked = CrawlPlan::from_seeds_with_exclusions(
            vec![url(Scheme::Https, "blocked.example", "/")],
            CrawlLimits::strict_single_host(),
            exclusions,
        )
        .unwrap();
        assert_eq!(blocked.pending_len(), 0);
    }

    #[test]
    fn url_byte_limit_uses_canonical_form() {
        let seed = url(Scheme::Https, "example.org", "/");
        let mut limits = CrawlLimits::strict_single_host();
        limits.max_url_bytes = "https://example.org/x".len() - 1;
        let mut plan = CrawlPlan::from_seeds(vec![seed.clone()], limits).unwrap();

        assert_eq!(
            plan.admit(CrawlRequest::discovered(
                url(Scheme::Https, "example.org", "/x"),
                1,
                seed
            )),
            CrawlAdmission::Rejected(CrawlRejectReason::UrlTooLong)
        );
    }

    #[test]
    fn http_can_be_enabled_explicitly() {
        let mut limits = CrawlLimits::strict_single_host();
        limits.allow_http = true;
        let seed = url(Scheme::Http, "example.org", "/");
        let plan = CrawlPlan::from_seeds(vec![seed], limits).unwrap();

        assert_eq!(plan.pending_len(), 1);
    }

    #[test]
    fn batch_admission_preserves_order_and_partial_progress() {
        let seed = url(Scheme::Https, "example.org", "/");
        let mut limits = CrawlLimits::strict_single_host();
        limits.max_pending_urls = 2;
        let mut plan = strict_plan(seed.clone());
        plan.pop_next();
        plan.limits = limits;

        let outcomes = plan.admit_batch(vec![
            CrawlRequest::discovered(url(Scheme::Https, "example.org", "/a"), 1, seed.clone()),
            CrawlRequest::discovered(url(Scheme::Https, "other.example", "/b"), 1, seed.clone()),
            CrawlRequest::discovered(url(Scheme::Https, "example.org", "/c"), 1, seed),
        ]);

        assert_eq!(outcomes[0], CrawlAdmission::Accepted);
        assert_eq!(
            outcomes[1],
            CrawlAdmission::Rejected(CrawlRejectReason::CrossHost)
        );
        assert_eq!(outcomes[2], CrawlAdmission::Accepted);
        assert_eq!(plan.pending_len(), 2);
    }
}
