//! Owner-started connection sessions: a bounded schedule of public (and,
//! when the owner opts in, private-route) exchanges with the peers the owner
//! saved. No endpoint is trusted as an identity, no session outlives the
//! process, and nothing here opens a socket — the scheduler only says *which*
//! saved peer is due next. Whether a session may start on launch is a
//! separate, persisted, default-off owner choice in `connectivity`.

use std::time::{Duration, Instant};

/// Interval between successful exchanges with the same peer.
pub const SYNC_INTERVAL: Duration = Duration::from_secs(30);
/// Longest wait between retries against a peer that keeps failing.
const MAX_RETRY: Duration = Duration::from_secs(120);
/// Upper bound on saved peers a session will cycle through.
pub const MAX_SESSION_PEERS: usize = 32;

/// How long a session may run before it stops scheduling new exchanges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLength {
    /// Fifteen minutes, then stop.
    Short,
    /// One hour, then stop.
    Hour,
    /// Until the owner stops it or closes the application.
    WhileOpen,
}

impl SessionLength {
    pub fn limit(self) -> Option<Duration> {
        match self {
            SessionLength::Short => Some(Duration::from_secs(15 * 60)),
            SessionLength::Hour => Some(Duration::from_secs(60 * 60)),
            SessionLength::WhileOpen => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SessionLength::Short => "15 minutes",
            SessionLength::Hour => "1 hour",
            SessionLength::WhileOpen => "While Mininet is open",
        }
    }

    pub fn code(self) -> u8 {
        match self {
            SessionLength::Short => 0,
            SessionLength::Hour => 1,
            SessionLength::WhileOpen => 2,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(SessionLength::Short),
            1 => Some(SessionLength::Hour),
            2 => Some(SessionLength::WhileOpen),
            _ => None,
        }
    }
}

/// One saved peer's place in the schedule.
#[derive(Debug, Clone)]
pub struct PeerSlot {
    endpoint: String,
    failures: u32,
    successes: u32,
    next_attempt: Instant,
    last_ok: Option<bool>,
    last_summary: String,
}

impl PeerSlot {
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn last_ok(&self) -> Option<bool> {
        self.last_ok
    }

    pub fn last_summary(&self) -> &str {
        &self.last_summary
    }

    pub fn successes(&self) -> u32 {
        self.successes
    }

    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// Seconds until this peer is due again; zero when due now.
    pub fn due_in(&self, now: Instant) -> u64 {
        self.next_attempt.saturating_duration_since(now).as_secs()
    }
}

#[derive(Debug)]
pub struct NetworkSession {
    peers: Vec<PeerSlot>,
    started: Instant,
    expires: Option<Instant>,
    include_private: bool,
}

/// Validate a `host:port` / `[v6]:port` endpoint the owner typed. Accepts a
/// hostname because the dial path resolves it; rejects whitespace, empty
/// parts, port 0 and oversized input. Returns the trimmed endpoint.
pub fn validate_endpoint(endpoint: &str) -> Result<String, String> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err("Enter a peer hostname or IP and port.".into());
    }
    if endpoint.len() > 320 || endpoint.chars().any(char::is_whitespace) {
        return Err("A peer endpoint cannot contain spaces.".into());
    }
    let (host, port) = endpoint
        .rsplit_once(':')
        .ok_or("A peer port is required, for example peer.example:46000.")?;
    if host.is_empty() || host == "[]" || !port.parse::<u16>().is_ok_and(|port| port != 0) {
        return Err("Enter a peer hostname or IP with a non-zero port.".into());
    }
    if host.starts_with('[') != host.ends_with(']') {
        return Err("Write an IPv6 address in brackets: [2001:db8::1]:46000.".into());
    }
    Ok(endpoint.to_owned())
}

impl NetworkSession {
    pub fn start(
        endpoints: &[String],
        now: Instant,
        length: SessionLength,
        include_private: bool,
    ) -> Result<Self, String> {
        if endpoints.is_empty() {
            return Err("Save at least one peer before starting a session.".into());
        }
        if endpoints.len() > MAX_SESSION_PEERS {
            return Err(format!(
                "A session cycles through at most {MAX_SESSION_PEERS} saved peers."
            ));
        }
        let mut peers: Vec<PeerSlot> = Vec::with_capacity(endpoints.len());
        for endpoint in endpoints {
            let endpoint = validate_endpoint(endpoint)?;
            if peers.iter().any(|slot| slot.endpoint == endpoint) {
                continue;
            }
            peers.push(PeerSlot {
                endpoint,
                failures: 0,
                successes: 0,
                next_attempt: now,
                last_ok: None,
                last_summary: String::new(),
            });
        }
        Ok(Self {
            peers,
            started: now,
            expires: length.limit().map(|limit| now + limit),
            include_private,
        })
    }

    pub fn peers(&self) -> &[PeerSlot] {
        &self.peers
    }

    pub fn include_private(&self) -> bool {
        self.include_private
    }

    pub fn elapsed(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.started)
    }

    /// Time left before the session stops scheduling, or `None` for an
    /// open-ended session.
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        self.expires
            .map(|expires| expires.saturating_duration_since(now))
    }

    pub fn expired(&self, now: Instant) -> bool {
        self.expires.is_some_and(|expires| now >= expires)
    }

    /// The index of the peer whose next attempt is earliest and already due.
    pub fn next_due(&self, now: Instant) -> Option<usize> {
        if self.expired(now) {
            return None;
        }
        self.peers
            .iter()
            .enumerate()
            .filter(|(_, slot)| now >= slot.next_attempt)
            .min_by_key(|(_, slot)| slot.next_attempt)
            .map(|(index, _)| index)
    }

    pub fn endpoint(&self, index: usize) -> Option<&str> {
        self.peers.get(index).map(|slot| slot.endpoint.as_str())
    }

    /// Record the outcome of an exchange with `index` and schedule its next
    /// attempt: the regular interval after success, doubling (capped) after
    /// consecutive failures.
    pub fn completed(&mut self, index: usize, success: bool, summary: String, now: Instant) {
        let Some(slot) = self.peers.get_mut(index) else {
            return;
        };
        if success {
            slot.failures = 0;
            slot.successes = slot.successes.saturating_add(1);
        } else {
            slot.failures = slot.failures.saturating_add(1);
        }
        let delay = SYNC_INTERVAL.saturating_mul(1 << slot.failures.min(2));
        slot.next_attempt = now + delay.min(MAX_RETRY);
        slot.last_ok = Some(success);
        slot.last_summary = summary;
    }

    /// Ask for an exchange with every peer as soon as possible (owner pressed
    /// "Sync now"). Backoff counters are kept so a dead peer still slows down
    /// again after the manual attempt.
    pub fn sync_now(&mut self, now: Instant) {
        for slot in &mut self.peers {
            slot.next_attempt = now;
        }
    }

    pub fn any_success(&self) -> bool {
        self.peers.iter().any(|slot| slot.last_ok == Some(true))
    }

    pub fn all_failed(&self) -> bool {
        !self.peers.is_empty() && self.peers.iter().all(|slot| slot.last_ok == Some(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(endpoints: &[&str], length: SessionLength) -> (NetworkSession, Instant) {
        let now = Instant::now();
        let endpoints: Vec<String> = endpoints.iter().map(|s| s.to_string()).collect();
        (
            NetworkSession::start(&endpoints, now, length, false).unwrap(),
            now,
        )
    }

    #[test]
    fn unavailable_peer_backs_off_and_success_recovers() {
        let (mut session, now) = session(&["peer.example:46000"], SessionLength::Short);
        assert_eq!(session.next_due(now), Some(0));
        session.completed(0, false, "refused".into(), now);
        assert_eq!(session.next_due(now + Duration::from_secs(59)), None);
        assert_eq!(session.next_due(now + Duration::from_secs(60)), Some(0));
        session.completed(0, false, "refused".into(), now);
        assert_eq!(session.next_due(now + Duration::from_secs(119)), None);
        session.completed(0, false, "refused".into(), now);
        // Capped: a third failure still waits two minutes, not four.
        assert_eq!(session.next_due(now + Duration::from_secs(120)), Some(0));
        session.completed(0, true, "ok".into(), now);
        assert_eq!(session.next_due(now + SYNC_INTERVAL), Some(0));
        assert_eq!(session.peers()[0].successes(), 1);
        assert!(session.any_success());
    }

    #[test]
    fn consent_expires_even_if_every_attempt_failed() {
        let (session, now) = session(&["[::1]:46000"], SessionLength::Short);
        let limit = SessionLength::Short.limit().unwrap();
        assert!(session.expired(now + limit));
        assert_eq!(session.next_due(now + limit), None);
        assert_eq!(session.remaining(now + limit), Some(Duration::ZERO));
    }

    #[test]
    fn open_ended_session_never_expires_by_itself() {
        let (session, now) = session(&["peer.example:46000"], SessionLength::WhileOpen);
        assert!(!session.expired(now + Duration::from_secs(86_400 * 30)));
        assert_eq!(session.remaining(now), None);
    }

    #[test]
    fn several_peers_are_scheduled_earliest_first_and_deduplicated() {
        let (mut session, now) = session(
            &["a.example:1", "b.example:2", "a.example:1"],
            SessionLength::Hour,
        );
        assert_eq!(session.peers().len(), 2);
        assert_eq!(session.next_due(now), Some(0));
        session.completed(0, true, "ok".into(), now);
        assert_eq!(session.next_due(now), Some(1));
        session.completed(1, false, "down".into(), now);
        // a is due after 30s, b after 60s.
        assert_eq!(session.next_due(now + Duration::from_secs(30)), Some(0));
        session.completed(0, true, "ok".into(), now + Duration::from_secs(30));
        assert_eq!(session.next_due(now + Duration::from_secs(45)), None);
        // Both become due at t=60; the tie goes to the lower index, and the
        // failed peer is served right after it.
        assert_eq!(session.next_due(now + Duration::from_secs(60)), Some(0));
        session.completed(0, true, "ok".into(), now + Duration::from_secs(60));
        assert_eq!(session.next_due(now + Duration::from_secs(60)), Some(1));
        session.sync_now(now + Duration::from_secs(61));
        assert_eq!(session.next_due(now + Duration::from_secs(61)), Some(0));
    }

    #[test]
    fn invalid_configuration_never_starts_a_session() {
        let now = Instant::now();
        for endpoint in [
            "",
            "host",
            ":123",
            "host:0",
            "host:65536",
            "host name:80",
            "[::1:46000",
        ] {
            assert!(
                NetworkSession::start(&[endpoint.to_string()], now, SessionLength::Short, false)
                    .is_err(),
                "{endpoint:?} should be rejected"
            );
        }
        assert!(NetworkSession::start(&[], now, SessionLength::Short, false).is_err());
        assert_eq!(
            validate_endpoint("  peer.example:46000 ").unwrap(),
            "peer.example:46000"
        );
        assert_eq!(validate_endpoint("[::1]:46000").unwrap(), "[::1]:46000");
    }

    #[test]
    fn session_length_codes_round_trip() {
        for length in [
            SessionLength::Short,
            SessionLength::Hour,
            SessionLength::WhileOpen,
        ] {
            assert_eq!(SessionLength::from_code(length.code()), Some(length));
        }
        assert_eq!(SessionLength::from_code(7), None);
    }
}
