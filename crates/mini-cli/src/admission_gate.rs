//! Escalating admission cost for `mini service run` (the long-running
//! listener). Wires `mini_transport_security::AdmissionMeter` into the
//! handshake without changing the wire for honest, low-rate peers:
//!
//! - The client sends its hello as always. The server meters the connecting
//!   source (IPv4 address, or IPv6 /64). While the source is inside the free
//!   tier ([`FREE_BITS`]) the server answers normally: no extra frame, no work.
//! - Once a source connects often enough inside the 5 s window that its cost
//!   passes the free tier, the server answers the hello with a challenge frame
//!   instead of the handshake response. The client grinds the requested work
//!   (bound to the hello it sent) and replies with an 8-byte nonce; only then
//!   does the server send the real response.
//! - Past the meter's ceiling the connection is refused.
//!
//! Challenge frame: `MNADM1 | bits:u8 | seq:u32be | src_len:u8 | src`.
//! A client that does not understand it would fail the handshake, which is the
//! intended outcome for a flooding source.
//!
//! Honest limits: botnets and GPUs pay less per unit than a phone; this
//! raises cost and rate-limits, it does not make flooding impossible, and it
//! sits beside the existing handshake/worker caps. Unaudited.

use crate::error::{CliError, Result};
use mini_bearer::{Bearer, TcpBearer};
use mini_transport_security::{
    solve_admission_work, AdmissionMeter, AdmissionPolicy, AdmissionQuote,
};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

/// Required work bits at or below which no proof is demanded. With the
/// starting policy (8 base bits, +1 per request) the first five connections
/// from a source in any 5 s window are free.
pub(crate) const FREE_BITS: u8 = 12;
/// The most work a client will agree to grind (about 2^26 hashes, seconds on a
/// phone). Anything higher is reported as "server busy, try later".
const CLIENT_MAX_BITS: u8 = 26;
const MAGIC: &[u8] = b"MNADM1";

fn sync_err(e: impl std::fmt::Display) -> CliError {
    CliError::Sync(e.to_string())
}

/// Shared meter for one listener.
pub(crate) fn new_meter() -> Arc<Mutex<AdmissionMeter>> {
    Arc::new(Mutex::new(AdmissionMeter::new(
        AdmissionPolicy::starting_point(),
    )))
}

/// The metered identity of a connecting address: IPv4 address, or the IPv6
/// /64 (a single host commonly controls a whole /64).
pub(crate) fn source_key(ip: IpAddr) -> Vec<u8> {
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
pub(crate) struct Gate {
    meter: Arc<Mutex<AdmissionMeter>>,
    source: Vec<u8>,
}

impl std::fmt::Debug for Gate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Gate")
    }
}

impl Gate {
    pub(crate) fn new(meter: Arc<Mutex<AdmissionMeter>>, ip: IpAddr) -> Self {
        Gate {
            meter,
            source: source_key(ip),
        }
    }

    /// Meter this connection after its `hello` arrived and before the
    /// handshake response is sent. Returns an error to drop the connection.
    pub(crate) fn check(&self, bearer: &mut TcpBearer, hello: &[u8]) -> Result<()> {
        let now = crate::sequence::now_ms();
        let quote = {
            let mut m = self.meter.lock().map_err(|_| sync_err("admission lock"))?;
            if m.admit_free(&self.source, FREE_BITS, now) {
                return Ok(());
            }
            m.quote(&self.source, now)
        };
        let mut frame = MAGIC.to_vec();
        frame.push(quote.required_bits);
        frame.extend_from_slice(&quote.seq.to_be_bytes());
        frame.push(self.source.len() as u8);
        frame.extend_from_slice(&self.source);
        bearer.send(&frame).map_err(sync_err)?;
        let reply = bearer.recv().map_err(sync_err)?;
        let nonce: [u8; 8] = reply
            .as_slice()
            .try_into()
            .map_err(|_| sync_err("malformed admission reply"))?;
        let digest = *blake3::hash(hello).as_bytes();
        let mut m = self.meter.lock().map_err(|_| sync_err("admission lock"))?;
        m.admit(&self.source, &digest, u64::from_be_bytes(nonce), now)
            .map(|_| ())
            .map_err(|_| sync_err("admission work refused: too many recent connections"))
    }
}

/// Client side: read the handshake response, first solving the server's
/// admission challenge if it sent one instead.
pub(crate) fn client_response(bearer: &mut TcpBearer, hello: &[u8]) -> Result<Vec<u8>> {
    let first = bearer.recv().map_err(sync_err)?;
    let Some(rest) = first.strip_prefix(MAGIC) else {
        return Ok(first);
    };
    if rest.len() < 6 || rest.len() != 6 + usize::from(rest[5]) {
        return Err(sync_err("malformed admission challenge"));
    }
    let bits = rest[0];
    let seq = u32::from_be_bytes([rest[1], rest[2], rest[3], rest[4]]);
    let source = &rest[6..];
    if bits > CLIENT_MAX_BITS {
        return Err(sync_err(
            "peer is busy and asks for too much work right now; try again shortly",
        ));
    }
    let digest = *blake3::hash(hello).as_bytes();
    let quote = AdmissionQuote {
        required_bits: bits,
        seq,
    };
    let nonce = solve_admission_work(source, &digest, quote, 1u64 << (u32::from(bits) + 6))
        .ok_or_else(|| sync_err("could not satisfy admission work"))?;
    bearer.send(&nonce.to_be_bytes()).map_err(sync_err)?;
    bearer.recv().map_err(sync_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_bearer::Bearer;
    use std::net::{Ipv4Addr, TcpListener, TcpStream};

    fn policy() -> AdmissionPolicy {
        // Free for seq 0..=2 (10, 11, 12 bits), proof for seq 3..=4, refused after.
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

    /// One server-side connection: meter the hello, then answer "RESP".
    fn serve_once(listener: &TcpListener, gate: &Gate) -> Result<()> {
        let (stream, _) = listener.accept().unwrap();
        let mut b = TcpBearer::from_stream(stream).map_err(sync_err)?;
        let hello = b.recv().map_err(sync_err)?;
        gate.check(&mut b, &hello)?;
        b.send(b"RESP").map_err(sync_err)
    }

    fn dial(addr: std::net::SocketAddr, hello: &[u8]) -> Result<Vec<u8>> {
        let mut b = TcpBearer::from_stream(TcpStream::connect(addr).unwrap()).map_err(sync_err)?;
        b.send(hello).map_err(sync_err)?;
        client_response(&mut b, hello)
    }

    #[test]
    fn free_tier_then_proof_then_refusal_over_real_tcp() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let meter = Arc::new(Mutex::new(AdmissionMeter::new(policy())));
        let gate = Gate::new(meter.clone(), IpAddr::V4(Ipv4Addr::LOCALHOST));
        let server = std::thread::spawn(move || {
            (0..6)
                .map(|_| serve_once(&listener, &gate).is_ok())
                .collect::<Vec<_>>()
        });
        let mut got = Vec::new();
        for i in 0..6u8 {
            got.push(dial(addr, &[i; 40]).ok());
        }
        let served = server.join().unwrap();
        // Five connections succeed (three free, two after grinding work); the
        // sixth is refused outright and the client sees a failed handshake.
        assert_eq!(served, vec![true, true, true, true, true, false]);
        assert!(got[..5].iter().all(|r| r.as_deref() == Some(&b"RESP"[..])));
        assert!(got[5].is_none());
    }

    #[test]
    fn a_client_that_skips_the_work_is_dropped() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let meter = Arc::new(Mutex::new(AdmissionMeter::new(policy())));
        // Burn the free tier so the next connection is challenged.
        for _ in 0..3 {
            assert!(meter.lock().unwrap().admit_free(
                &source_key(IpAddr::V4(Ipv4Addr::LOCALHOST)),
                FREE_BITS,
                crate::sequence::now_ms()
            ));
        }
        let gate = Gate::new(meter, IpAddr::V4(Ipv4Addr::LOCALHOST));
        let server = std::thread::spawn(move || serve_once(&listener, &gate));
        let mut b = TcpBearer::from_stream(TcpStream::connect(addr).unwrap()).unwrap();
        b.send(b"hello-bytes").unwrap();
        let challenge = b.recv().unwrap();
        assert!(challenge.starts_with(MAGIC));
        b.send(&[0u8; 8]).unwrap(); // no real work
        assert!(server.join().unwrap().is_err());
    }

    #[test]
    fn ipv6_sources_are_metered_per_slash_64() {
        let a = source_key("2001:db8::1".parse().unwrap());
        let b = source_key("2001:db8::ffff".parse().unwrap());
        let c = source_key("2001:db8:0:1::1".parse().unwrap());
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
