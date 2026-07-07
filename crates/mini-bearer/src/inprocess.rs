//! An in-process [`Bearer`] pair backed by channels.
//!
//! This is the deterministic test double for the transport: [`pair`] returns two
//! connected endpoints in the same process, so the channel handshake and higher
//! layers can be exercised in CI with no radios, no sockets, and no threads
//! required. Real BLE / Wi-Fi bearers implement the same [`Bearer`] trait.

use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};

use crate::bearer::{Bearer, MAX_FRAME_BYTES};
use crate::error::{BearerError, Result};

/// One endpoint of an in-process bearer pair.
#[derive(Debug)]
pub struct InProcessBearer {
    tx: Sender<Vec<u8>>,
    rx: Receiver<Vec<u8>>,
}

/// Create two connected in-process endpoints. A frame sent on one is received on
/// the other.
pub fn pair() -> (InProcessBearer, InProcessBearer) {
    let (tx_a, rx_a) = channel();
    let (tx_b, rx_b) = channel();
    (
        InProcessBearer { tx: tx_a, rx: rx_b },
        InProcessBearer { tx: tx_b, rx: rx_a },
    )
}

impl Bearer for InProcessBearer {
    fn send(&mut self, frame: &[u8]) -> Result<()> {
        if frame.len() > MAX_FRAME_BYTES {
            return Err(BearerError::FrameTooLarge {
                max: MAX_FRAME_BYTES,
                got: frame.len(),
            });
        }
        self.tx.send(frame.to_vec()).map_err(|_| BearerError::Closed)
    }

    fn recv(&mut self) -> Result<Vec<u8>> {
        self.rx.recv().map_err(|_| BearerError::Closed)
    }

    fn try_recv(&mut self) -> Result<Option<Vec<u8>>> {
        match self.rx.try_recv() {
            Ok(v) => Ok(Some(v)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(BearerError::Closed),
        }
    }
}
