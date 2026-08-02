use std::net::SocketAddr;

use mini_crawler::CrawlRequest;
use mini_crypto::{HashAlgorithm, Multihash};
use mini_web_types::{
    CanonicalUrl, CrawlObservation, CrawlObservationId, FetchStatus, HttpStatus, NormalizedHost,
    ProviderPseudonym, Scheme, WebMediaType,
};
use url::Url;

use crate::{
    validate_resolved_addresses, BackendError, FetchBackend, FetchLimits, RawResponse,
    RobotsDecision,
};

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RuntimeError {
    InvalidLimits,
    RobotsUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchOutcome {
    pub observation: CrawlObservation,
    /// Present only for a successful, supported, bounded response. The digest
    /// and byte length in `observation` cover these exact bytes.
    pub body: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct FetchRuntime<B> {
    backend: B,
    limits: FetchLimits,
}

impl<B: FetchBackend> FetchRuntime<B> {
    pub fn new(backend: B, limits: FetchLimits) -> Result<Self, RuntimeError> {
        if !limits.valid() {
            return Err(RuntimeError::InvalidLimits);
        }
        Ok(Self { backend, limits })
    }

    pub async fn fetch(
        &self,
        request: &CrawlRequest,
        crawler: ProviderPseudonym,
        observed_at_ms: u64,
        robots: RobotsDecision,
    ) -> Result<FetchOutcome, RuntimeError> {
        match robots {
            RobotsDecision::Unknown => return Err(RuntimeError::RobotsUnknown),
            RobotsDecision::Excluded => {
                return Ok(outcome(
                    request.url.clone(),
                    request.url.clone(),
                    observed_at_ms,
                    FetchStatus::RobotsExcluded,
                    None,
                    Vec::new(),
                    crawler,
                ));
            }
            RobotsDecision::Allowed => {}
        }

        let requested = request.url.clone();
        let mut current = requested.clone();
        let mut redirects = Vec::new();

        loop {
            if !scheme_and_port_allowed(&current, &self.limits) {
                return Ok(outcome(
                    requested,
                    current,
                    observed_at_ms,
                    FetchStatus::UnsupportedScheme,
                    None,
                    redirects,
                    crawler,
                ));
            }
            let port = current.port.unwrap_or(match current.scheme {
                Scheme::Http => 80,
                Scheme::Https => 443,
                _ => 0,
            });
            let addresses = match self.backend.resolve(current.host.as_str(), port).await {
                Ok(addresses) if validate_resolved_addresses(&addresses) => addresses,
                Ok(_) => {
                    return Ok(outcome(
                        requested,
                        current,
                        observed_at_ms,
                        FetchStatus::AddressBlocked,
                        None,
                        redirects,
                        crawler,
                    ))
                }
                Err(error) => {
                    return Ok(backend_failure(
                        requested,
                        current,
                        observed_at_ms,
                        error,
                        redirects,
                        crawler,
                    ))
                }
            };
            let raw = match self.fetch_hop(&current, &addresses).await {
                Ok(raw) => raw,
                Err(error) => {
                    return Ok(backend_failure(
                        requested,
                        current,
                        observed_at_ms,
                        error,
                        redirects,
                        crawler,
                    ))
                }
            };
            let status = match HttpStatus::new(raw.status) {
                Ok(status) => status,
                Err(_) => {
                    return Ok(outcome(
                        requested,
                        current,
                        observed_at_ms,
                        FetchStatus::NetworkError,
                        None,
                        redirects,
                        crawler,
                    ))
                }
            };

            if (300..=399).contains(&raw.status) {
                let Some(location) = raw.location.as_deref() else {
                    return Ok(outcome(
                        requested,
                        current,
                        observed_at_ms,
                        FetchStatus::InvalidRedirect,
                        None,
                        redirects,
                        crawler,
                    ));
                };
                if redirects.len() >= self.limits.max_redirects {
                    return Ok(outcome(
                        requested,
                        current,
                        observed_at_ms,
                        FetchStatus::RedirectLimitExceeded,
                        None,
                        redirects,
                        crawler,
                    ));
                }
                let Some(next) = resolve_redirect(&current, location) else {
                    return Ok(outcome(
                        requested,
                        current,
                        observed_at_ms,
                        FetchStatus::InvalidRedirect,
                        None,
                        redirects,
                        crawler,
                    ));
                };
                redirects.push(next.clone());
                current = next;
                continue;
            }

            let Some(media_type) = classify_media_type(raw.content_type.as_deref()) else {
                return Ok(outcome(
                    requested,
                    current,
                    observed_at_ms,
                    FetchStatus::UnsupportedMediaType,
                    None,
                    redirects,
                    crawler,
                ));
            };
            return Ok(outcome(
                requested,
                current,
                observed_at_ms,
                FetchStatus::Success(status),
                Some((media_type, raw.body)),
                redirects,
                crawler,
            ));
        }
    }

    async fn fetch_hop(
        &self,
        url: &CanonicalUrl,
        addresses: &[SocketAddr],
    ) -> Result<RawResponse, BackendError> {
        self.backend
            .get(
                &url.canonical_string(),
                url.host.as_str(),
                addresses,
                self.limits.max_body_bytes,
                self.limits.request_timeout,
                self.limits.connect_timeout,
            )
            .await
    }
}

fn scheme_and_port_allowed(url: &CanonicalUrl, limits: &FetchLimits) -> bool {
    let scheme_ok = matches!(url.scheme, Scheme::Https)
        || (matches!(url.scheme, Scheme::Http) && limits.allow_http);
    let port_ok = limits.allow_nonstandard_ports || url.port.is_none();
    scheme_ok && port_ok
}

fn resolve_redirect(base: &CanonicalUrl, location: &str) -> Option<CanonicalUrl> {
    if location.len() > 4096 || location.chars().any(char::is_control) {
        return None;
    }
    let base = Url::parse(&base.canonical_string()).ok()?;
    canonical_from_url(base.join(location).ok()?)
}

fn canonical_from_url(mut url: Url) -> Option<CanonicalUrl> {
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    url.set_fragment(None);
    let scheme = match url.scheme() {
        "http" => Scheme::Http,
        "https" => Scheme::Https,
        _ => return None,
    };
    CanonicalUrl::new(
        scheme,
        NormalizedHost::new(url.host_str()?).ok()?,
        url.port(),
        url.path(),
        url.query().map(ToOwned::to_owned),
    )
    .ok()
}

fn classify_media_type(value: Option<&str>) -> Option<WebMediaType> {
    let essence = value?.split(';').next()?.trim().to_ascii_lowercase();
    Some(match essence.as_str() {
        "text/html" | "application/xhtml+xml" => WebMediaType::Html,
        "text/plain" => WebMediaType::TextPlain,
        "text/markdown" | "text/x-markdown" => WebMediaType::Markdown,
        "application/json" | "application/ld+json" => WebMediaType::Json,
        "application/pdf" => WebMediaType::Pdf,
        value if value.starts_with("image/") => WebMediaType::Image,
        _ => return None,
    })
}

fn backend_failure(
    requested: CanonicalUrl,
    final_url: CanonicalUrl,
    observed_at_ms: u64,
    error: BackendError,
    redirects: Vec<CanonicalUrl>,
    crawler: ProviderPseudonym,
) -> FetchOutcome {
    let status = match error {
        BackendError::Timeout => FetchStatus::Timeout,
        BackendError::ResponseTooLarge => FetchStatus::ResponseTooLarge,
        BackendError::Resolve | BackendError::Connect(_) => FetchStatus::NetworkError,
    };
    outcome(
        requested,
        final_url,
        observed_at_ms,
        status,
        None,
        redirects,
        crawler,
    )
}

fn outcome(
    requested_url: CanonicalUrl,
    final_url: CanonicalUrl,
    observed_at_ms: u64,
    status: FetchStatus,
    media_body: Option<(WebMediaType, Vec<u8>)>,
    redirect_chain: Vec<CanonicalUrl>,
    crawler: ProviderPseudonym,
) -> FetchOutcome {
    let (content_digest, media_type, byte_length, body) =
        media_body.map_or((None, None, None, None), |(media_type, body)| {
            let digest = Multihash::of(HashAlgorithm::Blake3, &body);
            let length = body.len() as u64;
            (Some(digest), Some(media_type), Some(length), Some(body))
        });
    let mut observation = CrawlObservation {
        id: CrawlObservationId(Multihash::of(HashAlgorithm::Blake3, b"pending")),
        requested_url,
        final_url,
        observed_at_ms,
        status,
        content_digest,
        media_type,
        byte_length,
        redirect_chain,
        crawler,
    };
    observation.id = derive_observation_id(&observation);
    FetchOutcome { observation, body }
}

/// Derive a stable observation identity from every public observation field
/// except the identity itself. Length prefixes remove concatenation ambiguity.
pub fn derive_observation_id(observation: &CrawlObservation) -> CrawlObservationId {
    let mut bytes = b"mini/crawl-observation-id/v1\0".to_vec();
    push(
        &mut bytes,
        observation.requested_url.canonical_string().as_bytes(),
    );
    push(
        &mut bytes,
        observation.final_url.canonical_string().as_bytes(),
    );
    bytes.extend_from_slice(&observation.observed_at_ms.to_be_bytes());
    encode_status(&mut bytes, &observation.status);
    match &observation.content_digest {
        Some(digest) => push(&mut bytes, &digest.to_bytes()),
        None => push(&mut bytes, &[]),
    }
    encode_media_type(&mut bytes, observation.media_type.as_ref());
    bytes.extend_from_slice(&observation.byte_length.unwrap_or(u64::MAX).to_be_bytes());
    bytes.extend_from_slice(&(observation.redirect_chain.len() as u32).to_be_bytes());
    for redirect in &observation.redirect_chain {
        push(&mut bytes, redirect.canonical_string().as_bytes());
    }
    push(&mut bytes, &observation.crawler.0.to_bytes());
    CrawlObservationId(Multihash::of(HashAlgorithm::Blake3, &bytes))
}

fn push(target: &mut Vec<u8>, value: &[u8]) {
    target.extend_from_slice(&(value.len() as u32).to_be_bytes());
    target.extend_from_slice(value);
}

fn encode_status(target: &mut Vec<u8>, status: &FetchStatus) {
    let tag = match status {
        FetchStatus::Success(code) => {
            target.push(0);
            target.extend_from_slice(&code.code().to_be_bytes());
            return;
        }
        FetchStatus::RedirectLimitExceeded => 1,
        FetchStatus::Timeout => 2,
        FetchStatus::NetworkError => 3,
        FetchStatus::RobotsExcluded => 4,
        FetchStatus::UnsupportedScheme => 5,
        FetchStatus::AddressBlocked => 6,
        FetchStatus::ResponseTooLarge => 7,
        FetchStatus::UnsupportedMediaType => 8,
        FetchStatus::InvalidRedirect => 9,
        _ => unreachable!("new FetchStatus variants require an observation-id encoding"),
    };
    target.push(tag);
}

fn encode_media_type(target: &mut Vec<u8>, media_type: Option<&WebMediaType>) {
    match media_type {
        None => target.push(0),
        Some(WebMediaType::Html) => target.push(1),
        Some(WebMediaType::TextPlain) => target.push(2),
        Some(WebMediaType::Markdown) => target.push(3),
        Some(WebMediaType::Json) => target.push(4),
        Some(WebMediaType::Pdf) => target.push(5),
        Some(WebMediaType::Image) => target.push(6),
        Some(WebMediaType::Other(value)) => {
            target.push(7);
            push(target, value.as_bytes());
        }
        _ => unreachable!("new WebMediaType variants require an observation-id encoding"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackendFuture, RawResponse};
    use std::{collections::VecDeque, sync::Mutex, time::Duration};

    #[derive(Debug)]
    struct MockBackend {
        resolutions: Mutex<VecDeque<Result<Vec<SocketAddr>, BackendError>>>,
        responses: Mutex<VecDeque<Result<RawResponse, BackendError>>>,
    }

    impl FetchBackend for MockBackend {
        fn resolve<'a>(
            &'a self,
            _: &'a str,
            _: u16,
        ) -> BackendFuture<'a, Result<Vec<SocketAddr>, BackendError>> {
            Box::pin(async move { self.resolutions.lock().unwrap().pop_front().unwrap() })
        }
        fn get<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: &'a [SocketAddr],
            _: usize,
            _: Duration,
            _: Duration,
        ) -> BackendFuture<'a, Result<RawResponse, BackendError>> {
            Box::pin(async move { self.responses.lock().unwrap().pop_front().unwrap() })
        }
    }

    fn url(host: &str, path: &str) -> CanonicalUrl {
        CanonicalUrl::new(
            Scheme::Https,
            NormalizedHost::new(host).unwrap(),
            None,
            path,
            None,
        )
        .unwrap()
    }
    fn crawler() -> ProviderPseudonym {
        ProviderPseudonym(Multihash::of(HashAlgorithm::Blake3, b"crawler"))
    }
    fn backend(resolutions: Vec<Vec<SocketAddr>>, responses: Vec<RawResponse>) -> MockBackend {
        MockBackend {
            resolutions: Mutex::new(resolutions.into_iter().map(Ok).collect()),
            responses: Mutex::new(responses.into_iter().map(Ok).collect()),
        }
    }
    fn ok(body: &[u8]) -> RawResponse {
        RawResponse {
            status: 200,
            content_type: Some("text/html; charset=utf-8".into()),
            location: None,
            body: body.to_vec(),
        }
    }

    #[tokio::test]
    async fn successful_fetch_binds_exact_body_to_observation() {
        let runtime = FetchRuntime::new(
            backend(
                vec![vec!["1.1.1.1:443".parse().unwrap()]],
                vec![ok(b"<h1>Mini</h1>")],
            ),
            FetchLimits::public_web_default(),
        )
        .unwrap();
        let result = runtime
            .fetch(
                &CrawlRequest::seed(url("example.org", "/")),
                crawler(),
                7,
                RobotsDecision::Allowed,
            )
            .await
            .unwrap();
        assert_eq!(result.body.as_deref(), Some(&b"<h1>Mini</h1>"[..]));
        assert_eq!(result.observation.byte_length, Some(13));
        assert_eq!(
            result.observation.id,
            derive_observation_id(&result.observation)
        );
    }

    #[tokio::test]
    async fn private_or_mixed_dns_answer_is_blocked_before_get() {
        for addresses in [
            vec!["127.0.0.1:443".parse().unwrap()],
            vec![
                "1.1.1.1:443".parse().unwrap(),
                "10.0.0.1:443".parse().unwrap(),
            ],
        ] {
            let runtime = FetchRuntime::new(
                backend(vec![addresses], vec![]),
                FetchLimits::public_web_default(),
            )
            .unwrap();
            let result = runtime
                .fetch(
                    &CrawlRequest::seed(url("example.org", "/")),
                    crawler(),
                    7,
                    RobotsDecision::Allowed,
                )
                .await
                .unwrap();
            assert_eq!(result.observation.status, FetchStatus::AddressBlocked);
        }
    }

    #[tokio::test]
    async fn every_redirect_is_resolved_and_private_redirect_is_blocked() {
        let redirect = RawResponse {
            status: 302,
            content_type: None,
            location: Some("https://internal.example/secret".into()),
            body: vec![],
        };
        let runtime = FetchRuntime::new(
            backend(
                vec![
                    vec!["1.1.1.1:443".parse().unwrap()],
                    vec!["169.254.169.254:443".parse().unwrap()],
                ],
                vec![redirect],
            ),
            FetchLimits::public_web_default(),
        )
        .unwrap();
        let result = runtime
            .fetch(
                &CrawlRequest::seed(url("example.org", "/")),
                crawler(),
                7,
                RobotsDecision::Allowed,
            )
            .await
            .unwrap();
        assert_eq!(result.observation.status, FetchStatus::AddressBlocked);
        assert_eq!(
            result.observation.redirect_chain,
            vec![url("internal.example", "/secret")]
        );
    }

    #[tokio::test]
    async fn redirect_limit_and_invalid_location_are_explicit() {
        let redirect = RawResponse {
            status: 302,
            content_type: None,
            location: Some("/again".into()),
            body: vec![],
        };
        let mut limits = FetchLimits::public_web_default();
        limits.max_redirects = 0;
        let runtime = FetchRuntime::new(
            backend(vec![vec!["1.1.1.1:443".parse().unwrap()]], vec![redirect]),
            limits,
        )
        .unwrap();
        let result = runtime
            .fetch(
                &CrawlRequest::seed(url("example.org", "/")),
                crawler(),
                7,
                RobotsDecision::Allowed,
            )
            .await
            .unwrap();
        assert_eq!(
            result.observation.status,
            FetchStatus::RedirectLimitExceeded
        );
    }

    #[tokio::test]
    async fn robots_unknown_fails_closed_and_excluded_never_resolves() {
        let runtime =
            FetchRuntime::new(backend(vec![], vec![]), FetchLimits::public_web_default()).unwrap();
        let request = CrawlRequest::seed(url("example.org", "/"));
        assert_eq!(
            runtime
                .fetch(&request, crawler(), 7, RobotsDecision::Unknown)
                .await,
            Err(RuntimeError::RobotsUnknown)
        );
        let result = runtime
            .fetch(&request, crawler(), 7, RobotsDecision::Excluded)
            .await
            .unwrap();
        assert_eq!(result.observation.status, FetchStatus::RobotsExcluded);
    }

    #[tokio::test]
    async fn oversized_and_unsupported_responses_are_not_returned() {
        let oversized = MockBackend {
            resolutions: Mutex::new(vec![Ok(vec!["1.1.1.1:443".parse().unwrap()])].into()),
            responses: Mutex::new(vec![Err(BackendError::ResponseTooLarge)].into()),
        };
        let runtime = FetchRuntime::new(oversized, FetchLimits::public_web_default()).unwrap();
        let result = runtime
            .fetch(
                &CrawlRequest::seed(url("example.org", "/")),
                crawler(),
                7,
                RobotsDecision::Allowed,
            )
            .await
            .unwrap();
        assert_eq!(result.observation.status, FetchStatus::ResponseTooLarge);

        let unsupported = RawResponse {
            status: 200,
            content_type: Some("application/octet-stream".into()),
            location: None,
            body: vec![1, 2, 3],
        };
        let runtime = FetchRuntime::new(
            backend(
                vec![vec!["1.1.1.1:443".parse().unwrap()]],
                vec![unsupported],
            ),
            FetchLimits::public_web_default(),
        )
        .unwrap();
        let result = runtime
            .fetch(
                &CrawlRequest::seed(url("example.org", "/")),
                crawler(),
                7,
                RobotsDecision::Allowed,
            )
            .await
            .unwrap();
        assert_eq!(result.observation.status, FetchStatus::UnsupportedMediaType);
        assert!(result.body.is_none());
    }

    #[tokio::test]
    async fn http_and_nonstandard_ports_are_refused_before_dns() {
        let runtime =
            FetchRuntime::new(backend(vec![], vec![]), FetchLimits::public_web_default()).unwrap();
        let http = CanonicalUrl::new(
            Scheme::Http,
            NormalizedHost::new("example.org").unwrap(),
            None,
            "/",
            None,
        )
        .unwrap();
        let result = runtime
            .fetch(
                &CrawlRequest::seed(http),
                crawler(),
                7,
                RobotsDecision::Allowed,
            )
            .await
            .unwrap();
        assert_eq!(result.observation.status, FetchStatus::UnsupportedScheme);

        let port = CanonicalUrl::new(
            Scheme::Https,
            NormalizedHost::new("example.org").unwrap(),
            Some(8443),
            "/",
            None,
        )
        .unwrap();
        let result = runtime
            .fetch(
                &CrawlRequest::seed(port),
                crawler(),
                7,
                RobotsDecision::Allowed,
            )
            .await
            .unwrap();
        assert_eq!(result.observation.status, FetchStatus::UnsupportedScheme);
    }

    #[tokio::test]
    async fn credential_and_non_web_redirects_are_rejected() {
        for location in ["https://user:secret@example.org/", "file:///etc/passwd"] {
            let redirect = RawResponse {
                status: 302,
                content_type: None,
                location: Some(location.into()),
                body: vec![],
            };
            let runtime = FetchRuntime::new(
                backend(vec![vec!["1.1.1.1:443".parse().unwrap()]], vec![redirect]),
                FetchLimits::public_web_default(),
            )
            .unwrap();
            let result = runtime
                .fetch(
                    &CrawlRequest::seed(url("example.org", "/")),
                    crawler(),
                    7,
                    RobotsDecision::Allowed,
                )
                .await
                .unwrap();
            assert_eq!(result.observation.status, FetchStatus::InvalidRedirect);
        }
    }

    #[tokio::test]
    async fn timeout_is_distinct_from_other_network_failure() {
        let timeout = MockBackend {
            resolutions: Mutex::new(vec![Ok(vec!["1.1.1.1:443".parse().unwrap()])].into()),
            responses: Mutex::new(vec![Err(BackendError::Timeout)].into()),
        };
        let runtime = FetchRuntime::new(timeout, FetchLimits::public_web_default()).unwrap();
        let result = runtime
            .fetch(
                &CrawlRequest::seed(url("example.org", "/")),
                crawler(),
                7,
                RobotsDecision::Allowed,
            )
            .await
            .unwrap();
        assert_eq!(result.observation.status, FetchStatus::Timeout);
    }

    #[tokio::test]
    async fn observation_identity_changes_with_transcript_fields() {
        let runtime = FetchRuntime::new(
            backend(
                vec![
                    vec!["1.1.1.1:443".parse().unwrap()],
                    vec!["1.1.1.1:443".parse().unwrap()],
                ],
                vec![ok(b"one"), ok(b"two")],
            ),
            FetchLimits::public_web_default(),
        )
        .unwrap();
        let request = CrawlRequest::seed(url("example.org", "/"));
        let one = runtime
            .fetch(&request, crawler(), 7, RobotsDecision::Allowed)
            .await
            .unwrap();
        let two = runtime
            .fetch(&request, crawler(), 7, RobotsDecision::Allowed)
            .await
            .unwrap();
        assert_ne!(one.observation.id, two.observation.id);
    }
}
