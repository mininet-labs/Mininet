use std::time::Duration;

/// Robots authorization supplied by the scheduler's independently maintained
/// robots cache. Unknown policy is never treated as permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RobotsDecision {
    Allowed,
    Excluded,
    Unknown,
}

/// Hard limits for one fetch, including all redirect hops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchLimits {
    pub max_redirects: usize,
    pub max_body_bytes: usize,
    pub request_timeout: Duration,
    pub connect_timeout: Duration,
    pub allow_http: bool,
    pub allow_nonstandard_ports: bool,
}

impl FetchLimits {
    pub fn public_web_default() -> Self {
        Self {
            max_redirects: 5,
            max_body_bytes: 8 * 1024 * 1024,
            request_timeout: Duration::from_secs(20),
            connect_timeout: Duration::from_secs(5),
            allow_http: false,
            allow_nonstandard_ports: false,
        }
    }

    pub(crate) fn valid(&self) -> bool {
        self.max_redirects <= 20
            && self.max_body_bytes > 0
            && self.max_body_bytes <= 64 * 1024 * 1024
            && !self.request_timeout.is_zero()
            && !self.connect_timeout.is_zero()
            && self.connect_timeout <= self.request_timeout
    }
}
