//! Durable, idempotent record of BETA mutations keyed by operation id.
//!
//! A BETA registration, transfer or grant approval is signed exactly once: the signed object
//! bytes are written to `pending/` *before* they are committed to the store, so
//! a retry after a lost response (or a crash between signing and committing)
//! replays the identical object instead of signing a second one. Once
//! committed, the pending file is replaced by a compact receipt, so startup
//! recovery only ever scans genuinely pending work. Registrations also leave an
//! `accounts/<nonce>` marker (its content is the epoch), the local list of accounts this profile
//! owns. Records never hold keys; the stored input digest rejects reuse of an
//! operation id for different content.

use mini_app_protocol::{ErrorCode, ServiceError};
use mini_beta_exec::parse_account_registration_object;
use mini_objects::Object;
use mini_store::{FsBackend, Store};
use std::fs;
use std::path::{Path, PathBuf};

use crate::{hex_encode, storage_error, MAX_SIGNED_OBJECT_BYTES};

const PENDING_MAGIC: &[u8; 8] = b"MBETAOP2";
const RECEIPT_MAGIC: &[u8; 8] = b"MBETARC2";
const PENDING_HEADER: usize = 8 + 1 + 32 + 32 + 8 + 8 + 4;
const RECEIPT_FIXED: usize = 8 + 1 + 32 + 32 + 8 + 8 + 2;
const MAX_OBJECT_ID_LEN: usize = 256;
const MAX_PENDING_RECORDS: usize = 1024;
const MAX_ACCOUNT_MARKERS: usize = 256;

pub(crate) const KIND_REGISTER: u8 = 1;
pub(crate) const KIND_TRANSFER: u8 = 2;
/// A grant approval signed by the user's primary identity. Its `account`,
/// `amount` and `change` fields are unused (zero).
pub(crate) const KIND_APPROVAL: u8 = 3;

/// A signed object waiting to be committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Record {
    pub kind: u8,
    pub digest: [u8; 32],
    pub account: [u8; 32],
    pub amount: u64,
    pub change: u64,
    pub object: Vec<u8>,
}

/// The outcome of a committed operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Receipt {
    pub kind: u8,
    pub digest: [u8; 32],
    pub account: [u8; 32],
    pub amount: u64,
    pub change: u64,
    pub object_id: String,
}

pub(crate) enum Entry {
    Done(Receipt),
    Pending(Record),
}

fn io(error: impl core::fmt::Display) -> ServiceError {
    ServiceError::new(ErrorCode::Io, error.to_string())
}

fn corrupt() -> ServiceError {
    ServiceError::new(ErrorCode::Storage, "BETA operation journal is corrupt")
}

fn pending_dir(dir: &Path) -> PathBuf {
    dir.join("pending")
}
fn receipts_dir(dir: &Path) -> PathBuf {
    dir.join("receipts")
}
/// Flat on purpose: every extra 64-hex path component costs Windows' 260-char
/// path budget. Each marker is named by its nonce and holds its epoch.
fn accounts_dir(dir: &Path) -> PathBuf {
    dir.join("accounts")
}

pub(crate) fn open(dir: &Path) -> Result<(), ServiceError> {
    for sub in [pending_dir(dir), receipts_dir(dir), accounts_dir(dir)] {
        mini_durable::create_dir_all(&sub).map_err(io)?;
    }
    Ok(())
}

fn name(operation_id: &str) -> String {
    hex_encode(operation_id.as_bytes())
}

fn take32(bytes: &[u8], at: usize) -> [u8; 32] {
    bytes[at..at + 32].try_into().expect("length checked")
}
fn take64(bytes: &[u8], at: usize) -> u64 {
    u64::from_be_bytes(bytes[at..at + 8].try_into().expect("length checked"))
}

fn encode_pending(record: &Record) -> Result<Vec<u8>, ServiceError> {
    if record.object.len() > MAX_SIGNED_OBJECT_BYTES {
        return Err(corrupt());
    }
    let mut out = Vec::with_capacity(PENDING_HEADER + record.object.len());
    out.extend_from_slice(PENDING_MAGIC);
    out.push(record.kind);
    out.extend_from_slice(&record.digest);
    out.extend_from_slice(&record.account);
    out.extend_from_slice(&record.amount.to_be_bytes());
    out.extend_from_slice(&record.change.to_be_bytes());
    out.extend_from_slice(&(record.object.len() as u32).to_be_bytes());
    out.extend_from_slice(&record.object);
    Ok(out)
}

fn valid_kind(kind: u8) -> bool {
    kind == KIND_REGISTER || kind == KIND_TRANSFER || kind == KIND_APPROVAL
}

fn decode_pending(bytes: &[u8]) -> Result<Record, ServiceError> {
    if bytes.len() < PENDING_HEADER || &bytes[..8] != PENDING_MAGIC || !valid_kind(bytes[8]) {
        return Err(corrupt());
    }
    let len = u32::from_be_bytes(
        bytes[PENDING_HEADER - 4..PENDING_HEADER]
            .try_into()
            .expect("checked"),
    ) as usize;
    if len > MAX_SIGNED_OBJECT_BYTES || bytes.len() != PENDING_HEADER + len {
        return Err(corrupt());
    }
    Ok(Record {
        kind: bytes[8],
        digest: take32(bytes, 9),
        account: take32(bytes, 41),
        amount: take64(bytes, 73),
        change: take64(bytes, 81),
        object: bytes[PENDING_HEADER..].to_vec(),
    })
}

fn encode_receipt(receipt: &Receipt) -> Result<Vec<u8>, ServiceError> {
    if receipt.object_id.len() > MAX_OBJECT_ID_LEN {
        return Err(corrupt());
    }
    let mut out = Vec::with_capacity(RECEIPT_FIXED + receipt.object_id.len());
    out.extend_from_slice(RECEIPT_MAGIC);
    out.push(receipt.kind);
    out.extend_from_slice(&receipt.digest);
    out.extend_from_slice(&receipt.account);
    out.extend_from_slice(&receipt.amount.to_be_bytes());
    out.extend_from_slice(&receipt.change.to_be_bytes());
    out.extend_from_slice(&(receipt.object_id.len() as u16).to_be_bytes());
    out.extend_from_slice(receipt.object_id.as_bytes());
    Ok(out)
}

fn decode_receipt(bytes: &[u8]) -> Result<Receipt, ServiceError> {
    if bytes.len() < RECEIPT_FIXED || &bytes[..8] != RECEIPT_MAGIC || !valid_kind(bytes[8]) {
        return Err(corrupt());
    }
    let len = u16::from_be_bytes(
        bytes[RECEIPT_FIXED - 2..RECEIPT_FIXED]
            .try_into()
            .expect("checked"),
    ) as usize;
    if len > MAX_OBJECT_ID_LEN || bytes.len() != RECEIPT_FIXED + len {
        return Err(corrupt());
    }
    Ok(Receipt {
        kind: bytes[8],
        digest: take32(bytes, 9),
        account: take32(bytes, 41),
        amount: take64(bytes, 73),
        change: take64(bytes, 81),
        object_id: String::from_utf8(bytes[RECEIPT_FIXED..].to_vec()).map_err(|_| corrupt())?,
    })
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, ServiceError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io(error)),
    }
}

/// The receipt (done) or pending record for `operation_id`, if any.
pub(crate) fn lookup(dir: &Path, operation_id: &str) -> Result<Option<Entry>, ServiceError> {
    let file = name(operation_id);
    if let Some(bytes) = read_optional(&receipts_dir(dir).join(&file))? {
        return decode_receipt(&bytes).map(|r| Some(Entry::Done(r)));
    }
    if let Some(bytes) = read_optional(&pending_dir(dir).join(&file))? {
        return decode_pending(&bytes).map(|r| Some(Entry::Pending(r)));
    }
    Ok(None)
}

pub(crate) fn write_pending(
    dir: &Path,
    operation_id: &str,
    record: &Record,
) -> Result<(), ServiceError> {
    mini_durable::atomic_replace(
        &pending_dir(dir).join(name(operation_id)),
        &encode_pending(record)?,
    )
    .map_err(io)
}

/// Commit a record's exact signed bytes, write its receipt (and, for a
/// registration, the owned-account marker), then drop the pending file.
/// Idempotent: safe to repeat after a crash at any point.
pub(crate) fn complete(
    dir: &Path,
    store: &mut Store<FsBackend>,
    operation_id: &str,
    record: &Record,
) -> Result<Receipt, ServiceError> {
    let object = Object::from_bytes(&record.object).map_err(storage_error)?;
    if object.to_bytes() != record.object {
        return Err(corrupt());
    }
    store.insert(&object).map_err(storage_error)?;
    if record.kind == KIND_REGISTER {
        let registration = parse_account_registration_object(&object).map_err(storage_error)?;
        mini_durable::atomic_replace(
            &accounts_dir(dir).join(hex_encode(&registration.nonce)),
            registration.epoch.as_bytes(),
        )
        .map_err(io)?;
    }
    let receipt = Receipt {
        kind: record.kind,
        digest: record.digest,
        account: record.account,
        amount: record.amount,
        change: record.change,
        object_id: object.id().as_str().to_string(),
    };
    let file = name(operation_id);
    mini_durable::atomic_replace(&receipts_dir(dir).join(&file), &encode_receipt(&receipt)?)
        .map_err(io)?;
    match mini_durable::remove_file(&pending_dir(dir).join(&file)) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(error)),
    }
    Ok(receipt)
}

/// Commit every pending record (startup recovery and before each new
/// operation). Only the small `pending/` directory is scanned.
pub(crate) fn commit_pending(dir: &Path, store: &mut Store<FsBackend>) -> Result<(), ServiceError> {
    let mut seen = 0usize;
    for entry in fs::read_dir(pending_dir(dir)).map_err(io)? {
        let entry = entry.map_err(io)?;
        let file = entry.file_name();
        let Some(file) = file.to_str() else { continue };
        if file.ends_with(".tmp") {
            continue;
        }
        seen += 1;
        if seen > MAX_PENDING_RECORDS {
            return Err(ServiceError::new(
                ErrorCode::Storage,
                "too many pending BETA operations",
            ));
        }
        let record = decode_pending(&fs::read(entry.path()).map_err(io)?)?;
        let operation_id = String::from_utf8(unhex(file)?).map_err(|_| corrupt())?;
        complete(dir, store, &operation_id, &record)?;
    }
    Ok(())
}

/// Nonces of the accounts this profile registered in `epoch`.
pub(crate) fn owned_nonces(dir: &Path, epoch: &[u8; 32]) -> Result<Vec<[u8; 32]>, ServiceError> {
    let mut nonces = Vec::new();
    let mut seen = 0usize;
    for entry in fs::read_dir(accounts_dir(dir)).map_err(io)? {
        let entry = entry.map_err(io)?;
        let file = entry.file_name();
        let Some(file) = file.to_str() else { continue };
        if file.ends_with(".tmp") {
            continue;
        }
        seen += 1;
        if seen > MAX_ACCOUNT_MARKERS {
            return Err(ServiceError::new(
                ErrorCode::Storage,
                "too many BETA account markers",
            ));
        }
        if fs::read(entry.path()).map_err(io)? != epoch {
            continue;
        }
        let nonce: [u8; 32] = unhex(file)?.try_into().map_err(|_| corrupt())?;
        nonces.push(nonce);
    }
    nonces.sort();
    Ok(nonces)
}

fn unhex(value: &str) -> Result<Vec<u8>, ServiceError> {
    if value.len() % 2 != 0 {
        return Err(corrupt());
    }
    (0..value.len() / 2)
        .map(|i| u8::from_str_radix(&value[i * 2..i * 2 + 2], 16).map_err(|_| corrupt()))
        .collect()
}
