//! Handshake-time admission gate built on [`crate::AdmissionMeter`], generic
//! over any [`Bearer`], so every listener (CLI service, one-shot listeners,
//! the desktop delivery listener) shares one wire format and one set of rules.
//!
//! - The client sends its hello as always. The server meters the connecting
//!   source (IPv4 address, or IPv6 /64). While the source is inside the free
//!   tier ([`FREE_BITS`]) the server answers normally: no extra frame, no work.
//! - Past the free tier the server answers the hello with a challenge frame
//!   `MNADM1 | bits:u8 | seq:u32be | src_len:u8 | src` instead of the handshake
//!   response. The client grinds BLAKE3 work bound to its own hello and replies
//!   with an 8-byte nonce; only then does the server send the real response.
//! - Past the meter's ceiling the connection is refused.
//!
//! Honest limits: botnets and GPUs pay less per unit than a phone; this raises
//! cost and rate-limits, it does not make flooding impossible. Many honest
//! users behind one NAT share a meter. Unaudited.

use crate::admission::{solve, AdmissionMeter, AdmissionPolicy, Quote};
use mini_bearer::Bearer;
use mini_crypto::hash::blake3_256;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

/// Required work bits at or below which no proof is demanded. With the
/// starting policy (8 base bits, +1 per request) the first five connections
/// from a source in any 5 s window are free.
pub const FREE_BITS: u8 = 12;
/// The most work a client agrees to grind (about 2^26 hashes). Anything higher
/// is reported as [`GateError::Busy`].
pub const CLIENT_MAX_BITS: u8 = 26;
const MAGIC: &[u8] = b"MNADM1";

/// Why the gate dropped or failed a connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateError {
    /// The underlying bearer failed.
    Transport(String),
    /// The peer sent a malformed challenge or reply.
    Malformed,
    /// Server side: the work was missing, wrong, or the source is exhausted.
    Refused,
    /// Client side: the server asks for more work than we will grind.
    Busy,
    /// The shared meter lock was poisoned.
    Internal,
}

impl std::fmt::Display for GateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GateError::Transport(e) => write!(f, "{e}"),
            GateError::Malformed => f.write_str("malformed admission frame"),
            GateError::Refused => {
                f.write_str("admission work refused: too many recent connections")
            }
            GateError::Busy => {
                f.write_str("peer is busy and asks for too much work right now; try again shortly")
            }
            GateError::Internal => f.write_str("admission state unavailable"),
        }
    }
}
impl std::error::Error for GateError {}

fn transport(e: impl std::fmt::Display) -> GateError {
    GateError::Transport(e.to_string())
}

/// A fresh shared meter with the starting policy.
pub fn new_meter() -> Arc<Mutex<AdmissionMeter>> {
    Arc::new(Mutex::new(AdmissionMeter::new(
        AdmissionPolicy::starting_point(),
    )))
}

/// The metered identity of a connecting address: IPv4 address, or the IPv6 /64
/// (a single host commonly controls a whole /64).
pub fn source_key(ip: IpAddr) -> Vec<u8> {
    match ip {
        IpAddr::V4(v4) => {
            let mut k = vec![4];
            k.extend_from_slice(&v4.octets());
            k
        }
        IpAddr::V6(v6) => {
            let mut k = vec![6];
            k.extend_from_slice(&v6.octets()[..8]);
            k
        }
    }
}

/// Server-side gate for one accepted connection.
#[derive(Clone)]
pub struct AdmissionGate {
    meter: Arc<Mutex<AdmissionMeter>>,
    source: Vec<u8>,
}

impl std::fmt::Debug for AdmissionGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AdmissionGate")
    }
}

impl AdmissionGate {
    /// Gate a connection from `ip` against a shared `meter`.
    pub fn new(meter: Arc<Mutex<AdmissionMeter>>, ip: IpAddr) -> Self {
        AdmissionGate {
            meter,
            source: source_key(ip),
        }
    }

    /// Meter this connection after its `hello` arrived and before the
    /// handshake response is sent. `Err` means: drop the connection.
    pub fn check<B: Bearer>(
        &self,
        bearer: &mut B,
        hello: &[u8],
        now_ms: u64,
    ) -> Result<(), GateError> {
        let quote = {
            let mut m = self.meter.lock().map_err(|_| GateError::Internal)?;
            if m.admit_free(&self.source, FREE_BITS, now_ms) {
                return Ok(());
            }
            m.quote(&self.source, now_ms)
        };
        let mut frame = MAGIC.to_vec();
        frame.push(quote.required_bits);
        frame.extend_from_slice(&quote.seq.to_be_bytes());
        frame.push(self.source.len() as u8);
        frame.extend_from_slice(&self.source);
        bearer.send(&frame).map_err(transport)?;
        let reply = bearer.recv().map_err(transport)?;
        let nonce: [u8; 8] = reply
            .as_slice()
            .try_into()
            .map_err(|_| GateError::Malformed)?;
        let digest = blake3_256(hello);
        let mut m = self.meter.lock().map_err(|_| GateError::Internal)?;
        m.admit(&self.source, &digest, u64::from_be_bytes(nonce), now_ms)
            .map(|_| ())
            .map_err(|_| GateError::Refused)
    }
}

/// Client side: read the handshake response, first solving the server's
/// admission challenge if it sent one instead.
pub fn client_response<B: Bearer>(bearer: &mut B, hello: &[u8]) -> Result<Vec<u8>, GateError> {
    let first = bearer.recv().map_err(transport)?;
    let Some(rest) = first.strip_prefix(MAGIC) else {
        return Ok(first);
    };
    if rest.len() < 6 || rest.len() != 6 + usize::from(rest[5]) {
        return Err(GateError::Malformed);
    }
    let bits = rest[0];
    let seq = u32::from_be_bytes([rest[1], rest[2], rest[3], rest[4]]);
    let source = &rest[6..];
    if bits > CLIENT_MAX_BITS {
        return Err(GateError::Busy);
    }
    let digest = blake3_256(hello);
    let quote = Quote {
        required_bits: bits,
        seq,
    };
    let nonce =
        solve(source, &digest, quote, 1u64 << (u32::from(bits) + 6)).ok_or(GateError::Busy)?;
    bearer.send(&nonce.to_be_bytes()).map_err(transport)?;
    bearer.recv().map_err(transport)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn policy() -> AdmissionPolicy {
        AdmissionPolicy {
            window_ms: 60_000,
            base_bits: 10,
            bits_per_request: 1,
            max_bits: 14,
            global_soft_limit: 1_000_000,
            global_max_extra_bits: 0,
            max_sources: 16,
        }
    }

    #[test]
    fn ipv6_sources_are_metered_per_slash_64() {
        let a = source_key("2001:db8::1".parse().unwrap());
        let b = source_key("2001:db8::ffff".parse().unwrap());
        let c = source_key("2001:db8:0:1::1".parse().unwrap());
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn free_tier_then_proof_then_refusal_over_an_in_process_bearer() {
        let meter = Arc::new(Mutex::new(AdmissionMeter::new(policy())));
        let gate = AdmissionGate::new(meter, IpAddr::V4(Ipv4Addr::LOCALHOST));
        let mut served = Vec::new();
        for i in 0..6u8 {
            let (mut client, mut server) = mini_bearer::pair();
            let hello = vec![i; 40];
            client.send(&hello).unwrap();
            let got = server.recv().unwrap();
            // Drive both ends from one thread: server decides, client answers.
            let outcome = std::thread::scope(|s| {
                let g = &gate;
                let h = s.spawn(move || {
                    let r = g.check(&mut server, &got, 1_000);
                    if r.is_ok() {
                        server.send(b"RESP").unwrap();
                    }
                    r
                });
                let resp = client_response(&mut client, &hello);
                (h.join().unwrap(), resp)
            });
            served.push(outcome);
        }
        let ok: Vec<bool> = served.iter().map(|(s, _)| s.is_ok()).collect();
        assert_eq!(ok, vec![true, true, true, true, true, false]);
        assert!(served[..5]
            .iter()
            .all(|(_, c)| c.as_deref() == Ok(&b"RESP"[..])));
    }

    #[test]
    fn a_client_that_skips_the_work_is_dropped() {
        let meter = Arc::new(Mutex::new(AdmissionMeter::new(policy())));
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        for _ in 0..3 {
            assert!(meter
                .lock()
                .unwrap()
                .admit_free(&source_key(ip), FREE_BITS, 500));
        }
        let gate = AdmissionGate::new(meter, ip);
        let (mut client, mut server) = mini_bearer::pair();
        client.send(b"hello-bytes").unwrap();
        let hello = server.recv().unwrap();
        let handle = std::thread::scope(|s| {
            let g = &gate;
            let h = s.spawn(move || g.check(&mut server, &hello, 600));
            let challenge = client.recv().unwrap();
            assert!(challenge.starts_with(MAGIC));
            client.send(&[0u8; 8]).unwrap();
            h.join().unwrap()
        });
        assert_eq!(handle, Err(GateError::Refused));
    }

    #[test]
    fn an_excessive_demand_is_reported_as_busy() {
        let (mut client, mut server) = mini_bearer::pair();
        let mut frame = MAGIC.to_vec();
        frame.push(CLIENT_MAX_BITS + 1);
        frame.extend_from_slice(&0u32.to_be_bytes());
        frame.push(0);
        server.send(&frame).unwrap();
        assert_eq!(client_response(&mut client, b"h"), Err(GateError::Busy));
    }
}
