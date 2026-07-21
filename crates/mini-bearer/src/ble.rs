//! MTU-bounded chunking and reassembly for small-payload bearers (Android
//! beta slice 5, issue #201).
//!
//! ## What this is, and what it is not
//!
//! BLE GATT characteristic writes/notifications are capped by the
//! connection's negotiated ATT MTU — typically well under 512 bytes, far
//! smaller than a real [`crate::Bearer`] frame ([`crate::MAX_FRAME_BYTES`]
//! is 16 MiB). A BLE-backed [`crate::Bearer`] therefore cannot hand a whole
//! frame to one GATT operation the way [`crate::tcp::TcpBearer`] hands one
//! to a socket write; it must split the frame into MTU-sized chunks and
//! reassemble them on the other end. [`chunk_frame`] and
//! [`ChunkReassembler`] are exactly that splitting/reassembly logic —
//! deterministic, platform-independent, and fully testable without any
//! Bluetooth hardware.
//!
//! What this module does **not** do: perform any actual GATT read, write,
//! or notify. There is no Bluetooth stack in this environment to test
//! against, and Android's real `BluetoothGattServer`/`BluetoothGattCallback`
//! APIs are Kotlin-only. A full `impl Bearer for AndroidBleBearer` needs a
//! UniFFI callback interface (the same shape as `mini-ffi`'s
//! `StorageCipher`, D-0338) so Rust can ask Kotlin to perform the actual
//! radio I/O — that wiring, and the real two-device BLE test, are the
//! Kotlin-side half of issue #201's division of labor. This module is the
//! protocol logic underneath it, ready to be driven by either side.
//!
//! ## Wire format
//!
//! Each chunk is `chunk_index: u16 (BE) | chunk_count: u16 (BE) | payload`.
//! `chunk_count` never exceeds [`u16::MAX`]: a frame that would need more
//! chunks than that at the given `mtu` is rejected by [`chunk_frame`]
//! ([`crate::BearerError::TooManyChunks`]) rather than silently wrapping
//! the counter — a maximum-sized ([`crate::MAX_FRAME_BYTES`]) frame needs
//! an MTU of at least a few hundred bytes to fit within 65,535 chunks,
//! which any real negotiated BLE ATT MTU comfortably clears, but this
//! function makes no assumption about what `mtu` the caller passes.
//! [`ChunkReassembler`] independently caps the reassembled total at
//! [`crate::MAX_FRAME_BYTES`] regardless of what a chunk header claims.
//!
//! ## Fails closed, never silently reorders
//!
//! [`crate::Channel`]'s own doc note applies here too: "the encrypted
//! channel is intentionally in-order." [`ChunkReassembler`] rejects an
//! out-of-order chunk index or a chunk whose `chunk_count` disagrees with
//! the reassembly already in progress, rather than guessing how to merge
//! them — a confused or hostile peer gets an error, never a silently
//! corrupted reassembled frame.

use crate::error::{BearerError, Result};
use crate::MAX_FRAME_BYTES;

/// Byte width of one chunk's header (`chunk_index` + `chunk_count`, both `u16`).
const CHUNK_HEADER_BYTES: usize = 4;

/// Split `frame` into a sequence of MTU-sized chunks, each carrying a
/// 4-byte header. `mtu` is the number of bytes usable for one chunk
/// (header included) — the caller determines this from the real,
/// negotiated GATT MTU minus whatever ATT/GATT overhead the platform has
/// already accounted for; this function does not assume any particular
/// value.
///
/// Returns one chunk even for an empty frame (`chunk_count == 1`, empty
/// payload), so an empty frame round-trips through [`ChunkReassembler`]
/// like any other.
pub fn chunk_frame(frame: &[u8], mtu: usize) -> Result<Vec<Vec<u8>>> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err(BearerError::FrameTooLarge {
            max: MAX_FRAME_BYTES,
            got: frame.len(),
        });
    }
    if mtu <= CHUNK_HEADER_BYTES {
        return Err(BearerError::MtuTooSmall {
            min: CHUNK_HEADER_BYTES + 1,
            got: mtu,
        });
    }
    let payload_per_chunk = mtu - CHUNK_HEADER_BYTES;
    let chunk_count = frame.len().div_ceil(payload_per_chunk).max(1);
    if chunk_count > usize::from(u16::MAX) {
        return Err(BearerError::TooManyChunks {
            max: u16::MAX,
            needed: chunk_count,
        });
    }
    let chunk_count = chunk_count as u16;

    let mut chunks = Vec::with_capacity(chunk_count as usize);
    for (index, payload) in frame.chunks(payload_per_chunk).enumerate() {
        chunks.push(encode_chunk(index as u16, chunk_count, payload));
    }
    if chunks.is_empty() {
        chunks.push(encode_chunk(0, 1, &[]));
    }
    Ok(chunks)
}

fn encode_chunk(index: u16, count: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(CHUNK_HEADER_BYTES + payload.len());
    out.extend_from_slice(&index.to_be_bytes());
    out.extend_from_slice(&count.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Reassembles chunks produced by [`chunk_frame`] back into a frame.
///
/// One reassembler tracks one frame at a time; feed it every chunk of one
/// frame, in order, before starting the next. It has no notion of a
/// connection or peer identity — that belongs to whatever drives the real
/// GATT I/O.
#[derive(Debug, Default)]
pub struct ChunkReassembler {
    expected_index: u16,
    total: Option<u16>,
    buf: Vec<u8>,
}

impl ChunkReassembler {
    /// A fresh reassembler, ready for the first chunk of a frame.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one chunk. Returns the completed frame once its last chunk has
    /// arrived, `Ok(None)` while more are still expected, and an error —
    /// fail closed, no partial data used — the moment anything about the
    /// chunk stream doesn't add up.
    pub fn push_chunk(&mut self, chunk: &[u8]) -> Result<Option<Vec<u8>>> {
        if chunk.len() < CHUNK_HEADER_BYTES {
            return Err(BearerError::Truncated);
        }
        let index = u16::from_be_bytes([chunk[0], chunk[1]]);
        let count = u16::from_be_bytes([chunk[2], chunk[3]]);
        let payload = &chunk[CHUNK_HEADER_BYTES..];

        if count == 0 {
            return Err(BearerError::BadChunk);
        }
        match self.total {
            None => {
                if index != 0 {
                    return Err(BearerError::OutOfOrderChunk {
                        expected: 0,
                        got: index,
                    });
                }
                self.total = Some(count);
            }
            Some(expected_count) => {
                if count != expected_count {
                    return Err(BearerError::BadChunk);
                }
                if index != self.expected_index {
                    return Err(BearerError::OutOfOrderChunk {
                        expected: self.expected_index,
                        got: index,
                    });
                }
            }
        }

        let new_len =
            self.buf
                .len()
                .checked_add(payload.len())
                .ok_or(BearerError::FrameTooLarge {
                    max: MAX_FRAME_BYTES,
                    got: usize::MAX,
                })?;
        if new_len > MAX_FRAME_BYTES {
            return Err(BearerError::FrameTooLarge {
                max: MAX_FRAME_BYTES,
                got: new_len,
            });
        }
        self.buf.extend_from_slice(payload);
        self.expected_index += 1;

        if self.expected_index == count {
            let frame = std::mem::take(&mut self.buf);
            self.total = None;
            self.expected_index = 0;
            Ok(Some(frame))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reassemble_all(chunks: &[Vec<u8>]) -> Result<Vec<u8>> {
        let mut reassembler = ChunkReassembler::new();
        let mut result = None;
        for chunk in chunks {
            if let Some(frame) = reassembler.push_chunk(chunk)? {
                assert!(result.is_none(), "frame completed twice");
                result = Some(frame);
            }
        }
        result.ok_or(BearerError::Truncated)
    }

    #[test]
    fn a_frame_smaller_than_one_chunk_round_trips_as_a_single_chunk() {
        let frame = b"hello mininet";
        let chunks = chunk_frame(frame, 64).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(reassemble_all(&chunks).unwrap(), frame);
    }

    #[test]
    fn a_frame_spanning_many_chunks_round_trips_in_order() {
        let frame: Vec<u8> = (0u32..10_000).map(|i| (i % 251) as u8).collect();
        let chunks = chunk_frame(&frame, 37).unwrap();
        assert!(chunks.len() > 1);
        assert_eq!(reassemble_all(&chunks).unwrap(), frame);
    }

    #[test]
    fn an_empty_frame_round_trips_as_one_empty_chunk() {
        let chunks = chunk_frame(&[], 64).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(reassemble_all(&chunks).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn chunking_with_an_mtu_too_small_for_even_the_header_is_rejected() {
        let err = chunk_frame(b"data", CHUNK_HEADER_BYTES).unwrap_err();
        assert_eq!(
            err,
            BearerError::MtuTooSmall {
                min: CHUNK_HEADER_BYTES + 1,
                got: CHUNK_HEADER_BYTES
            }
        );
    }

    #[test]
    fn chunking_a_frame_larger_than_max_frame_bytes_is_rejected() {
        let frame = vec![0u8; MAX_FRAME_BYTES + 1];
        let err = chunk_frame(&frame, 64).unwrap_err();
        assert_eq!(
            err,
            BearerError::FrameTooLarge {
                max: MAX_FRAME_BYTES,
                got: MAX_FRAME_BYTES + 1
            }
        );
    }

    #[test]
    fn an_mtu_too_small_to_fit_a_max_size_frame_within_u16_chunks_is_rejected() {
        // At mtu = 5 (a 4-byte header plus 1 payload byte per chunk), a
        // maximum-size frame would need far more than u16::MAX chunks.
        let frame = vec![0u8; MAX_FRAME_BYTES];
        let err = chunk_frame(&frame, CHUNK_HEADER_BYTES + 1).unwrap_err();
        assert!(matches!(err, BearerError::TooManyChunks { .. }));
    }

    #[test]
    fn reassembly_rejects_a_chunk_shorter_than_the_header() {
        let mut reassembler = ChunkReassembler::new();
        let err = reassembler.push_chunk(&[0u8, 1, 2]).unwrap_err();
        assert_eq!(err, BearerError::Truncated);
    }

    #[test]
    fn reassembly_rejects_a_chunk_claiming_zero_total_chunks() {
        let mut reassembler = ChunkReassembler::new();
        let err = reassembler
            .push_chunk(&encode_chunk(0, 0, b"x"))
            .unwrap_err();
        assert_eq!(err, BearerError::BadChunk);
    }

    #[test]
    fn reassembly_rejects_starting_mid_sequence() {
        let mut reassembler = ChunkReassembler::new();
        let err = reassembler
            .push_chunk(&encode_chunk(1, 3, b"x"))
            .unwrap_err();
        assert_eq!(
            err,
            BearerError::OutOfOrderChunk {
                expected: 0,
                got: 1
            }
        );
    }

    #[test]
    fn reassembly_rejects_a_skipped_chunk_index() {
        let mut reassembler = ChunkReassembler::new();
        reassembler.push_chunk(&encode_chunk(0, 3, b"a")).unwrap();
        let err = reassembler
            .push_chunk(&encode_chunk(2, 3, b"c"))
            .unwrap_err();
        assert_eq!(
            err,
            BearerError::OutOfOrderChunk {
                expected: 1,
                got: 2
            }
        );
    }

    #[test]
    fn reassembly_rejects_a_mid_stream_chunk_count_change() {
        let mut reassembler = ChunkReassembler::new();
        reassembler.push_chunk(&encode_chunk(0, 3, b"a")).unwrap();
        let err = reassembler
            .push_chunk(&encode_chunk(1, 5, b"b"))
            .unwrap_err();
        assert_eq!(err, BearerError::BadChunk);
    }

    #[test]
    fn a_reassembler_can_be_reused_for_a_second_frame_after_completing_the_first() {
        let mut reassembler = ChunkReassembler::new();
        let first = chunk_frame(b"first frame", 6).unwrap();
        let mut completed = None;
        for chunk in &first {
            if let Some(frame) = reassembler.push_chunk(chunk).unwrap() {
                completed = Some(frame);
            }
        }
        assert_eq!(completed.unwrap(), b"first frame");

        let second = chunk_frame(b"second", 6).unwrap();
        let mut completed = None;
        for chunk in &second {
            if let Some(frame) = reassembler.push_chunk(chunk).unwrap() {
                completed = Some(frame);
            }
        }
        assert_eq!(completed.unwrap(), b"second");
    }

    #[test]
    fn round_tripping_at_a_realistic_ble_att_mtu_matches_the_original_frame() {
        // A conservative negotiated ATT MTU (23 bytes, the BLE default
        // before any MTU-exchange request) minus a few bytes of ATT
        // opcode/handle overhead a real platform layer would have already
        // stripped -- exercised here as a plain byte count, this module's
        // only real input.
        let frame: Vec<u8> = (0u32..2_000).map(|i| (i % 200) as u8).collect();
        let chunks = chunk_frame(&frame, 20).unwrap();
        assert_eq!(reassemble_all(&chunks).unwrap(), frame);
    }
}
