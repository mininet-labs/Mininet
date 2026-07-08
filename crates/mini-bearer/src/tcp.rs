//! A real [`Bearer`] over TCP — the first bearer in this crate that leaves
//! one process. Everything else in this crate (the in-process bearer, the
//! anonymous [`crate::Channel`] on top) has always worked; this module is
//! what actually puts frames on a wire between two separate devices or
//! processes for the first time.
//!
//! ## What this is a stand-in for
//!
//! This crate's own docs describe BLE, local Wi-Fi/hotspot, and an internet
//! relay as "all just bearers... platform bearers bind behind the same
//! trait." TCP is not BLE — it needs an IP-reachable network (a shared LAN,
//! a hotspot, or a relay with a public address), not physical radio
//! proximity — but it is a real, honest network transport today, usable
//! for local-Wi-Fi-style connections and relay connections alike, and it
//! exercises the exact same [`Bearer`] contract a BLE adapter will need to
//! satisfy later. A real BLE adapter is still separate future work; this
//! is not it.
//!
//! ## Honest limits
//!
//! - **No authentication or encryption at this layer, by design.** A raw
//!   TCP bearer is a dumb pipe, same as the in-process bearer — anonymity
//!   and confidentiality are [`crate::Channel`]'s job, layered on top.
//!   Do not send anything over a bare [`TcpBearer`] that needs either
//!   property without a `Channel` wrapping it.
//! - **No NAT traversal, hole punching, or address discovery.** Both ends
//!   need a reachable address already (loopback, LAN, or a relay with a
//!   public IP) — this is transport, not peer discovery (that's
//!   `mini-net`'s job).
//! - **No reconnect or retry logic.** A dropped connection surfaces as
//!   [`BearerError::Closed`] on the next call; the caller redials.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};

use crate::bearer::{encode_frame, Bearer, FrameReader};
use crate::error::{BearerError, Result};

/// How many bytes to read from the socket per syscall while filling the
/// frame reader. Not a limit on frame size (see [`crate::bearer::MAX_FRAME_BYTES`]),
/// just a read buffer size.
const READ_CHUNK_BYTES: usize = 16 * 1024;

/// A [`Bearer`] over a real TCP connection.
///
/// Construct one with [`TcpBearer::connect`] (dial out) or
/// [`TcpBearer::from_stream`] (wrap an already-accepted connection — pair
/// it with a plain [`std::net::TcpListener`] on the accepting side; this
/// crate does not wrap the listener itself, since accepting is ordinary
/// `std::net` and needs no framing until a connection exists).
#[derive(Debug)]
pub struct TcpBearer {
    stream: TcpStream,
    reader: FrameReader,
}

impl TcpBearer {
    /// Dial out to `addr` and wrap the resulting connection.
    pub fn connect(addr: impl ToSocketAddrs) -> Result<Self> {
        let stream = TcpStream::connect(addr)?;
        Self::from_stream(stream)
    }

    /// Wrap an already-connected [`TcpStream`] (typically from
    /// [`std::net::TcpListener::accept`]).
    pub fn from_stream(stream: TcpStream) -> Result<Self> {
        // Frames are already whole application messages; don't let the
        // kernel batch small ones and add latency no demo or real presence
        // round-trip should have to pay for.
        stream.set_nodelay(true)?;
        Ok(TcpBearer {
            stream,
            reader: FrameReader::new(),
        })
    }
}

impl Bearer for TcpBearer {
    fn send(&mut self, frame: &[u8]) -> Result<()> {
        let encoded = encode_frame(frame)?;
        self.stream.write_all(&encoded)?;
        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<u8>> {
        loop {
            if let Some(frame) = self.reader.next_frame()? {
                return Ok(frame);
            }
            let mut buf = [0u8; READ_CHUNK_BYTES];
            let n = self.stream.read(&mut buf)?;
            if n == 0 {
                return Err(BearerError::Closed);
            }
            self.reader.push(&buf[..n])?;
        }
    }

    fn try_recv(&mut self) -> Result<Option<Vec<u8>>> {
        if let Some(frame) = self.reader.next_frame()? {
            return Ok(Some(frame));
        }
        self.stream.set_nonblocking(true)?;
        let mut buf = [0u8; READ_CHUNK_BYTES];
        let result = self.stream.read(&mut buf);
        // Always restore blocking mode before returning, on every path --
        // recv() above assumes a blocking stream.
        self.stream.set_nonblocking(false)?;
        match result {
            Ok(0) => Err(BearerError::Closed),
            Ok(n) => {
                self.reader.push(&buf[..n])?;
                self.reader.next_frame()
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    use super::*;

    /// Bind an ephemeral loopback port and return (listener, address) so
    /// tests never race on a fixed port number.
    fn ephemeral_listener() -> (TcpListener, std::net::SocketAddr) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        (listener, addr)
    }

    #[test]
    fn a_frame_sent_by_one_side_arrives_intact_on_the_other() {
        let (listener, addr) = ephemeral_listener();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut bearer = TcpBearer::from_stream(stream).unwrap();
            bearer.recv().unwrap()
        });

        let mut client = TcpBearer::connect(addr).unwrap();
        client.send(b"hello over a real socket").unwrap();

        let received = server.join().unwrap();
        assert_eq!(received, b"hello over a real socket");
    }

    #[test]
    fn traffic_flows_in_both_directions_on_the_same_connection() {
        let (listener, addr) = ephemeral_listener();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut bearer = TcpBearer::from_stream(stream).unwrap();
            let from_client = bearer.recv().unwrap();
            bearer.send(b"reply from server").unwrap();
            from_client
        });

        let mut client = TcpBearer::connect(addr).unwrap();
        client.send(b"hello from client").unwrap();
        let reply = client.recv().unwrap();

        assert_eq!(server.join().unwrap(), b"hello from client");
        assert_eq!(reply, b"reply from server");
    }

    #[test]
    fn multiple_frames_pipelined_back_to_back_are_split_correctly() {
        let (listener, addr) = ephemeral_listener();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut bearer = TcpBearer::from_stream(stream).unwrap();
            (
                bearer.recv().unwrap(),
                bearer.recv().unwrap(),
                bearer.recv().unwrap(),
            )
        });

        let mut client = TcpBearer::connect(addr).unwrap();
        client.send(b"one").unwrap();
        client.send(b"two").unwrap();
        client.send(b"three").unwrap();

        let (a, b, c) = server.join().unwrap();
        assert_eq!(
            (a, b, c),
            (b"one".to_vec(), b"two".to_vec(), b"three".to_vec())
        );
    }

    #[test]
    fn closing_the_peer_surfaces_as_closed_on_the_next_recv() {
        let (listener, addr) = ephemeral_listener();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            drop(stream); // close immediately without sending anything
        });

        let mut client = TcpBearer::connect(addr).unwrap();
        server.join().unwrap();
        // Give the FIN a moment to arrive; recv() should then see EOF.
        thread::sleep(Duration::from_millis(50));
        assert_eq!(client.recv().unwrap_err(), BearerError::Closed);
    }

    #[test]
    fn try_recv_returns_none_with_nothing_pending_then_the_frame_once_sent() {
        let (listener, addr) = ephemeral_listener();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
        });

        let mut client = TcpBearer::connect(addr).unwrap();
        assert_eq!(client.try_recv().unwrap(), None);

        let server_stream = server.join().unwrap();
        let mut server_bearer = TcpBearer::from_stream(server_stream).unwrap();
        server_bearer.send(b"eventually").unwrap();

        // Poll briefly -- real network delivery isn't instantaneous even on
        // loopback.
        let mut got = None;
        for _ in 0..100 {
            if let Some(frame) = client.try_recv().unwrap() {
                got = Some(frame);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(got, Some(b"eventually".to_vec()));
    }

    #[test]
    fn an_oversized_frame_is_rejected_before_anything_is_written() {
        let (listener, addr) = ephemeral_listener();
        let _server = thread::spawn(move || listener.accept().unwrap());

        let mut client = TcpBearer::connect(addr).unwrap();
        let huge = vec![0u8; crate::bearer::MAX_FRAME_BYTES + 1];
        assert_eq!(
            client.send(&huge).unwrap_err(),
            BearerError::FrameTooLarge {
                max: crate::bearer::MAX_FRAME_BYTES,
                got: huge.len(),
            }
        );
    }
}
