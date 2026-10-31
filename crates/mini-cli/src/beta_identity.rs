//! Per-account BETA signing identities for the CLI.
//!
//! BETA objects are public and replicate to anonymous peers, so signing them
//! with the CLI's primary identity would link every account and transfer to
//! the user's normal DID. Each account instead gets its own root + device,
//! derived with HKDF from the primary root seed and a random per-registration
//! nonce. Nothing derived is stored except the nonce, so the author DID is
//! unlinkable without the seed, and a fresh random nonce means two homes
//! restored from one seed never share an identity, account id or author
//! sequence. The owned-account list is local state: `home/beta-accounts/<nonce>`
//! markers that hold the epoch.

use std::fs;
use std::path::{Path, PathBuf};

use did_mini::{Capabilities, Controller, Did};
use mini_beta::{BetaAccountId, BetaEpochId};
use mini_beta_exec::derive_account_id;
use mini_crypto::KdfSuite;
use mini_objects::Object;
use mini_store::{FsBackend, Store};

use crate::beta::hex;
use crate::error::{CliError, Result};

const INFO: &[u8] = b"mininet/beta/account-identity/v1";
const MAX_MARKERS: usize = 256;

/// One BETA account's isolated identity.
pub(crate) struct AccountIdentity {
    pub human: Controller,
    pub device: Controller,
    pub nonce: [u8; 32],
    pub account: BetaAccountId,
}

impl AccountIdentity {
    pub fn owner(&self) -> Did {
        self.human.did()
    }
}

fn dir(home: &Path) -> PathBuf {
    home.join("beta-accounts")
}

fn identity_err(message: impl Into<String>) -> CliError {
    CliError::Identity(message.into())
}

pub(crate) fn derive(home: &Path, epoch: BetaEpochId, nonce: [u8; 32]) -> Result<AccountIdentity> {
    let seed = crate::identity::human_root_seed(home)?;
    let mut info = Vec::with_capacity(INFO.len() + 64);
    info.extend_from_slice(INFO);
    info.extend_from_slice(epoch.as_bytes());
    info.extend_from_slice(&nonce);
    let mut okm = KdfSuite::HkdfSha256
        .derive_bytes(None, seed.as_ref(), &info, 128)
        .map_err(|_| identity_err("BETA identity derivation failed"))?;
    let part = |at: usize| -> [u8; 32] { okm[at..at + 32].try_into().expect("128 bytes") };
    let (root_current, root_next) = (part(0), part(32));
    let (device_current, device_next) = (part(64), part(96));
    okm.fill(0);
    let mut human = Controller::incept_single_from_seeds(&root_current, &root_next)
        .map_err(|e| identity_err(e.to_string()))?;
    let device =
        Controller::incept_device_single_from_seeds(&human.did(), &device_current, &device_next)
            .map_err(|e| identity_err(e.to_string()))?;
    human
        .delegate_device(&device.did(), Capabilities::primary())
        .map_err(|e| identity_err(e.to_string()))?;
    let account = derive_account_id(epoch, &human.did(), nonce)
        .map_err(|e| CliError::Object(format!("beta wallet: {e}")))?;
    Ok(AccountIdentity {
        human,
        device,
        nonce,
        account,
    })
}

/// Exclusive lock for allocating a derived author's next sequence and
/// inserting the object, so concurrent invocations can never equivocate.
pub(crate) fn lock(home: &Path) -> Result<fs::File> {
    fs::create_dir_all(dir(home)).map_err(|e| CliError::Io(e.to_string()))?;
    mini_durable::lock_exclusive(&dir(home).join(".lock")).map_err(|e| CliError::Io(e.to_string()))
}

/// Record a nonce as owned by this home in `epoch` (before the registration is
/// inserted, so a crash never loses ownership of a registered account).
pub(crate) fn record(home: &Path, epoch: BetaEpochId, nonce: &[u8; 32]) -> Result<()> {
    fs::create_dir_all(dir(home)).map_err(|e| CliError::Io(e.to_string()))?;
    mini_durable::atomic_replace(&dir(home).join(hex(nonce)), epoch.as_bytes())
        .map_err(|e| CliError::Io(e.to_string()))
}

/// Nonces this home registered in `epoch`.
pub(crate) fn owned_nonces(home: &Path, epoch: BetaEpochId) -> Result<Vec<[u8; 32]>> {
    let entries = match fs::read_dir(dir(home)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(CliError::Io(error.to_string())),
    };
    let mut nonces = Vec::new();
    for (seen, entry) in entries.enumerate() {
        if seen >= MAX_MARKERS {
            return Err(CliError::Io("too many BETA account markers".into()));
        }
        let entry = entry.map_err(|e| CliError::Io(e.to_string()))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with('.') || name.ends_with(".tmp") {
            continue;
        }
        if fs::read(entry.path()).map_err(|e| CliError::Io(e.to_string()))? != epoch.as_bytes() {
            continue;
        }
        if name.len() != 64 {
            continue;
        }
        let mut nonce = [0u8; 32];
        for (i, byte) in nonce.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&name[i * 2..i * 2 + 2], 16)
                .map_err(|_| CliError::Io("corrupt BETA account marker".into()))?;
        }
        nonces.push(nonce);
    }
    nonces.sort();
    Ok(nonces)
}

/// All of this home's account identities in `epoch`.
pub(crate) fn owned(home: &Path, epoch: BetaEpochId) -> Result<Vec<AccountIdentity>> {
    owned_nonces(home, epoch)?
        .into_iter()
        .map(|nonce| derive(home, epoch, nonce))
        .collect()
}

/// Publish the identity's KEL carriers so peers can verify its objects.
/// Deterministic and idempotent.
pub(crate) fn publish_kels(identity: &AccountIdentity, store: &mut Store<FsBackend>) -> Result<()> {
    let owner = identity.owner();
    for kel in [identity.human.kel(), identity.device.kel()] {
        let carrier = mini_sync::kel_carrier(&kel, &owner, &identity.device)
            .map_err(|e| CliError::Object(e.to_string()))?;
        store
            .insert(&carrier)
            .map_err(|e| CliError::Store(e.to_string()))?;
    }
    Ok(())
}

/// Next author sequence for a derived identity: one above the highest signed
/// object it has in `store`. Call under [`lock`].
pub(crate) fn next_sequence(store: &Store<FsBackend>, author: &Did) -> Result<u64> {
    let mut floor = 0u64;
    for id in store
        .by_author(author)
        .map_err(|e| CliError::Store(e.to_string()))?
    {
        let object: Object = store.get(&id).map_err(|e| CliError::Store(e.to_string()))?;
        floor = floor.max(object.sequence);
    }
    floor
        .checked_add(1)
        .ok_or_else(|| CliError::Store("BETA author sequence exhausted".into()))
}
