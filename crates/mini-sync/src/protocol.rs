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
const PAGED_MAX_IDS: usize = 65_536;
const PAGED_MAX_ID_BYTES: usize = 8 * 1024 * 1024;
/// Hard ceiling on inventory IDs one paged session will transmit. Already-held
/// IDs are exempt from the receiver's work window, so without this a nearly
/// synchronized huge store could retransmit its whole inventory every session
/// and hit the session deadline.
const PAGED_MAX_OFFER_IDS: usize = 16 * PAGED_MAX_IDS;
const PAGED_PULL_BYTES: usize = 64 * 1024 * 1024;
/// A targeted release retrieval is one bounded exchange, not a general
/// replication session. The selector in `mini-forge` uses the same ceiling.
const MAX_RETRIEVAL_OBJECTS: usize = 4096;
const RETRIEVAL_BYTE_BUDGET: usize = PULL_BYTE_BUDGET;

/// Which side of the encounter this peer is (decides who pulls first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRole {
    /// Pulls first, then serves the peer's pull.
    Initiator,
    /// Serves first, then pulls.
    Responder,
}

/// What one sync ingested (this peer's own pull).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestReport {
    /// False when a bounded window leaves work for a subsequent sync.
    pub complete: bool,
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

impl Default for IngestReport {
    fn default() -> Self {
        Self {
            complete: true,
            received: 0,
            accepted: 0,
            carriers: 0,
            unknown_author: 0,
            invalid: 0,
            unsolicited: 0,
        }
    }
}

/// Result of one exact retrieval request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetrievalReport {
    /// Number of seed ids requested by the client.
    pub requested: usize,
    /// Number of object ids selected by the serving peer.
    pub selected: usize,
    /// Verified-ingest counters for the received objects.
    pub ingest: IngestReport,
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
    sync_mode(bearer, chan, store, cache, role, false, PULL_BYTE_BUDGET)
}

/// Versioned paged reconciliation for mutually authenticated forge sessions.
/// Both peers must explicitly select this API; legacy MINI/SYNC1 is unchanged.
/// Inventories use bounded flow-controlled pages. Only missing IDs consume
/// the 65536-ID / 8MiB work window, so later sessions advance past known IDs.
/// At most 64MiB of object bytes transfer per pull. WindowDone persists a
/// verified subset with complete=false; subsequent syncs resume missing IDs.
/// An individual object larger than the byte window returns an error.
pub fn sync_bidirectional_paged<B: Backend>(
    bearer: &mut dyn Bearer,
    chan: &mut Channel,
    store: &mut Store<B>,
    cache: &mut KelCache,
    role: SyncRole,
) -> Result<IngestReport> {
    sync_mode(bearer, chan, store, cache, role, true, PAGED_PULL_BYTES)
}

/// Bound a paged inventory offer to `cap` IDs.
///
/// KEL carriers always come first (identity must precede content windows).
/// When the rest does not fit, a window of `cap` IDs is taken starting at a
/// pseudo-random position (`seed`) and wrapping, so repeated sessions cover the
/// whole store without per-peer state; the second value reports truncation so
/// the session finishes as incomplete rather than silently dropping IDs.
///
/// Honest limit: coverage of a store larger than `cap` is probabilistic across
/// sessions, not a deterministic cursor.
fn bound_offer(
    offered: Vec<String>,
    carriers: &std::collections::BTreeSet<&str>,
    cap: usize,
    seed: u64,
) -> (Vec<String>, bool) {
    if offered.len() <= cap {
        return (offered, false);
    }
    let (mut bounded, rest): (Vec<String>, Vec<String>) = offered
        .into_iter()
        .partition(|id| carriers.contains(id.as_str()));
    bounded.truncate(cap);
    let room = cap - bounded.len();
    if room > 0 && !rest.is_empty() {
        let start = (seed % rest.len() as u64) as usize;
        bounded.extend(
            rest.iter()
                .cycle()
                .skip(start)
                .take(room.min(rest.len()))
                .cloned(),
        );
    }
    (bounded, true)
}

fn sync_mode<B: Backend>(
    bearer: &mut dyn Bearer,
    chan: &mut Channel,
    store: &mut Store<B>,
    cache: &mut KelCache,
    role: SyncRole,
    paged: bool,
    window_bytes: usize,
) -> Result<IngestReport> {
    match role {
        SyncRole::Initiator => {
            let mut report = pull(bearer, chan, store, cache, paged, window_bytes)?;
            report.complete &= serve_pull_mode(
                bearer,
                chan,
                store,
                paged,
                window_bytes,
                PAGED_MAX_OFFER_IDS,
            )?;
            Ok(report)
        }
        SyncRole::Responder => {
            let sent_complete = serve_pull_mode(
                bearer,
                chan,
                store,
                paged,
                window_bytes,
                PAGED_MAX_OFFER_IDS,
            )?;
            let mut report = pull(bearer, chan, store, cache, paged, window_bytes)?;
            report.complete &= sent_complete;
            Ok(report)
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
    paged: bool,
    window_bytes: usize,
) -> Result<IngestReport> {
    let report = IngestReport::default();
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

    let mut inventory_partial = false;
    let offered = if paged {
        let mut ids = Vec::new();
        let mut unique = std::collections::BTreeSet::new();
        let mut id_bytes = 0usize;
        loop {
            match recv(bearer, chan)? {
                Msg::Done => break,
                Msg::Ids(page) if !page.is_empty() => {
                    for id in page {
                        let oid = ObjectId::parse(&id)?;
                        // Keep only missing IDs. Already-held inventory never
                        // consumes the next session's bounded work window.
                        if !store.contains(&oid)? {
                            id_bytes = id_bytes
                                .checked_add(id.len())
                                .ok_or(SyncError::LimitExceeded)?;
                            if ids.len() < PAGED_MAX_IDS && id_bytes <= PAGED_MAX_ID_BYTES {
                                if !unique.insert(id.clone()) {
                                    return Err(SyncError::Protocol);
                                }
                                ids.push(id);
                            } else {
                                inventory_partial = true;
                            }
                        }
                    }
                    let more = ids.len() < PAGED_MAX_IDS && id_bytes < PAGED_MAX_ID_BYTES;
                    send(bearer, chan, &Msg::InventoryMore(more))?;
                }
                Msg::WindowDone => {
                    inventory_partial = true;
                    break;
                }
                _ => return Err(SyncError::Protocol),
            }
        }
        ids
    } else {
        match recv(bearer, chan)? {
            Msg::Ids(ids) => ids,
            _ => return Err(SyncError::Protocol),
        }
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
    let mut budget = window_bytes;
    let mut partial = false;
    let mut received_ids = std::collections::BTreeSet::new();
    let wanted_ids: std::collections::BTreeSet<&str> = wants.iter().map(String::as_str).collect();
    let mut rounds = 0usize;
    let mut cursor = 0usize;
    'wants: loop {
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
                Msg::WindowDone if paged => {
                    partial = true;
                    break 'wants;
                }
                Msg::Objects(objs) if objs.is_empty() => break,
                Msg::Objects(objs) => {
                    for o in objs {
                        if paged {
                            if received_ids.len() >= PAGED_MAX_IDS {
                                return Err(SyncError::LimitExceeded);
                            }
                            let object = Object::from_bytes(&o)?;
                            let id = object.id().as_str().to_string();
                            if !wanted_ids.contains(id.as_str()) || !received_ids.insert(id) {
                                return Err(SyncError::Protocol);
                            }
                        }
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

    if paged && !partial && received_ids.len() != wants.len() {
        return Err(SyncError::RetrievalIncomplete);
    }
    let mut report = ingest_objects(store, cache, received_bytes, &wants)?;
    if paged {
        report.complete = !partial && !inventory_partial;
        for id in &wants {
            if !store.contains(&ObjectId::parse(id)?)? {
                report.complete = false;
            }
        }
    }
    Ok(report)
}

/// Decode and ingest one bounded set of received object bytes. This is shared
/// by full reconciliation and exact retrieval so both paths have the same
/// verify-before-insert behavior and the same carrier ordering.
fn ingest_objects<B: Backend>(
    store: &mut Store<B>,
    cache: &mut KelCache,
    received_bytes: Vec<Vec<u8>>,
    requested_ids: &[String],
) -> Result<IngestReport> {
    let mut report = IngestReport {
        received: received_bytes.len(),
        ..IngestReport::default()
    };

    // Two-pass verified ingest: decode all, DROP anything we never asked for,
    // absorb carriers first, then verify and insert content.
    let requested: std::collections::BTreeSet<&str> =
        requested_ids.iter().map(String::as_str).collect();
    let mut decoded: Vec<Object> = Vec::new();
    for bytes in &received_bytes {
        match Object::from_bytes(bytes) {
            Ok(o) => {
                if requested.contains(o.id().as_str()) {
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
    serve_pull_mode(
        bearer,
        chan,
        store,
        false,
        PULL_BYTE_BUDGET,
        PAGED_MAX_OFFER_IDS,
    )
    .map(|_| ())
}
fn serve_pull_mode<B: Backend>(
    bearer: &mut dyn Bearer,
    chan: &mut Channel,
    store: &mut Store<B>,
    paged: bool,
    window_bytes: usize,
    offer_cap: usize,
) -> Result<bool> {
    let my_ids = sorted_ids(store)?;
    let client_root = match recv(bearer, chan)? {
        Msg::RootDigest(d) => d,
        _ => return Err(SyncError::Protocol),
    };
    if client_root == digest_of(&my_ids) {
        send(bearer, chan, &Msg::Done)?;
        return Ok(true);
    }
    send(bearer, chan, &Msg::BucketDigests(bucket_digests(&my_ids)))?;

    let need = match recv(bearer, chan)? {
        Msg::NeedBuckets(n) => n,
        _ => return Err(SyncError::Protocol),
    };
    let mut offered: Vec<String> = my_ids
        .iter()
        .filter(|id| need.contains(&bucket_of(id)))
        .cloned()
        .collect();
    let mut inventory_complete = true;
    if paged {
        // Identity carriers must precede bounded content windows so valid
        // unknown-author history cannot indefinitely starve its own KELs.
        let carrier_ids = store.by_type(&ObjectType::Custom(crate::KEL_CARRIER.into()))?;
        let carriers: std::collections::BTreeSet<&str> =
            carrier_ids.iter().map(ObjectId::as_str).collect();
        offered.sort_by_key(|id| !carriers.contains(id.as_str()));
        let seed = mini_crypto::random_32()
            .map(|bytes| u64::from_be_bytes(bytes[..8].try_into().expect("8 bytes")))
            .unwrap_or(0);
        let (bounded, truncated) = bound_offer(offered, &carriers, offer_cap, seed);
        offered = bounded;
        inventory_complete = !truncated;
        for (index, page) in offered.chunks(WANT_BATCH).enumerate() {
            send(bearer, chan, &Msg::Ids(page.to_vec()))?;
            match recv(bearer, chan)? {
                Msg::InventoryMore(true) => {}
                Msg::InventoryMore(false) => {
                    inventory_complete &= (index + 1) * WANT_BATCH >= offered.len();
                    break;
                }
                _ => return Err(SyncError::Protocol),
            }
        }
        send(
            bearer,
            chan,
            &if inventory_complete {
                Msg::Done
            } else {
                Msg::WindowDone
            },
        )?;
    } else {
        send(bearer, chan, &Msg::Ids(offered.clone()))?;
    }
    let offered_set: std::collections::BTreeSet<&str> =
        offered.iter().map(String::as_str).collect();
    let mut sent_budget = window_bytes;
    let mut sent_objects = 0usize;
    let mut requested = std::collections::BTreeSet::new();

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
            return Ok(inventory_complete);
        }
        let mut batch: Vec<Vec<u8>> = Vec::new();
        let mut batch_bytes = 0usize;
        for id in &want {
            if paged && (!offered_set.contains(id.as_str()) || !requested.insert(id.clone())) {
                return Err(SyncError::Protocol);
            }
            let oid = ObjectId::parse(id)?;
            let obj = match store.get(&oid) {
                Ok(o) => o,
                Err(_) => continue, // we never claimed it after all
            };
            let bytes = obj.to_bytes();
            if paged {
                if bytes.len() > sent_budget {
                    if sent_objects == 0 {
                        return Err(SyncError::LimitExceeded);
                    }
                    if !batch.is_empty() {
                        send(bearer, chan, &Msg::Objects(batch))?;
                    }
                    send(bearer, chan, &Msg::WindowDone)?;
                    return Ok(false);
                }
                sent_budget -= bytes.len();
                sent_objects += 1;
            }
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

/// Client side of an exact retrieval exchange.
///
/// The client sends seed ids. The serving peer responds with a bounded,
/// deterministic selection (for a release, `mini-forge` supplies the
/// evidence-closure selector), and the client explicitly echoes that
/// selection before bytes are sent. Every byte then goes through the same
/// verified ingest pipeline as ordinary sync. This deliberately does not
/// merge branch heads or perform any owner/adoption action.
pub fn request_retrieval<B: Backend>(
    bearer: &mut dyn Bearer,
    chan: &mut Channel,
    store: &mut Store<B>,
    cache: &mut KelCache,
    seeds: &[ObjectId],
) -> Result<RetrievalReport> {
    if seeds.is_empty() || seeds.len() > MAX_RETRIEVAL_OBJECTS {
        return Err(SyncError::LimitExceeded);
    }
    let seed_ids: Vec<String> = seeds.iter().map(|id| id.as_str().to_string()).collect();
    if has_duplicate_strings(&seed_ids) {
        return Err(SyncError::Protocol);
    }
    send(bearer, chan, &Msg::Want(seed_ids.clone()))?;

    let selected = match recv(bearer, chan)? {
        Msg::Ids(ids) => ids,
        _ => return Err(SyncError::Protocol),
    };
    if selected.is_empty() || selected.len() > MAX_RETRIEVAL_OBJECTS {
        return Err(SyncError::RetrievalIncomplete);
    }
    for id in &selected {
        ObjectId::parse(id)?;
    }
    if has_duplicate_strings(&selected)
        || seed_ids
            .iter()
            .any(|seed| !selected.iter().any(|candidate| candidate == seed))
    {
        return Err(SyncError::RetrievalIncomplete);
    }

    // Echoing the exact server selection gives the responder an explicit
    // allow-list. The server never streams an unrequested object, even if a
    // selector bug or a malicious client tries to alter the exchange.
    send(bearer, chan, &Msg::Want(selected.clone()))?;

    let mut received_bytes = Vec::new();
    let mut budget = RETRIEVAL_BYTE_BUDGET;
    loop {
        match recv(bearer, chan)? {
            Msg::Objects(objects) if objects.is_empty() => break,
            Msg::Objects(objects) => {
                for object in objects {
                    budget = budget
                        .checked_sub(object.len())
                        .ok_or(SyncError::LimitExceeded)?;
                    received_bytes.push(object);
                }
            }
            _ => return Err(SyncError::Protocol),
        }
    }

    let ingest = ingest_objects(store, cache, received_bytes, &selected)?;
    Ok(RetrievalReport {
        requested: seeds.len(),
        selected: selected.len(),
        ingest,
    })
}

/// Receive the seed ids for a targeted retrieval. The serving application
/// uses these ids to choose a bounded closure before calling
/// [`serve_retrieval`]. Keeping selection outside this generic sync crate
/// avoids giving the wire layer authority over forge policy.
pub fn receive_retrieval_request(
    bearer: &mut dyn Bearer,
    chan: &mut Channel,
) -> Result<Vec<ObjectId>> {
    let ids = match recv(bearer, chan)? {
        Msg::Want(ids) => ids,
        _ => return Err(SyncError::Protocol),
    };
    if ids.is_empty() || ids.len() > MAX_RETRIEVAL_OBJECTS || has_duplicate_strings(&ids) {
        return Err(SyncError::Protocol);
    }
    ids.into_iter()
        .map(|id| ObjectId::parse(&id).map_err(Into::into))
        .collect()
}

/// Serve a previously selected exact retrieval closure.
///
/// `selected` must be the result of an application-level selector such as
/// `mini_forge::release_retrieval_ids`. The client must echo the exact id list;
/// object bytes are then streamed in bounded batches from the serving store.
pub fn serve_retrieval<B: Backend>(
    bearer: &mut dyn Bearer,
    chan: &mut Channel,
    store: &Store<B>,
    selected: &[ObjectId],
) -> Result<()> {
    if selected.is_empty() || selected.len() > MAX_RETRIEVAL_OBJECTS {
        return Err(SyncError::LimitExceeded);
    }
    let selected_ids: Vec<String> = selected.iter().map(|id| id.as_str().to_string()).collect();
    if has_duplicate_strings(&selected_ids) {
        return Err(SyncError::Protocol);
    }
    send(bearer, chan, &Msg::Ids(selected_ids.clone()))?;

    let wanted = match recv(bearer, chan)? {
        Msg::Want(ids) => ids,
        _ => return Err(SyncError::Protocol),
    };
    if wanted != selected_ids {
        return Err(SyncError::Protocol);
    }

    let mut batch = Vec::new();
    let mut batch_bytes = 0usize;
    let mut total_budget = RETRIEVAL_BYTE_BUDGET;
    for id in &wanted {
        let object_id = ObjectId::parse(id)?;
        let object = store.get(&object_id)?;
        let bytes = object.to_bytes();
        total_budget = total_budget
            .checked_sub(bytes.len())
            .ok_or(SyncError::LimitExceeded)?;
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
    send(bearer, chan, &Msg::Objects(Vec::new()))
}

fn has_duplicate_strings(values: &[String]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[..index].iter().any(|previous| previous == value))
}

#[cfg(test)]
mod paged_tests {
    use super::*;
    use mini_bearer::{pair, Initiator, Responder};
    use mini_store::MemoryBackend;

    #[test]
    fn bound_offer_keeps_small_offers_untouched_and_carriers_first_when_truncating() {
        let ids: Vec<String> = (0..20).map(|n| format!("id{n:02}")).collect();
        let carriers: std::collections::BTreeSet<&str> = ["id03", "id07"].into_iter().collect();
        // Fits: unchanged, not truncated.
        let (same, truncated) = bound_offer(ids.clone(), &carriers, 20, 5);
        assert_eq!(same, ids);
        assert!(!truncated);
        // Does not fit: exactly `cap` IDs, carriers always included and first,
        // no duplicates, and the session is reported incomplete.
        let (bounded, truncated) = bound_offer(ids.clone(), &carriers, 8, 5);
        assert!(truncated);
        assert_eq!(bounded.len(), 8);
        assert_eq!(&bounded[..2], &["id03", "id07"]);
        let unique: std::collections::BTreeSet<_> = bounded.iter().collect();
        assert_eq!(unique.len(), 8);
    }

    #[test]
    fn bound_offer_rotates_so_repeated_sessions_cover_the_whole_store() {
        let ids: Vec<String> = (0..100).map(|n| format!("id{n:03}")).collect();
        let none = std::collections::BTreeSet::new();
        let mut seen = std::collections::BTreeSet::new();
        for seed in 0..10u64 {
            let (bounded, truncated) = bound_offer(ids.clone(), &none, 10, seed * 10);
            assert!(truncated && bounded.len() == 10);
            seen.extend(bounded);
        }
        assert_eq!(seen.len(), 100, "every ID is reachable across sessions");
        // Wrap-around near the end still yields a full window.
        let (wrapped, _) = bound_offer(ids.clone(), &none, 10, 95);
        assert_eq!(wrapped.len(), 10);
        assert!(wrapped.contains(&"id099".to_string()) && wrapped.contains(&"id004".to_string()));
    }

    #[test]
    fn paged_offer_rejects_duplicate_ids_across_pages() {
        let (mut client, mut server) = pair();
        let (init, hello) = Initiator::start().unwrap();
        let (mut server_channel, response) = Responder::respond(&hello).unwrap();
        let mut client_channel = init.finish(&response).unwrap();
        let task = std::thread::spawn(move || {
            assert!(matches!(
                recv(&mut server, &mut server_channel).unwrap(),
                Msg::RootDigest(_)
            ));
            send(
                &mut server,
                &mut server_channel,
                &Msg::BucketDigests(vec![(1, [1; 32])]),
            )
            .unwrap();
            assert!(matches!(
                recv(&mut server, &mut server_channel).unwrap(),
                Msg::NeedBuckets(_)
            ));
            let hash = mini_crypto::Multihash::of(HashAlgorithm::Blake3, b"same-offer");
            let id =
                mini_crypto::encoding::encode(mini_crypto::encoding::BASE58BTC, &hash.to_bytes())
                    .unwrap();
            send(
                &mut server,
                &mut server_channel,
                &Msg::Ids(vec![id.clone()]),
            )
            .unwrap();
            assert!(matches!(
                recv(&mut server, &mut server_channel).unwrap(),
                Msg::InventoryMore(true)
            ));
            send(&mut server, &mut server_channel, &Msg::Ids(vec![id])).unwrap();
        });
        let result = pull(
            &mut client,
            &mut client_channel,
            &mut Store::new(MemoryBackend::new()),
            &mut KelCache::new(),
            true,
            PAGED_PULL_BYTES,
        );
        assert!(matches!(result, Err(SyncError::Protocol)));
        task.join().unwrap();
    }

    #[test]
    fn paged_inventory_skips_more_than_65536_already_present_ids() {
        use mini_store::Backend;
        let mut backend = MemoryBackend::new();
        let mut ids = Vec::new();
        for n in 0..(PAGED_MAX_IDS + 1) {
            let hash = mini_crypto::Multihash::of(HashAlgorithm::Blake3, &n.to_be_bytes());
            let id =
                mini_crypto::encoding::encode(mini_crypto::encoding::BASE58BTC, &hash.to_bytes())
                    .unwrap();
            // Inventory needs only membership; no bytes are requested/ingested.
            backend.put_blob(&id, b"already held").unwrap();
            ids.push(id);
        }
        let (mut client, mut server) = pair();
        let (init, hello) = Initiator::start().unwrap();
        let (mut server_channel, response) = Responder::respond(&hello).unwrap();
        let mut client_channel = init.finish(&response).unwrap();
        let task = std::thread::spawn(move || {
            assert!(matches!(
                recv(&mut server, &mut server_channel).unwrap(),
                Msg::RootDigest(_)
            ));
            send(
                &mut server,
                &mut server_channel,
                &Msg::BucketDigests(vec![(1, [1; 32])]),
            )
            .unwrap();
            assert!(matches!(
                recv(&mut server, &mut server_channel).unwrap(),
                Msg::NeedBuckets(_)
            ));
            for page in ids.chunks(WANT_BATCH) {
                send(&mut server, &mut server_channel, &Msg::Ids(page.to_vec())).unwrap();
                assert_eq!(
                    recv(&mut server, &mut server_channel).unwrap(),
                    Msg::InventoryMore(true)
                );
            }
            send(&mut server, &mut server_channel, &Msg::Done).unwrap();
            assert_eq!(
                recv(&mut server, &mut server_channel).unwrap(),
                Msg::Want(vec![])
            );
            send(&mut server, &mut server_channel, &Msg::Done).unwrap();
        });
        let report = pull(
            &mut client,
            &mut client_channel,
            &mut Store::new(backend),
            &mut KelCache::new(),
            true,
            PAGED_PULL_BYTES,
        )
        .unwrap();
        assert!(report.complete);
        assert_eq!(report.received, 0);
        task.join().unwrap();
    }

    #[test]
    fn paged_windows_persist_verified_progress_and_resume_to_completion() {
        use did_mini::{Capabilities, Controller};
        use mini_objects::{ObjectBuilder, Payload};
        let mut root = Controller::incept_single_from_seeds(&[11; 32], &[12; 32]).unwrap();
        let device =
            Controller::incept_device_single_from_seeds(&root.did(), &[13; 32], &[14; 32]).unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        let mut server_store = Store::new(MemoryBackend::new());
        let mut server_cache = KelCache::new();
        server_cache.insert_verified(root.kel());
        server_cache.insert_verified(device.kel());
        let carriers = [
            crate::kel_carrier(&root.kel(), &root.did(), &device).unwrap(),
            crate::kel_carrier(&device.kel(), &root.did(), &device).unwrap(),
        ];
        let carrier_bytes: usize = carriers.iter().map(|c| c.to_bytes().len()).sum();
        for carrier in &carriers {
            server_store.insert(carrier).unwrap();
        }
        let mut post_bytes = 0;
        for n in 0..2 {
            let post = ObjectBuilder::new(ObjectType::POST)
                .sequence(n)
                .payload(Payload::Public(vec![n as u8; 4096]))
                .sign(&root.did(), &device)
                .unwrap();
            post_bytes = post.to_bytes().len();
            server_store.insert(&post).unwrap();
        }
        let window = carrier_bytes + post_bytes;
        let mut client_store = Store::new(MemoryBackend::new());
        let mut client_cache = KelCache::new();
        let mut encounter = || {
            let (mut client, mut server) = pair();
            let (init, hello) = Initiator::start().unwrap();
            let (mut server_channel, response) = Responder::respond(&hello).unwrap();
            let mut client_channel = init.finish(&response).unwrap();
            std::thread::scope(|scope| {
                let server_store = &mut server_store;
                let server_cache = &mut server_cache;
                let task = scope.spawn(move || {
                    sync_mode(
                        &mut server,
                        &mut server_channel,
                        server_store,
                        server_cache,
                        SyncRole::Responder,
                        true,
                        window,
                    )
                    .unwrap()
                });
                let report = sync_mode(
                    &mut client,
                    &mut client_channel,
                    &mut client_store,
                    &mut client_cache,
                    SyncRole::Initiator,
                    true,
                    window,
                )
                .unwrap();
                (report, task.join().unwrap())
            })
        };
        let (first, first_server) = encounter();
        assert!(!first.complete);
        assert!(!first_server.complete);
        assert_eq!(first.accepted, 1);
        assert_eq!(first.carriers, 2);
        assert_eq!(first.unknown_author, 0);
        let (second, second_server) = encounter();
        assert!(second.complete);
        assert!(second_server.complete);
        assert_eq!(second.accepted, 1);
        let (third, _) = encounter();
        assert!(third.complete);
        assert_eq!(third.received, 0);
        assert_eq!(client_store.all_ids().unwrap().len(), 4);
    }
}
