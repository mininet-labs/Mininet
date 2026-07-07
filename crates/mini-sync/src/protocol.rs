//! The MINI/SYNC1 pull protocol: strictly alternating, bounded, resumable by
//! idempotence.

use mini_bearer::{Bearer, Channel};
use mini_crypto::HashAlgorithm;
use mini_objects::{Object, ObjectId, ObjectType};
use mini_store::{Backend, Store};

use crate::ingest::{Ingest, IngestOutcome, KelCache};
use crate::message::{Msg, SYNC_AAD};
use crate::{Result, SyncError};

const WANT_BATCH: usize = 4096;
/// Hard per-pull budget on received object bytes (session DoS bound).
const PULL_BYTE_BUDGET: usize = 512 * 1024 * 1024;
const OBJECTS_BYTE_BUDGET: usize = 4 * 1024 * 1024;
/// Maximum KEL carriers processed in one pull (identity-cache DoS bound).
const PULL_KEL_CARRIER_BUDGET: usize = 512;
const MAX_WANT_ROUNDS: usize = 1024;

/// Which side of the encounter this peer is (decides who pulls first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRole {
    /// Pulls first, then serves the peer's pull.
    Initiator,
    /// Serves first, then pulls.
    Responder,
}

/// What one sync ingested (this peer's own pull).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IngestReport {
    /// Objects received over the wire.
    pub received: usize,
    /// Content objects verified and inserted.
    pub accepted: usize,
    /// KEL carriers whose embedded KEL was absorbed into the identity cache.
    /// Envelope-provable carriers are also inserted as objects; KEL-only carriers
    /// are absorbed for identity but kept transport-only (not indexed).
    pub carriers: usize,
    /// Rejected: author unknown even after carriers were absorbed.
    pub unknown_author: usize,
    /// Rejected: failed integrity, signature, provenance, or carrier check.
    pub invalid: usize,
    /// Dropped without decoding: the peer sent objects we never asked for.
    pub unsolicited: usize,
}

fn send(bearer: &mut dyn Bearer, chan: &mut Channel, msg: &Msg) -> Result<()> {
    let ct = chan.seal(&msg.encode(), SYNC_AAD)?;
    bearer.send(&ct)?;
    Ok(())
}

fn recv(bearer: &mut dyn Bearer, chan: &mut Channel) -> Result<Msg> {
    let ct = bearer.recv()?;
    let pt = chan.open(&ct, SYNC_AAD)?;
    Msg::decode(&pt)
}

fn sorted_ids<B: Backend>(store: &Store<B>) -> Result<Vec<String>> {
    let mut ids: Vec<String> = store
        .all_ids()?
        .into_iter()
        .map(|i| i.as_str().to_string())
        .collect();
    ids.sort();
    Ok(ids)
}

fn digest_of(ids: &[String]) -> [u8; 32] {
    let mut buf = Vec::new();
    for id in ids {
        buf.extend_from_slice(&(id.len() as u32).to_be_bytes());
        buf.extend_from_slice(id.as_bytes());
    }
    HashAlgorithm::Blake3.digest(&buf)
}

/// Bucket by the character after the multibase prefix (deterministic spread).
fn bucket_of(id: &str) -> u8 {
    id.as_bytes().get(1).copied().unwrap_or(0)
}

fn bucket_digests(ids: &[String]) -> Vec<(u8, [u8; 32])> {
    let mut buckets: Vec<(u8, Vec<String>)> = Vec::new();
    for id in ids {
        let b = bucket_of(id);
        match buckets.iter_mut().find(|(k, _)| *k == b) {
            Some((_, v)) => v.push(id.clone()),
            None => buckets.push((b, vec![id.clone()])),
        }
    }
    buckets.sort_by_key(|(k, _)| *k);
    buckets
        .into_iter()
        .map(|(k, v)| (k, digest_of(&v)))
        .collect()
}

/// Run one full bidirectional sync over an established channel. Both peers call
/// this with opposite [`SyncRole`]s; each returns the report of its **own**
/// pull.
pub fn sync_bidirectional<B: Backend>(
    bearer: &mut dyn Bearer,
    chan: &mut Channel,
    store: &mut Store<B>,
    cache: &mut KelCache,
    role: SyncRole,
) -> Result<IngestReport> {
    match role {
        SyncRole::Initiator => {
            let report = pull(bearer, chan, store, cache)?;
            serve_pull(bearer, chan, store)?;
            Ok(report)
        }
        SyncRole::Responder => {
            serve_pull(bearer, chan, store)?;
            pull(bearer, chan, store, cache)
        }
    }
}

/// Client side of one pull: fetch everything the server has that we lack,
/// through the verified-ingest pipeline.
fn pull<B: Backend>(
    bearer: &mut dyn Bearer,
    chan: &mut Channel,
    store: &mut Store<B>,
    cache: &mut KelCache,
) -> Result<IngestReport> {
    let mut report = IngestReport::default();
    let my_ids = sorted_ids(store)?;
    send(bearer, chan, &Msg::RootDigest(digest_of(&my_ids)))?;

    let server_buckets = match recv(bearer, chan)? {
        Msg::Done => return Ok(report), // sets already equal
        Msg::BucketDigests(b) => b,
        _ => return Err(SyncError::Protocol),
    };
    let mine = bucket_digests(&my_ids);
    let need: Vec<u8> = server_buckets
        .iter()
        .filter(|(k, d)| mine.iter().find(|(mk, _)| mk == k).map(|(_, md)| md) != Some(d))
        .map(|(k, _)| *k)
        .collect();
    send(bearer, chan, &Msg::NeedBuckets(need))?;

    let offered = match recv(bearer, chan)? {
        Msg::Ids(ids) => ids,
        _ => return Err(SyncError::Protocol),
    };
    // Everything offered that we lack, structurally validated.
    let mut wants: Vec<String> = Vec::new();
    for id in offered {
        let oid = ObjectId::parse(&id)?;
        if !store.contains(&oid)? {
            wants.push(id);
        }
    }

    // Fetch in batches; buffer bytes for two-pass ingest. Session budget: the
    // peer can cost us at most what we asked for, and never more than
    // PULL_BYTE_BUDGET bytes total.
    let mut received_bytes: Vec<Vec<u8>> = Vec::new();
    let mut budget = PULL_BYTE_BUDGET;
    let mut rounds = 0usize;
    let mut cursor = 0usize;
    loop {
        rounds += 1;
        if rounds > MAX_WANT_ROUNDS {
            return Err(SyncError::LimitExceeded);
        }
        let batch: Vec<String> = wants[cursor..wants.len().min(cursor + WANT_BATCH)].to_vec();
        cursor += batch.len();
        let last = batch.is_empty();
        send(bearer, chan, &Msg::Want(batch))?;
        if last {
            match recv(bearer, chan)? {
                Msg::Done => break,
                _ => return Err(SyncError::Protocol),
            }
        }
        // Objects batches for this want, terminated by an empty batch.
        loop {
            match recv(bearer, chan)? {
                Msg::Objects(objs) if objs.is_empty() => break,
                Msg::Objects(objs) => {
                    for o in objs {
                        budget = budget
                            .checked_sub(o.len())
                            .ok_or(SyncError::LimitExceeded)?;
                        received_bytes.push(o);
                    }
                }
                _ => return Err(SyncError::Protocol),
            }
        }
    }

    // Two-pass verified ingest: decode all, DROP anything we never asked for,
    // absorb carriers first, then verify and insert content.
    report.received = received_bytes.len();
    let mut decoded: Vec<Object> = Vec::new();
    for bytes in &received_bytes {
        match Object::from_bytes(bytes) {
            Ok(o) => {
                if wants.iter().any(|w| w == o.id().as_str()) {
                    decoded.push(o);
                } else {
                    report.unsolicited += 1;
                }
            }
            Err(_) => report.invalid += 1,
        }
    }
    let mut deferred: Vec<&Object> = Vec::new();
    let mut carriers_seen = 0usize;
    for obj in decoded
        .iter()
        .filter(|o| o.object_type == ObjectType::Custom(crate::KEL_CARRIER.to_string()))
        .chain(
            decoded
                .iter()
                .filter(|o| o.object_type != ObjectType::Custom(crate::KEL_CARRIER.to_string())),
        )
    {
        if obj.object_type == ObjectType::Custom(crate::KEL_CARRIER.to_string()) {
            carriers_seen += 1;
            if carriers_seen > PULL_KEL_CARRIER_BUDGET {
                report.invalid += 1;
                continue;
            }
        }
        match Ingest::check(cache, obj) {
            IngestOutcome::AcceptedCarrier => {
                store.insert(obj)?;
                report.carriers += 1;
            }
            IngestOutcome::AcceptedKelOnly => {
                // The self-certifying KEL is absorbed (identity is usable), but the
                // carrier envelope's authorship isn't provable yet — its signing
                // device may arrive later in this same batch. Don't index the
                // object now; defer one retry. If it stays unprovable it remains
                // transport-only, never polluting the authorship index.
                report.carriers += 1;
                deferred.push(obj);
            }
            IngestOutcome::Accepted => {
                if obj.object_type == ObjectType::HEAD {
                    store.apply_head(obj)?;
                } else {
                    store.insert(obj)?;
                }
                report.accepted += 1;
            }
            IngestOutcome::UnknownAuthor => report.unknown_author += 1,
            IngestOutcome::Invalid => report.invalid += 1,
        }
    }
    // Second pass: now that every carrier in the batch has been absorbed, a root
    // carrier whose signing device arrived in-band becomes envelope-provable and
    // may finally be indexed. Its KEL was already counted, so we don't re-count.
    // A carrier that is still only KEL-provable stays transport-only.
    for obj in deferred {
        if let IngestOutcome::AcceptedCarrier = Ingest::check(cache, obj) {
            store.insert(obj)?;
        }
    }
    Ok(report)
}

/// Server side of one pull: answer the peer's reconciliation and stream the
/// objects it asks for (only ids we actually hold).
pub fn serve_pull<B: Backend>(
    bearer: &mut dyn Bearer,
    chan: &mut Channel,
    store: &mut Store<B>,
) -> Result<()> {
    let my_ids = sorted_ids(store)?;
    let client_root = match recv(bearer, chan)? {
        Msg::RootDigest(d) => d,
        _ => return Err(SyncError::Protocol),
    };
    if client_root == digest_of(&my_ids) {
        send(bearer, chan, &Msg::Done)?;
        return Ok(());
    }
    send(bearer, chan, &Msg::BucketDigests(bucket_digests(&my_ids)))?;

    let need = match recv(bearer, chan)? {
        Msg::NeedBuckets(n) => n,
        _ => return Err(SyncError::Protocol),
    };
    let offered: Vec<String> = my_ids
        .iter()
        .filter(|id| need.contains(&bucket_of(id)))
        .cloned()
        .collect();
    send(bearer, chan, &Msg::Ids(offered))?;

    let mut rounds = 0usize;
    loop {
        rounds += 1;
        if rounds > MAX_WANT_ROUNDS {
            return Err(SyncError::LimitExceeded);
        }
        let want = match recv(bearer, chan)? {
            Msg::Want(w) => w,
            _ => return Err(SyncError::Protocol),
        };
        if want.is_empty() {
            send(bearer, chan, &Msg::Done)?;
            return Ok(());
        }
        let mut batch: Vec<Vec<u8>> = Vec::new();
        let mut batch_bytes = 0usize;
        for id in &want {
            let oid = ObjectId::parse(id)?;
            let obj = match store.get(&oid) {
                Ok(o) => o,
                Err(_) => continue, // we never claimed it after all
            };
            let bytes = obj.to_bytes();
            if !batch.is_empty() && batch_bytes + bytes.len() > OBJECTS_BYTE_BUDGET {
                send(bearer, chan, &Msg::Objects(std::mem::take(&mut batch)))?;
                batch_bytes = 0;
            }
            batch_bytes += bytes.len();
            batch.push(bytes);
            if batch.len() == 64 {
                send(bearer, chan, &Msg::Objects(std::mem::take(&mut batch)))?;
                batch_bytes = 0;
            }
        }
        if !batch.is_empty() {
            send(bearer, chan, &Msg::Objects(batch))?;
        }
        send(bearer, chan, &Msg::Objects(Vec::new()))?; // end-of-want marker
    }
}
