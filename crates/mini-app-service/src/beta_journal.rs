//! Durable, idempotent record of BETA mutations keyed by operation id.
//!
//! A BETA registration or transfer is signed exactly once: the signed object
//! bytes are written here *before* they are committed to the store, so a retry
//! after a lost response (or a crash between signing and committing) replays
//! the identical object instead of signing a second one. Records never hold
//! keys; the stored input digest rejects reuse of an operation id for
//! different content.

use mini_app_protocol::{ErrorCode, ServiceError};
use mini_objects::Object;
use mini_store::{FsBackend, Store};
use std::fs;
use std::path::{Path, PathBuf};

use crate::{hex_encode, storage_error, MAX_SIGNED_OBJECT_BYTES};

const MAGIC: &[u8; 8] = b"MBETAOP1";
const HEADER_LEN: usize = 8 + 1 + 1 + 32 + 32 + 8 + 8 + 4;
const MAX_PENDING_RECORDS: usize = 1024;

pub(crate) const KIND_REGISTER: u8 = 1;
pub(crate) const KIND_TRANSFER: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Record {
    pub kind: u8,
    pub complete: bool,
    pub digest: [u8; 32],
    pub account: [u8; 32],
    pub amount: u64,
    pub change: u64,
    pub object: Vec<u8>,
}

fn io(error: impl core::fmt::Display) -> ServiceError {
    ServiceError::new(ErrorCode::Io, error.to_string())
}

fn corrupt() -> ServiceError {
    ServiceError::new(ErrorCode::Storage, "BETA operation journal is corrupt")
}

pub(crate) fn open(dir: &Path) -> Result<(), ServiceError> {
    mini_durable::create_dir_all(dir).map_err(io)
}

fn path(dir: &Path, operation_id: &str) -> PathBuf {
    dir.join(format!("{}.beta", hex_encode(operation_id.as_bytes())))
}

fn encode(record: &Record) -> Result<Vec<u8>, ServiceError> {
    if record.object.len() > MAX_SIGNED_OBJECT_BYTES {
        return Err(corrupt());
    }
    let mut out = Vec::with_capacity(HEADER_LEN + record.object.len());
    out.extend_from_slice(MAGIC);
    out.push(record.kind);
    out.push(u8::from(record.complete));
    out.extend_from_slice(&record.digest);
    out.extend_from_slice(&record.account);
    out.extend_from_slice(&record.amount.to_be_bytes());
    out.extend_from_slice(&record.change.to_be_bytes());
    out.extend_from_slice(&(record.object.len() as u32).to_be_bytes());
    out.extend_from_slice(&record.object);
    Ok(out)
}

fn decode(bytes: &[u8]) -> Result<Record, ServiceError> {
    if bytes.len() < HEADER_LEN || &bytes[..8] != MAGIC {
        return Err(corrupt());
    }
    let kind = bytes[8];
    let complete = match bytes[9] {
        0 => false,
        1 => true,
        _ => return Err(corrupt()),
    };
    if kind != KIND_REGISTER && kind != KIND_TRANSFER {
        return Err(corrupt());
    }
    let take32 = |at: usize| -> [u8; 32] { bytes[at..at + 32].try_into().expect("length checked") };
    let take64 = |at: usize| -> u64 {
        u64::from_be_bytes(bytes[at..at + 8].try_into().expect("length checked"))
    };
    let len = u32::from_be_bytes(
        bytes[HEADER_LEN - 4..HEADER_LEN]
            .try_into()
            .expect("checked"),
    ) as usize;
    if len > MAX_SIGNED_OBJECT_BYTES || bytes.len() != HEADER_LEN + len {
        return Err(corrupt());
    }
    Ok(Record {
        kind,
        complete,
        digest: take32(10),
        account: take32(42),
        amount: take64(74),
        change: take64(82),
        object: bytes[HEADER_LEN..].to_vec(),
    })
}

pub(crate) fn read(dir: &Path, operation_id: &str) -> Result<Option<Record>, ServiceError> {
    match fs::read(path(dir, operation_id)) {
        Ok(bytes) => decode(&bytes).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io(error)),
    }
}

pub(crate) fn write(dir: &Path, operation_id: &str, record: &Record) -> Result<(), ServiceError> {
    mini_durable::atomic_replace(&path(dir, operation_id), &encode(record)?).map_err(io)
}

/// Commit a record's exact signed bytes and mark it complete. Idempotent.
pub(crate) fn complete(
    dir: &Path,
    store: &mut Store<FsBackend>,
    operation_id: &str,
    record: &Record,
) -> Result<Record, ServiceError> {
    let object = Object::from_bytes(&record.object).map_err(storage_error)?;
    if object.to_bytes() != record.object {
        return Err(corrupt());
    }
    store.insert(&object).map_err(storage_error)?;
    let mut done = record.clone();
    done.complete = true;
    if !record.complete {
        write(dir, operation_id, &done)?;
    }
    Ok(done)
}

/// Commit every pending record (startup recovery and before choosing the next
/// account index, so a half-finished registration is never raced).
pub(crate) fn commit_pending(dir: &Path, store: &mut Store<FsBackend>) -> Result<(), ServiceError> {
    let mut seen = 0usize;
    for entry in fs::read_dir(dir).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(hex) = name.strip_suffix(".beta") else {
            continue;
        };
        let bytes = fs::read(entry.path()).map_err(io)?;
        let record = decode(&bytes)?;
        if record.complete {
            continue;
        }
        seen += 1;
        if seen > MAX_PENDING_RECORDS {
            return Err(ServiceError::new(
                ErrorCode::Storage,
                "too many pending BETA operations",
            ));
        }
        let operation_id = String::from_utf8(unhex(hex)?).map_err(|_| corrupt())?;
        complete(dir, store, &operation_id, &record)?;
    }
    Ok(())
}

fn unhex(value: &str) -> Result<Vec<u8>, ServiceError> {
    if value.len() % 2 != 0 {
        return Err(corrupt());
    }
    (0..value.len() / 2)
        .map(|i| u8::from_str_radix(&value[i * 2..i * 2 + 2], 16).map_err(|_| corrupt()))
        .collect()
}
