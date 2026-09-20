//! Owner-started, session-only public sync scheduling. No endpoint is trusted
//! as an identity and no setting can silently enable networking on startup.

use std::time::{Duration, Instant};

const SESSION_LIMIT: Duration = Duration::from_secs(15 * 60);
const SYNC_INTERVAL: Duration = Duration::from_secs(30);
const MAX_RETRY: Duration = Duration::from_secs(120);

#[derive(Debug)]
pub struct NetworkSession {
    endpoint: String,
    expires: Instant,
    next_attempt: Instant,
    failures: u32,
}

impl NetworkSession {
    pub fn start(endpoint: &str, now: Instant) -> Result<Self, String> {
        let endpoint = endpoint.trim();
        if endpoint.is_empty() || endpoint.len() > 320 || endpoint.chars().any(char::is_whitespace)
        {
            return Err("Enter a peer hostname or IP and port.".into());
        }
        let (host, port) = endpoint
            .rsplit_once(':')
            .ok_or("A peer port is required.")?;
        if host.is_empty() || !port.parse::<u16>().is_ok_and(|port| port != 0) {
            return Err("Enter a peer hostname or IP with a non-zero port.".into());
        }
        Ok(Self {
            endpoint: endpoint.to_owned(),
            expires: now + SESSION_LIMIT,
            next_attempt: now,
            failures: 0,
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn expired(&self, now: Instant) -> bool {
        now >= self.expires
    }

    pub fn due(&self, now: Instant) -> bool {
        !self.expired(now) && now >= self.next_attempt
    }

    pub fn completed(&mut self, success: bool, now: Instant) {
        self.failures = if success {
            0
        } else {
            self.failures.saturating_add(1)
        };
        let delay = SYNC_INTERVAL.saturating_mul(1 << self.failures.min(2));
        self.next_attempt = now + delay.min(MAX_RETRY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_peer_backs_off_and_success_recovers() {
        let now = Instant::now();
        let mut session = NetworkSession::start("peer.example:46000", now).unwrap();
        assert!(session.due(now));
        session.completed(false, now);
        assert!(!session.due(now + Duration::from_secs(59)));
        assert!(session.due(now + Duration::from_secs(60)));
        session.completed(false, now);
        assert!(!session.due(now + Duration::from_secs(119)));
        session.completed(true, now);
        assert!(session.due(now + SYNC_INTERVAL));
    }

    #[test]
    fn consent_expires_even_if_every_attempt_failed() {
        let now = Instant::now();
        let session = NetworkSession::start("[::1]:46000", now).unwrap();
        assert!(session.expired(now + SESSION_LIMIT));
        assert!(!session.due(now + SESSION_LIMIT));
    }

    #[test]
    fn invalid_configuration_never_starts_a_session() {
        let now = Instant::now();
        for endpoint in ["", "host", ":123", "host:0", "host:65536", "host name:80"] {
            assert!(NetworkSession::start(endpoint, now).is_err());
        }
    }
}
