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
use mini_bearer::TcpBearer;
use mini_transport_security::{admission_client_response, AdmissionGate, AdmissionMeter};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

pub(crate) use mini_transport_security::new_admission_meter as new_meter;

fn sync_err(e: impl std::fmt::Display) -> CliError {
    CliError::Sync(e.to_string())
}

/// Server-side gate for one accepted TCP connection.
#[derive(Clone, Debug)]
pub(crate) struct Gate(AdmissionGate);

impl Gate {
    pub(crate) fn new(meter: Arc<Mutex<AdmissionMeter>>, ip: IpAddr) -> Self {
        Gate(AdmissionGate::new(meter, ip))
    }

    /// Meter this connection after its `hello` arrived and before the
    /// handshake response is sent. Returns an error to drop the connection.
    pub(crate) fn check(&self, bearer: &mut TcpBearer, hello: &[u8]) -> Result<()> {
        self.0
            .check(bearer, hello, crate::sequence::now_ms())
            .map_err(sync_err)
    }
}

/// Client side: read the handshake response, first solving the server's
/// admission challenge if it sent one instead.
pub(crate) fn client_response(bearer: &mut TcpBearer, hello: &[u8]) -> Result<Vec<u8>> {
    admission_client_response(bearer, hello).map_err(sync_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_bearer::Bearer;
    use mini_transport_security::{source_key, FREE_BITS};
    use std::net::{Ipv4Addr, TcpListener, TcpStream};

    fn policy() -> mini_transport_security::AdmissionPolicy {
        // Free for seq 0..=2 (10, 11, 12 bits), proof for seq 3..=4, refused after.
        mini_transport_security::AdmissionPolicy {
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
        assert!(challenge.starts_with(b"MNADM1"));
        b.send(&[0u8; 8]).unwrap(); // no real work
        assert!(server.join().unwrap().is_err());
    }
}
