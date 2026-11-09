//! Detecting a restored or forked FROST signer (research PR #429, T19 / D-0552).
//!
//! [`crate::signing::DurableCustodySigner`] makes nonce reuse impossible across a
//! crash, but a journal in a local directory cannot tell that the **whole disk or
//! VM was restored** to an older snapshot: the older directory looks perfectly
//! consistent, and a snapshot that includes memory can even still hold a secret
//! nonce that was already spent in the timeline that continued. Signing twice
//! with one nonce leaks the share.
//!
//! The defense needs one fact the signer's own storage cannot supply: a counter
//! that only moves forward and lives *outside* the thing that gets restored. This
//! module defines that boundary ([`MonotonicCounter`]) and an [`AnchoredSigner`]
//! that uses it:
//!
//! - every `commit` and every `sign` first does a **compare-and-set** on the
//!   external counter, from the value this signer last saw to that value plus
//!   one, *before* any commitment or signature share is released;
//! - if the counter is not the value this signer expects, another timeline has
//!   used the key since (a restored copy, a forked VM, a second host), and the
//!   operation is refused and the signer is poisoned;
//! - on open, the counter may not be behind the local journal and may not be
//!   more than one operation ahead of it (one is the legal crash window: the
//!   counter advanced, the process died before the journal write).
//!
//! What this is **not**: the counter implementations here ([`MemoryCounter`],
//! [`FileCounter`]) are only as external as the medium they sit on. A file next
//! to the journal is restored with it and protects nothing; it exists for tests
//! and for hosts that point it at separate storage. A real deployment must back
//! [`MonotonicCounter`] with something that is not rolled back together with the
//! signer: an HSM or TPM monotonic counter, or a remote service the other
//! signers trust. That backing is a host decision and is not built or audited
//! here. The one-operation crash window is accepted and documented, not hidden.

use std::path::{Path, PathBuf};

use frost_ristretto255::keys::KeyPackage;
use frost_ristretto255::round1::SigningCommitments;
use frost_ristretto255::round2::SignatureShare;
use frost_ristretto255::SigningPackage;
use rand_core::{CryptoRng, RngCore};

use crate::error::{CustodyError, Result};
use crate::signing::{DurableCustodySigner, DurableSigningNonces};

fn anchor_error(msg: impl std::fmt::Display) -> CustodyError {
    CustodyError::SigningJournal(format!("anchor: {msg}"))
}

/// A counter that only moves forward and survives restoration of the signer.
pub trait MonotonicCounter {
    /// The current value.
    fn current(&self) -> Result<u64>;
    /// Atomically set the counter to `expected + 1` **only if** it currently
    /// equals `expected`. Any other current value must be an error, and the
    /// counter must never decrease.
    fn compare_and_advance(&mut self, expected: u64) -> Result<()>;
}

/// An in-process counter for tests and for hosts that share one across signers.
#[derive(Debug, Clone, Default)]
pub struct MemoryCounter {
    value: std::sync::Arc<std::sync::Mutex<u64>>,
}

impl MemoryCounter {
    /// A counter starting at zero.
    pub fn new() -> Self {
        Self::default()
    }
}

impl MonotonicCounter for MemoryCounter {
    fn current(&self) -> Result<u64> {
        Ok(*self.value.lock().map_err(|_| anchor_error("poisoned"))?)
    }
    fn compare_and_advance(&mut self, expected: u64) -> Result<()> {
        let mut value = self.value.lock().map_err(|_| anchor_error("poisoned"))?;
        if *value != expected {
            return Err(anchor_error(
                "counter moved: another timeline used this key",
            ));
        }
        *value += 1;
        Ok(())
    }
}

/// A counter in a file. **Only as external as the file's medium**: put it on
/// storage that is not restored together with the signer's directory, or it
/// protects nothing.
#[derive(Debug)]
pub struct FileCounter {
    path: PathBuf,
}

impl FileCounter {
    /// Use `path`; a missing file reads as zero.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn lock_path(&self) -> PathBuf {
        let mut name = self
            .path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_default();
        name.push(".lock");
        self.path.with_file_name(name)
    }

    fn read(path: &Path) -> Result<u64> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let arr: [u8; 8] = bytes
                    .try_into()
                    .map_err(|_| anchor_error("counter file is not eight bytes"))?;
                Ok(u64::from_be_bytes(arr))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
            Err(e) => Err(anchor_error(e)),
        }
    }
}

impl MonotonicCounter for FileCounter {
    fn current(&self) -> Result<u64> {
        Self::read(&self.path)
    }
    fn compare_and_advance(&mut self, expected: u64) -> Result<()> {
        let _lock = mini_durable::lock_exclusive(&self.lock_path()).map_err(anchor_error)?;
        let value = Self::read(&self.path)?;
        if value != expected {
            return Err(anchor_error(
                "counter moved: another timeline used this key",
            ));
        }
        let next = expected
            .checked_add(1)
            .ok_or_else(|| anchor_error("counter exhausted"))?;
        mini_durable::atomic_replace(&self.path, &next.to_be_bytes()).map_err(anchor_error)
    }
}

/// A [`DurableCustodySigner`] whose every operation is anchored to a
/// [`MonotonicCounter`].
#[derive(Debug)]
pub struct AnchoredSigner<C: MonotonicCounter> {
    inner: DurableCustodySigner,
    counter: C,
    expected: u64,
    poisoned: bool,
}

impl<C: MonotonicCounter> AnchoredSigner<C> {
    /// Open the journal and check it against the counter. Refuses when the
    /// counter is behind the journal (the counter was reset) or more than one
    /// operation ahead of it (the journal was restored to an older state).
    pub fn open(directory: impl Into<PathBuf>, key: KeyPackage, counter: C) -> Result<Self> {
        let inner = DurableCustodySigner::open(directory, key)?;
        let local = inner.operation_count();
        let external = counter.current()?;
        if external < local {
            return Err(anchor_error(
                "external counter is behind the journal; the counter was reset",
            ));
        }
        if external > local + 1 {
            return Err(anchor_error(
                "journal is behind the external counter; the signer directory was restored or forked",
            ));
        }
        Ok(Self {
            inner,
            counter,
            expected: external,
            poisoned: false,
        })
    }

    fn step(&mut self) -> Result<()> {
        if self.poisoned {
            return Err(anchor_error("signer is poisoned; reopen and investigate"));
        }
        if let Err(error) = self.counter.compare_and_advance(self.expected) {
            self.poisoned = true;
            return Err(error);
        }
        self.expected += 1;
        Ok(())
    }

    /// Round 1, anchored: the counter advances before the commitment is released.
    pub fn commit<R>(&mut self, rng: &mut R) -> Result<(DurableSigningNonces, SigningCommitments)>
    where
        R: RngCore + CryptoRng,
    {
        self.step()?;
        // The counter has moved; if the journal then fails, stop rather than
        // let the two drift further apart.
        self.inner.commit(rng).inspect_err(|_| self.poisoned = true)
    }

    /// Round 2, anchored: the counter advances before the share is computed.
    pub fn sign(
        &mut self,
        nonces: DurableSigningNonces,
        package: &SigningPackage,
    ) -> Result<SignatureShare> {
        self.step()?;
        self.inner
            .sign(nonces, package)
            .inspect_err(|_| self.poisoned = true)
    }

    /// Operations the journal has recorded.
    pub fn operation_count(&self) -> u64 {
        self.inner.operation_count()
    }
}
