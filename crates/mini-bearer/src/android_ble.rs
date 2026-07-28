//! [`AndroidBleBearer`] — the [`crate::Bearer`] half of
//! [`crate::ble`]'s own module doc: *"A full `impl Bearer for
//! AndroidBleBearer` needs a UniFFI callback interface... so Rust can ask
//! Kotlin to perform the actual radio I/O... This module is the protocol
//! logic underneath it, ready to be driven by either side."*
//! [`BleRadio`] is that callback boundary, expressed here as a plain Rust
//! trait rather than a UniFFI-decorated one — see "Honest limit" below.
//! [`AndroidBleBearer`] drives any [`BleRadio`] through
//! [`crate::ble::chunk_frame`]/[`crate::ble::ChunkReassembler`] to
//! implement the full [`crate::Bearer`] trait generically.
//!
//! **Honest limit — what this closes and what it does not.** This is the
//! Rust-side half of Android beta slice 5 (issue #201). It is not wired
//! into `mini-ffi`'s UniFFI boundary yet (no `.udl` callback interface
//! exists for this trait, unlike `mini-ffi::StorageCipher`, D-0338), and
//! no Kotlin `BluetoothGattServer`/`BluetoothGattCallback` implementation
//! exists — that remains the Kotlin-side half of #201's division of
//! labor, and Android CI's `assembleDebug` is the only real verification
//! gate for it once it exists (this environment has no JDK/Android SDK).
//! What *is* real and tested here: the chunking/reassembly wiring that
//! turns any [`BleRadio`] implementation — Kotlin's real one, a future
//! different platform's, or the mock used in this file's own tests — into
//! a complete, drop-in [`crate::Bearer`].
//!
//! Named `AndroidBleBearer` to match the name [`crate::ble`]'s own doc
//! comment already uses for this exact gap, despite nothing in this file
//! being Android-specific: any [`BleRadio`] implementer drives it
//! identically.

use crate::bearer::Bearer;
use crate::ble::{chunk_frame, ChunkReassembler};
use crate::error::Result;

/// What a real BLE radio implementation must provide for
/// [`AndroidBleBearer`] to drive it. Chunk-level, not frame-level —
/// [`AndroidBleBearer`] is what turns chunks into frames and back.
pub trait BleRadio {
    /// Send one already-chunked piece of a frame over the radio (one real
    /// GATT characteristic write or notify).
    fn write_chunk(&mut self, chunk: &[u8]) -> Result<()>;

    /// Block until the next chunk arrives from the peer.
    fn read_chunk(&mut self) -> Result<Vec<u8>>;

    /// Return the next chunk if one is already buffered, `Ok(None)`
    /// otherwise — must never block, matching
    /// [`crate::tcp::TcpBearer::try_recv`]'s contract.
    fn try_read_chunk(&mut self) -> Result<Option<Vec<u8>>>;
}

/// A [`crate::Bearer`] over any [`BleRadio`]: chunks outgoing frames to
/// fit the radio's negotiated MTU and reassembles incoming chunks back
/// into frames.
pub struct AndroidBleBearer<R: BleRadio> {
    radio: R,
    mtu: usize,
    reassembler: ChunkReassembler,
}

impl<R: BleRadio> std::fmt::Debug for AndroidBleBearer<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AndroidBleBearer")
            .field("mtu", &self.mtu)
            .field("reassembler", &self.reassembler)
            .finish_non_exhaustive()
    }
}

impl<R: BleRadio> AndroidBleBearer<R> {
    /// `mtu` is the usable bytes per chunk — the radio's negotiated ATT
    /// MTU minus whatever ATT/GATT overhead the caller has already
    /// accounted for — passed straight through to
    /// [`crate::ble::chunk_frame`], which rejects it if too small to fit
    /// even the chunk header.
    pub fn new(radio: R, mtu: usize) -> Self {
        AndroidBleBearer {
            radio,
            mtu,
            reassembler: ChunkReassembler::new(),
        }
    }
}

impl<R: BleRadio> Bearer for AndroidBleBearer<R> {
    fn send(&mut self, frame: &[u8]) -> Result<()> {
        for chunk in chunk_frame(frame, self.mtu)? {
            self.radio.write_chunk(&chunk)?;
        }
        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<u8>> {
        loop {
            let chunk = self.radio.read_chunk()?;
            if let Some(frame) = self.reassembler.push_chunk(&chunk)? {
                return Ok(frame);
            }
        }
    }

    fn try_recv(&mut self) -> Result<Option<Vec<u8>>> {
        loop {
            match self.radio.try_read_chunk()? {
                Some(chunk) => {
                    if let Some(frame) = self.reassembler.push_chunk(&chunk)? {
                        return Ok(Some(frame));
                    }
                    // Chunk consumed, frame still incomplete: loop back to
                    // try_read_chunk, which is itself non-blocking, so this
                    // never blocks the caller -- it just stops the moment
                    // nothing more is buffered.
                }
                None => return Ok(None),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::BearerError;
    use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};

    struct MockRadio {
        tx: Sender<Vec<u8>>,
        rx: Receiver<Vec<u8>>,
    }

    impl BleRadio for MockRadio {
        fn write_chunk(&mut self, chunk: &[u8]) -> Result<()> {
            self.tx
                .send(chunk.to_vec())
                .map_err(|_| BearerError::Closed)
        }
        fn read_chunk(&mut self) -> Result<Vec<u8>> {
            self.rx.recv().map_err(|_| BearerError::Closed)
        }
        fn try_read_chunk(&mut self) -> Result<Option<Vec<u8>>> {
            match self.rx.try_recv() {
                Ok(v) => Ok(Some(v)),
                Err(TryRecvError::Empty) => Ok(None),
                Err(TryRecvError::Disconnected) => Err(BearerError::Closed),
            }
        }
    }

    fn pair_with_mtu(mtu: usize) -> (AndroidBleBearer<MockRadio>, AndroidBleBearer<MockRadio>) {
        let (tx_a, rx_a) = channel();
        let (tx_b, rx_b) = channel();
        let radio_a = MockRadio { tx: tx_a, rx: rx_b };
        let radio_b = MockRadio { tx: tx_b, rx: rx_a };
        (
            AndroidBleBearer::new(radio_a, mtu),
            AndroidBleBearer::new(radio_b, mtu),
        )
    }

    fn pair() -> (AndroidBleBearer<MockRadio>, AndroidBleBearer<MockRadio>) {
        pair_with_mtu(64)
    }

    #[test]
    fn a_frame_larger_than_one_chunk_round_trips_through_chunking_and_reassembly() {
        let (mut a, mut b) = pair();
        let frame: Vec<u8> = (0u32..500).map(|i| (i % 251) as u8).collect();
        a.send(&frame).unwrap();
        assert_eq!(b.recv().unwrap(), frame);
    }

    #[test]
    fn multiple_frames_round_trip_in_order() {
        let (mut a, mut b) = pair();
        a.send(b"first frame").unwrap();
        a.send(b"second frame, a bit longer than the first")
            .unwrap();
        assert_eq!(b.recv().unwrap(), b"first frame");
        assert_eq!(
            b.recv().unwrap(),
            b"second frame, a bit longer than the first"
        );
    }

    #[test]
    fn try_recv_returns_none_with_nothing_pending_then_the_frame_once_sent() {
        let (mut a, mut b) = pair();
        assert_eq!(b.try_recv().unwrap(), None);
        a.send(b"eventually").unwrap();
        assert_eq!(b.try_recv().unwrap(), Some(b"eventually".to_vec()));
    }

    #[test]
    fn a_write_failure_when_the_peer_radio_is_gone_surfaces_through_send() {
        let (mut a, b) = pair();
        drop(b);
        let err = a.send(b"anyone there?").unwrap_err();
        assert_eq!(err, BearerError::Closed);
    }

    #[test]
    fn a_read_failure_when_the_peer_radio_is_gone_surfaces_through_recv() {
        let (a, mut b) = pair();
        drop(a);
        let err = b.recv().unwrap_err();
        assert_eq!(err, BearerError::Closed);
    }

    #[test]
    fn an_empty_frame_round_trips() {
        let (mut a, mut b) = pair();
        a.send(&[]).unwrap();
        assert_eq!(b.recv().unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn a_tiny_mtu_forces_many_chunks_and_still_reassembles_correctly() {
        let (mut a, mut b) = pair_with_mtu(5); // 4-byte header + 1 payload byte
        let frame = b"twenty bytes total!!".to_vec();
        a.send(&frame).unwrap();
        assert_eq!(b.recv().unwrap(), frame);
    }

    #[test]
    fn an_mtu_too_small_for_even_the_header_is_rejected_before_any_write() {
        let (mut a, _b) = pair_with_mtu(3);
        let err = a.send(b"data").unwrap_err();
        assert!(matches!(err, BearerError::MtuTooSmall { .. }));
    }
}
