//! UniFFI BLE bearer boundary — the Kotlin-callable half of Android beta
//! slice 5 (issue #201), the exact follow-up D-0374 named when it shipped
//! `mini-bearer::android_ble`'s Rust-side-only `AndroidBleBearer`.
//!
//! `mini-bearer::android_ble::AndroidBleBearer<R>` is already a full,
//! tested `impl Bearer` generic over any `R: mini_bearer::BleRadio`. What
//! was missing is a way for a real Kotlin `BluetoothGattServer`/
//! `BluetoothGattCallback` implementation to actually *be* that `R` across
//! the UniFFI boundary — this module is exactly that, mirroring this
//! crate's existing [`StorageCipher`](crate::StorageCipher) callback
//! pattern (D-0338): [`BleRadio`] is a UniFFI callback interface Kotlin
//! implements and Rust calls, [`RadioAdapter`] bridges its `&self` shape
//! to `mini_bearer::BleRadio`'s `&mut self` shape, and [`BleBearerHandle`]
//! is the UniFFI object wrapping `mini_bearer::AndroidBleBearer` that
//! Kotlin drives with `send`/`recv`/`try_recv`.
//!
//! **Honest limit:** this module still never touches a real radio. No
//! Kotlin `BluetoothGattServer`/`BluetoothGattCallback` implementation of
//! [`BleRadio`] exists yet, and nothing here can be exercised end to end
//! without one — Android CI's `assembleDebug` plus a real two-device test
//! remain the only gates that actually prove this wiring works, exactly
//! as D-0374 named.

use std::sync::Mutex;

use mini_bearer::Bearer;

/// Caller-implemented BLE radio I/O boundary (issue #201), mirroring
/// [`crate::StorageCipher`]'s callback-interface shape (D-0338).
///
/// `mini-ffi` never touches Bluetooth itself. On Android, the intended
/// implementation wraps a `BluetoothGattServer`/`BluetoothGattCallback`
/// pair: `write_chunk` performs one characteristic write or notify,
/// `read_chunk`/`try_read_chunk` drain chunks the callback already
/// buffered from the peer. Chunk-level, not frame-level — [`RadioAdapter`]
/// is what turns an implementation of this trait into
/// `mini_bearer::BleRadio`, the trait `mini_bearer::AndroidBleBearer`
/// actually drives.
pub trait BleRadio: Send + Sync {
    /// Send one already-chunked piece of a frame over the radio.
    fn write_chunk(&self, chunk: Vec<u8>) -> Result<(), BleRadioError>;
    /// Block until the next chunk arrives from the peer.
    fn read_chunk(&self) -> Result<Vec<u8>, BleRadioError>;
    /// Return the next already-buffered chunk, or `Ok(None)` if none is
    /// pending yet. Must never block.
    fn try_read_chunk(&self) -> Result<Option<Vec<u8>>, BleRadioError>;
}

/// Failure reported by a caller-implemented [`BleRadio`]. Carries no
/// message across the FFI boundary, matching
/// [`crate::StorageCipherError`]'s reasoning: a disconnected GATT link, a
/// characteristic write timeout, and a rejected MTU negotiation are all
/// platform-specific detail this crate has no use for beyond "did this
/// succeed."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BleRadioError {
    /// The radio operation failed.
    Failed,
}

impl core::fmt::Display for BleRadioError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ble radio operation failed")
    }
}

impl std::error::Error for BleRadioError {}

/// Adapts a caller-supplied [`BleRadio`] into `mini_bearer::BleRadio`,
/// whose `&mut self` shape is what `mini_bearer::AndroidBleBearer` needs
/// to drive it — a UniFFI callback interface can only ever offer `&self`
/// methods (Kotlin owns no borrow checker), so this adapter is the entire
/// difference between the two.
struct RadioAdapter(Box<dyn BleRadio>);

impl mini_bearer::BleRadio for RadioAdapter {
    fn write_chunk(&mut self, chunk: &[u8]) -> mini_bearer::Result<()> {
        self.0
            .write_chunk(chunk.to_vec())
            .map_err(|_| mini_bearer::BearerError::Closed)
    }

    fn read_chunk(&mut self) -> mini_bearer::Result<Vec<u8>> {
        self.0
            .read_chunk()
            .map_err(|_| mini_bearer::BearerError::Closed)
    }

    fn try_read_chunk(&mut self) -> mini_bearer::Result<Option<Vec<u8>>> {
        self.0
            .try_read_chunk()
            .map_err(|_| mini_bearer::BearerError::Closed)
    }
}

/// UniFFI object wrapping `mini_bearer::AndroidBleBearer` (D-0374) so
/// Kotlin can drive a real BLE-backed [`mini_bearer::Bearer`] by
/// implementing only [`BleRadio`] — Rust owns all chunking, reassembly,
/// and framing; Kotlin owns only the actual characteristic I/O.
#[derive(Debug)]
pub struct BleBearerHandle {
    bearer: Mutex<mini_bearer::AndroidBleBearer<RadioAdapter>>,
}

impl BleBearerHandle {
    /// `mtu` is the usable bytes per chunk (header included) — the
    /// caller's already-negotiated GATT ATT MTU. Validated lazily: an MTU
    /// too small to fit even the chunk header surfaces as
    /// [`BleBearerError::MtuTooSmall`] on the first [`Self::send`], not
    /// here, matching `mini_bearer::AndroidBleBearer::new`'s own
    /// contract.
    pub fn new(radio: Box<dyn BleRadio>, mtu: u32) -> Self {
        let adapter = RadioAdapter(radio);
        BleBearerHandle {
            bearer: Mutex::new(mini_bearer::AndroidBleBearer::new(adapter, mtu as usize)),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, mini_bearer::AndroidBleBearer<RadioAdapter>> {
        self.bearer
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Chunk and send one frame.
    pub fn send(&self, frame: Vec<u8>) -> Result<(), BleBearerError> {
        self.lock()
            .send(&frame)
            .map_err(BleBearerError::from_bearer)
    }

    /// Block until the next full frame is reassembled from incoming chunks.
    pub fn recv(&self) -> Result<Vec<u8>, BleBearerError> {
        self.lock().recv().map_err(BleBearerError::from_bearer)
    }

    /// Return the next full frame if one is already reassembled, `None`
    /// otherwise. Never blocks.
    pub fn try_recv(&self) -> Result<Option<Vec<u8>>, BleBearerError> {
        self.lock().try_recv().map_err(BleBearerError::from_bearer)
    }
}

/// FFI-facing failure from [`BleBearerHandle`]. Distinguishes what a
/// caller can actually act on differently — retry after a radio failure,
/// shrink the frame, renegotiate the MTU — from the residual "something
/// about the chunk protocol didn't add up" bucket ([`Self::Protocol`]),
/// which also absorbs any `mini_bearer::BearerError` variant this module
/// does not itself produce today, since that enum is `#[non_exhaustive]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BleBearerError {
    /// The underlying [`BleRadio`] call failed.
    RadioFailed,
    /// The frame exceeds `mini_bearer`'s maximum bearer frame size.
    FrameTooLarge,
    /// The configured MTU cannot fit even the chunk header.
    MtuTooSmall,
    /// The frame needs more chunks than fit in a `u16` at this MTU.
    TooManyChunks,
    /// A chunk from the peer was truncated, malformed, arrived out of
    /// order, or the failure was some other `mini_bearer::BearerError`
    /// variant not produced by the bearer send/recv path.
    Protocol,
}

impl BleBearerError {
    fn from_bearer(err: mini_bearer::BearerError) -> Self {
        match err {
            mini_bearer::BearerError::Closed => Self::RadioFailed,
            mini_bearer::BearerError::FrameTooLarge { .. } => Self::FrameTooLarge,
            mini_bearer::BearerError::MtuTooSmall { .. } => Self::MtuTooSmall,
            mini_bearer::BearerError::TooManyChunks { .. } => Self::TooManyChunks,
            _ => Self::Protocol,
        }
    }
}

impl core::fmt::Display for BleBearerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::RadioFailed => "the underlying BLE radio operation failed",
            Self::FrameTooLarge => "frame exceeds the maximum bearer frame size",
            Self::MtuTooSmall => "the configured MTU cannot fit even the chunk header",
            Self::TooManyChunks => "the frame needs more chunks than fit at this MTU",
            Self::Protocol => {
                "a malformed, out-of-order, or otherwise invalid chunk protocol event occurred"
            }
        };
        f.write_str(message)
    }
}

impl std::error::Error for BleBearerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};

    struct MockRadio {
        tx: Sender<Vec<u8>>,
        rx: Mutex<Receiver<Vec<u8>>>,
    }

    impl BleRadio for MockRadio {
        fn write_chunk(&self, chunk: Vec<u8>) -> Result<(), BleRadioError> {
            self.tx.send(chunk).map_err(|_| BleRadioError::Failed)
        }
        fn read_chunk(&self) -> Result<Vec<u8>, BleRadioError> {
            self.rx
                .lock()
                .unwrap()
                .recv()
                .map_err(|_| BleRadioError::Failed)
        }
        fn try_read_chunk(&self) -> Result<Option<Vec<u8>>, BleRadioError> {
            match self.rx.lock().unwrap().try_recv() {
                Ok(v) => Ok(Some(v)),
                Err(TryRecvError::Empty) => Ok(None),
                Err(TryRecvError::Disconnected) => Err(BleRadioError::Failed),
            }
        }
    }

    fn pair_with_mtu(mtu: u32) -> (BleBearerHandle, BleBearerHandle) {
        let (tx_a, rx_a) = channel();
        let (tx_b, rx_b) = channel();
        let radio_a = MockRadio {
            tx: tx_a,
            rx: Mutex::new(rx_b),
        };
        let radio_b = MockRadio {
            tx: tx_b,
            rx: Mutex::new(rx_a),
        };
        (
            BleBearerHandle::new(Box::new(radio_a), mtu),
            BleBearerHandle::new(Box::new(radio_b), mtu),
        )
    }

    fn pair() -> (BleBearerHandle, BleBearerHandle) {
        pair_with_mtu(64)
    }

    #[test]
    fn a_frame_larger_than_one_chunk_round_trips_through_the_ffi_handle() {
        let (a, b) = pair();
        let frame: Vec<u8> = (0u32..500).map(|i| (i % 251) as u8).collect();
        a.send(frame.clone()).unwrap();
        assert_eq!(b.recv().unwrap(), frame);
    }

    #[test]
    fn try_recv_returns_none_with_nothing_pending_then_the_frame_once_sent() {
        let (a, b) = pair();
        assert_eq!(b.try_recv().unwrap(), None);
        a.send(b"eventually".to_vec()).unwrap();
        assert_eq!(b.try_recv().unwrap(), Some(b"eventually".to_vec()));
    }

    #[test]
    fn a_radio_failure_surfaces_as_radio_failed_not_a_panic() {
        let (a, b) = pair();
        drop(b);
        let err = a.send(b"anyone there?".to_vec()).unwrap_err();
        assert_eq!(err, BleBearerError::RadioFailed);
    }

    #[test]
    fn an_mtu_too_small_for_the_header_is_reported_distinctly() {
        let (a, _b) = pair_with_mtu(3);
        let err = a.send(b"data".to_vec()).unwrap_err();
        assert_eq!(err, BleBearerError::MtuTooSmall);
    }

    #[test]
    fn a_bleradio_error_maps_to_radio_failed_through_the_adapter() {
        struct AlwaysFailingRadio;
        impl BleRadio for AlwaysFailingRadio {
            fn write_chunk(&self, _chunk: Vec<u8>) -> Result<(), BleRadioError> {
                Err(BleRadioError::Failed)
            }
            fn read_chunk(&self) -> Result<Vec<u8>, BleRadioError> {
                Err(BleRadioError::Failed)
            }
            fn try_read_chunk(&self) -> Result<Option<Vec<u8>>, BleRadioError> {
                Err(BleRadioError::Failed)
            }
        }
        let handle = BleBearerHandle::new(Box::new(AlwaysFailingRadio), 64);
        assert_eq!(
            handle.send(b"x".to_vec()).unwrap_err(),
            BleBearerError::RadioFailed
        );
        assert_eq!(handle.recv().unwrap_err(), BleBearerError::RadioFailed);
        assert_eq!(handle.try_recv().unwrap_err(), BleBearerError::RadioFailed);
    }
}
