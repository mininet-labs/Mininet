//! Explicit-retention public seeding. No wallet keys, ledger writes or ranking inputs.
use crate::error::{CliError, Result};
use did_mini::{Did, IndexedSig};
use mini_bearer::{Bearer, Channel, Initiator, Responder, TcpBearer};
use mini_objects::{Object, ObjectId};
use mini_storage::{ReceiptFields, ServeReceipt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::{Duration, Instant};

mod collection;
mod mesh;

const AAD: &[u8] = b"MINI/SEED1";
const MAX_RECORD: usize = 128 * 1024;
const MAX_OBJECT: usize = 64 * 1024 * 1024;
const CHUNK: usize = 64 * 1024;
const MAX_RECORDS: usize = 4096;
fn err(e: impl std::fmt::Display) -> CliError {
    CliError::Usage(format!("seed: {e}"))
}
fn now() -> u64 {
    crate::sequence::now_ms()
}
fn pool(home: &Path) -> Result<mini_seed::Pool> {
    mini_seed::Pool::open(&home.join("seed-pool")).map_err(err)
}
fn take(args: &mut Vec<String>, key: &str) -> Result<Option<String>> {
    let Some(i) = args.iter().position(|s| s == key) else {
        return Ok(None);
    };
    if i + 1 >= args.len() || args[i + 1].starts_with("--") {
        return Err(err(format!("missing {key} value")));
    }
    args.remove(i);
    Ok(Some(args.remove(i)))
}
fn need(args: &mut Vec<String>, key: &str) -> Result<String> {
    take(args, key)?.ok_or_else(|| err(format!("{key} required")))
}
fn done(args: &[String]) -> Result<()> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(err("unexpected or duplicate argument"))
    }
}
fn number(s: &str) -> Result<u64> {
    s.parse().map_err(|_| err("invalid unsigned number"))
}
fn read_file(path: &Path, max: usize) -> Result<Vec<u8>> {
    let m = fs::symlink_metadata(path).map_err(err)?;
    if !m.is_file() || m.file_type().is_symlink() {
        return Err(err("expected regular non-symlink file"));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(err)?
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() > max {
        return Err(err("file exceeds limit"));
    }
    Ok(bytes)
}
fn safe_dir(path: &Path) -> Result<()> {
    let mut parent = Some(path);
    while let Some(p) = parent {
        if p.is_symlink() {
            return Err(err("state path cannot traverse symlinks"));
        }
        parent = p.parent();
    }
    mini_durable::create_dir_all(path).map_err(err)
}
fn send<T: Serialize>(b: &mut TcpBearer, c: &mut Channel, v: &T) -> Result<()> {
    let bytes = serde_json::to_vec(v).map_err(err)?;
    if bytes.len() > MAX_RECORD {
        return Err(err("protocol record exceeds limit"));
    }
    send_bytes(b, c, &bytes)
}
fn send_bytes(b: &mut TcpBearer, c: &mut Channel, bytes: &[u8]) -> Result<()> {
    let frame = c.seal(bytes, AAD).map_err(err)?;
    b.send(&frame).map_err(err)
}
fn receive_bytes(b: &mut TcpBearer, c: &mut Channel, max: usize) -> Result<Vec<u8>> {
    let frame = b.recv().map_err(err)?;
    if frame.len() > max + 128 {
        return Err(err("protocol frame exceeds limit"));
    }
    let bytes = c.open(&frame, AAD).map_err(err)?;
    if bytes.len() > max {
        return Err(err("protocol plaintext exceeds limit"));
    }
    Ok(bytes)
}
fn recv<T: serde::de::DeserializeOwned>(b: &mut TcpBearer, c: &mut Channel) -> Result<T> {
    serde_json::from_slice(&receive_bytes(b, c, MAX_RECORD)?).map_err(err)
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Price {
    micro_per_mib: u64,
    payee: Vec<u8>,
    network_id: [u8; 32],
}
fn price(home: &Path) -> Result<Price> {
    let p = home.join("seed-price.json");
    if !p.try_exists().map_err(err)? {
        return Ok(Price::default());
    }
    serde_json::from_slice(&read_file(&p, 4096)?).map_err(err)
}
fn cost(bytes: u64, p: &Price) -> Result<u64> {
    bytes
        .div_ceil(1024 * 1024)
        .checked_mul(p.micro_per_mib)
        .ok_or_else(|| err("price overflow"))
}
fn default_fetch_bytes() -> u64 {
    MAX_OBJECT as u64
}
fn default_fetch_window(bytes: &u64) -> bool {
    *bytes == MAX_OBJECT as u64
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
enum Request {
    Inventory,
    Advertisement,
    Catalog,
    Search {
        query: String,
        after: Option<String>,
        limit: u16,
    },
    Quote {
        id: String,
    },
    Fetch {
        id: String,
        witness_device: String,
        nonce: [u8; 32],
        max_micro: u64,
        payment_digest: Option<[u8; 32]>,
        #[serde(
            default = "default_fetch_bytes",
            skip_serializing_if = "default_fetch_window"
        )]
        max_bytes: u64,
    },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Offer {
    id: String,
    bytes: u64,
    expires_at_ms: u64,
    amount_micro: u64,
    price: Price,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sig {
    index: u32,
    suite: u8,
    bytes: Vec<u8>,
}
fn encode_sigs(s: &[IndexedSig]) -> Vec<Sig> {
    s.iter()
        .map(|s| Sig {
            index: s.index,
            suite: s.signature.suite().tag(),
            bytes: s.signature.to_bytes(),
        })
        .collect()
}
fn decode_sigs(s: &[Sig]) -> Result<Vec<IndexedSig>> {
    if s.is_empty() || s.len() > did_mini::MAX_SIGNATURES {
        return Err(err("invalid receipt signature count"));
    }
    s.iter()
        .map(|s| {
            Ok(IndexedSig {
                index: s.index,
                signature: mini_crypto::Signature::from_suite_bytes(
                    mini_crypto::SignatureSuite::from_tag(s.suite).map_err(err)?,
                    &s.bytes,
                )
                .map_err(err)?,
            })
        })
        .collect()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    id: String,
    content_id: String,
    bytes: u64,
    digest: [u8; 32],
    host_root: String,
    witness_root: String,
    host_device: String,
    witness_device: String,
    host_nonce: [u8; 32],
    witness_nonce: [u8; 32],
    at_ms: u64,
    host_sig: Vec<Sig>,
    witness_sig: Vec<Sig>,
    payment: Option<Vec<u8>>,
}
impl Evidence {
    fn fields(&self) -> Result<ReceiptFields> {
        Ok(ReceiptFields {
            version: mini_storage::RECEIPT_VERSION,
            content_id: ObjectId::parse(&self.content_id).map_err(err)?,
            bytes: self.bytes,
            content_digest: self.digest,
            host_device: Did::parse(&self.host_device).map_err(err)?,
            witness_device: Did::parse(&self.witness_device).map_err(err)?,
            host_nonce: self.host_nonce,
            witness_nonce: self.witness_nonce,
            at_ms: self.at_ms,
        })
    }
    fn verify(&self, home: &Path) -> Result<()> {
        let identity = crate::identity::load(home)?;
        let cache = crate::store::build_kel_cache(home, &identity)?;
        let host = Did::parse(&self.host_root).map_err(err)?;
        let witness = Did::parse(&self.witness_root).map_err(err)?;
        let fields = self.fields()?;
        if self.id != blake3::hash(&fields.transcript()).to_hex().as_str() {
            return Err(err("receipt id mismatch"));
        }
        let get = |d: &Did| {
            cache
                .get(d)
                .ok_or_else(|| err("receipt party is not trusted"))
        };
        let policy = mini_storage::FreshnessPolicy::default_policy();
        let ctx = mini_storage::VerifyContext {
            host_root: get(&host)?,
            witness_root: get(&witness)?,
            host_device: get(&fields.host_device)?,
            witness_device: get(&fields.witness_device)?,
            policy: &policy,
            now_ms: Some(now()),
        };
        mini_storage::verify_serve(
            &ServeReceipt::new(
                fields,
                decode_sigs(&self.host_sig)?,
                decode_sigs(&self.witness_sig)?,
            ),
            &ctx,
            &mut mini_storage::InMemoryReplayGuard::new(),
        )
        .map_err(err)?;
        Ok(())
    }
}
fn records(home: &Path) -> Result<Vec<Evidence>> {
    let dir = home.join("seed-evidence");
    safe_dir(&dir)?;
    let mut records = Vec::new();
    for entry in fs::read_dir(&dir).map_err(err)? {
        let entry = entry.map_err(err)?;
        if entry.file_name() == "journal.lock" {
            continue;
        }
        if records.len() >= MAX_RECORDS {
            return Err(err("evidence journal full; archive explicitly"));
        }
        let e: Evidence =
            serde_json::from_slice(&read_file(&entry.path(), MAX_RECORD)?).map_err(err)?;
        if entry.file_name().to_string_lossy() != format!("{}.json", e.id) {
            return Err(err("corrupt evidence filename"));
        }
        records.push(e);
    }
    Ok(records)
}
fn save_evidence(home: &Path, e: &Evidence) -> Result<()> {
    e.verify(home)?;
    let dir = home.join("seed-evidence");
    safe_dir(&dir)?;
    let _lock = mini_durable::lock_exclusive(&dir.join("journal.lock")).map_err(err)?;
    let all = records(home)?;
    if all.iter().any(|old| {
        old.id == e.id
            || (old.host_device == e.host_device && old.host_nonce == e.host_nonce)
            || (old.witness_device == e.witness_device && old.witness_nonce == e.witness_nonce)
            || (old.host_device == e.witness_device && old.host_nonce == e.witness_nonce)
            || (old.witness_device == e.host_device && old.witness_nonce == e.host_nonce)
    }) {
        return Err(err("receipt replay"));
    }
    if all.len() >= MAX_RECORDS {
        return Err(err("evidence journal full"));
    }
    let bytes = serde_json::to_vec(e).map_err(err)?;
    mini_durable::atomic_replace(&dir.join(format!("{}.json", e.id)), &bytes).map_err(err)
}
fn check_payment(
    payment: Option<&[u8]>,
    offer: &Offer,
) -> Result<Option<mini_settlement::PaymentClaim>> {
    if offer.amount_micro == 0 {
        if payment.is_some() {
            return Err(err("free transfer cannot accept a payment claim"));
        }
        return Ok(None);
    }
    let claim = mini_settlement::PaymentClaim::from_wire_bytes(payment.ok_or_else(|| {
        err("provider requires an explicitly supplied signed wallet payment claim")
    })?)
    .map_err(err)?;
    mini_settlement::verify_claim_signature(&claim).map_err(err)?;
    if claim.amount_micro != offer.amount_micro
        || claim.payee != offer.price.payee
        || claim.network_id != offer.price.network_id
        || claim.valid_until_ms <= now()
    {
        return Err(err(
            "payment promise does not match live quote/network/payee/validity",
        ));
    }
    Ok(Some(claim))
}
fn reserve_payment(home: &Path, claim: &mini_settlement::PaymentClaim) -> Result<()> {
    let dir = home.join("seed-payment-claims");
    safe_dir(&dir)?;
    let _lock = mini_durable::lock_exclusive(&dir.join("claims.lock")).map_err(err)?;
    let mut count = 0;
    for e in fs::read_dir(&dir).map_err(err)? {
        e.map_err(err)?;
        count += 1;
        if count > MAX_RECORDS {
            return Err(err("payment journal full"));
        }
    }
    let key = serde_json::to_vec(&(claim.network_id, &claim.payer, claim.sequence)).map_err(err)?;
    let dest = dir.join(format!("{}.claim", blake3::hash(&key).to_hex()));
    // Exact sequence slot is reserved durably before bytes leave; failure never reuses it.
    if dest.try_exists().map_err(err)? {
        return Err(err("payment sequence already used on this provider"));
    }
    mini_durable::atomic_replace(&dest, &claim.to_wire_bytes().map_err(err)?).map_err(err)
}
fn offer(home: &Path, id: &str) -> Result<Offer> {
    let lease = pool(home)?.lease(id, now()).map_err(err)?;
    let p = price(home)?;
    Ok(Offer {
        id: lease.id,
        bytes: lease.bytes,
        expires_at_ms: lease.expires_at_ms,
        amount_micro: cost(lease.bytes, &p)?,
        price: p,
    })
}
pub(crate) fn serve_stream(
    home: &Path,
    stream: TcpStream,
    allowed: &[Did],
    on_authenticated: impl FnOnce(),
) -> Result<()> {
    let identity = crate::identity::load(home)?;
    let mut b = TcpBearer::from_stream(stream).map_err(err)?;
    let hello = b.recv().map_err(err)?;
    let (mut c, response) = Responder::respond(&hello).map_err(err)?;
    b.send(&response).map_err(err)?;
    let transcript = crate::sync::session_transcript(&hello, &response);
    let peer = crate::sync::receive_session_proof(
        &mut b,
        &mut c,
        home,
        &identity,
        &transcript,
        b'i',
        None,
    )?;
    if !allowed.contains(&peer) {
        return Err(err("peer is not allowed to use this seed service"));
    }
    // The peer is proven and allowlisted: it no longer counts as a pending
    // handshake, so long transfers are not cut off by the handshake deadline.
    on_authenticated();
    crate::sync::send_session_proof(&mut b, &mut c, &identity, &transcript, b'r', &peer)?;
    match recv::<Request>(&mut b, &mut c)? {
        Request::Advertisement => mesh::advertise(home, &mut b, &mut c)?,
        Request::Catalog => mesh::serve_catalog(home, &mut b, &mut c)?,
        Request::Search {
            query,
            after,
            limit,
        } => mesh::serve_search(home, &mut b, &mut c, &query, after.as_deref(), limit)?,
        Request::Inventory => {
            let leases = pool(home)?.inventory(now()).map_err(err)?;
            // Bounded pagination avoids manufacturing one oversized advertisement.
            for batch in leases.chunks(128) {
                send(&mut b, &mut c, &json!({"items":batch,"done":false}))?;
            }
            send(&mut b, &mut c, &json!({"items":[],"done":true}))?;
        }
        Request::Quote { id } => send(&mut b, &mut c, &offer(home, &id)?)?,
        Request::Fetch {
            id,
            witness_device,
            nonce,
            max_micro,
            payment_digest,
            max_bytes,
        } => {
            if peer == identity.human_did() {
                return Err(err("self-serving cannot produce reward evidence"));
            }
            let o = offer(home, &id)?;
            if max_bytes == 0 || max_bytes > MAX_OBJECT as u64 || o.bytes > max_bytes {
                return Err(err("object exceeds receiver byte window"));
            }
            if o.amount_micro > max_micro {
                return Err(err("price exceeds owner budget"));
            }
            if (o.amount_micro > 0) != payment_digest.is_some() {
                return Err(err(
                    "paid offer requires a payment commitment; free offer rejects payment",
                ));
            }
            let cache = crate::store::build_kel_cache(home, &identity)?;
            let witness_device = Did::parse(&witness_device).map_err(err)?;
            let root = cache
                .get(&peer)
                .ok_or_else(|| err("witness root unknown"))?;
            let device = cache
                .get(&witness_device)
                .ok_or_else(|| err("witness device unknown"))?;
            if !did_mini::verify_delegation(root, device)
                .map_err(err)?
                .contains(did_mini::Capabilities::ATTEST)
            {
                return Err(err("witness lacks ATTEST capability"));
            }
            let bytes = pool(home)?.read(&id, now()).map_err(err)?;
            if bytes.len() as u64 != o.bytes {
                return Err(err("seed object changed after quote"));
            }
            send(&mut b, &mut c, &o)?;
            for chunk in bytes.chunks(CHUNK) {
                send_bytes(&mut b, &mut c, chunk)?;
            }
            let fields = ReceiptFields {
                version: mini_storage::RECEIPT_VERSION,
                content_id: ObjectId::parse(&id).map_err(err)?,
                bytes: bytes.len() as u64,
                content_digest: *blake3::hash(&bytes).as_bytes(),
                host_device: identity.device_did(),
                witness_device,
                host_nonce: mini_crypto::random_32().map_err(err)?,
                witness_nonce: nonce,
                at_ms: now(),
            };
            let mut e = Evidence {
                id: blake3::hash(&fields.transcript()).to_hex().to_string(),
                content_id: id,
                bytes: fields.bytes,
                digest: fields.content_digest,
                host_root: identity.human_did().to_string(),
                witness_root: peer.to_string(),
                host_device: fields.host_device.to_string(),
                witness_device: fields.witness_device.to_string(),
                host_nonce: fields.host_nonce,
                witness_nonce: nonce,
                at_ms: fields.at_ms,
                host_sig: encode_sigs(&fields.sign(&identity.device)),
                witness_sig: Vec::new(),
                payment: None,
            };
            send(&mut b, &mut c, &e)?;
            let ack: DeliveryAck = recv(&mut b, &mut c)?;
            if ack.payment.as_deref().map(|p| *blake3::hash(p).as_bytes()) != payment_digest {
                return Err(err("payment commitment mismatch"));
            }
            e.witness_sig = ack.witness_sig;
            e.payment = ack.payment;
            e.verify(home)?;
            let claim = check_payment(e.payment.as_deref(), &o)?;
            if let Some(claim) = &claim {
                reserve_payment(home, claim)?;
            }
            save_evidence(home, &e)?;
            send(
                &mut b,
                &mut c,
                &json!({"receipt_id":e.id,"delivery_verified":true,"payment_final":false,"payment_state":if claim.is_some(){"AcceptedLocal"}else{"NoPayment"}}),
            )?;
        }
    }
    Ok(())
}
struct Session {
    bearer: TcpBearer,
    channel: Channel,
    _cancel: std::sync::mpsc::Sender<()>,
}
fn connect(home: &Path, address: &str, peer: &Did) -> Result<Session> {
    connect_until(
        home,
        address,
        peer,
        Instant::now() + Duration::from_secs(120),
    )
}
fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| err("operation deadline exhausted"))
}
fn connect_until(home: &Path, address: &str, peer: &Did, deadline: Instant) -> Result<Session> {
    let address = address
        .parse()
        .map_err(|_| err("numeric IP:port required"))?;
    let stream =
        TcpStream::connect_timeout(&address, remaining(deadline)?.min(Duration::from_secs(10)))
            .map_err(err)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(err)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(err)?;
    let shutdown = stream.try_clone().map_err(err)?;
    let (cancel, wait) = std::sync::mpsc::channel::<()>();
    let watchdog = remaining(deadline)?;
    std::thread::Builder::new()
        .name("mini-seed-deadline".into())
        .spawn(move || {
            if wait.recv_timeout(watchdog) == Err(std::sync::mpsc::RecvTimeoutError::Timeout) {
                let _ = shutdown.shutdown(std::net::Shutdown::Both);
            }
        })
        .map_err(err)?;
    let identity = crate::identity::load(home)?;
    let mut b = TcpBearer::from_stream(stream).map_err(err)?;
    let (initiator, hello) = Initiator::start().map_err(err)?;
    b.send(&hello).map_err(err)?;
    let response = b.recv().map_err(err)?;
    let mut c = initiator.finish(&response).map_err(err)?;
    let transcript = crate::sync::session_transcript(&hello, &response);
    crate::sync::send_session_proof(&mut b, &mut c, &identity, &transcript, b'i', peer)?;
    crate::sync::receive_session_proof(
        &mut b,
        &mut c,
        home,
        &identity,
        &transcript,
        b'r',
        Some(peer),
    )?;
    Ok(Session {
        bearer: b,
        channel: c,
        _cancel: cancel,
    })
}
fn ingest(home: &Path, store: &Path, obj: &Object) -> Result<()> {
    let identity = crate::identity::load(home)?;
    let source = crate::store::open_store(store)?;
    let mut cache = crate::store::build_kel_cache(home, &identity)?;
    cache.hydrate_from_store(&source).map_err(err)?;
    match mini_sync::Ingest::check(&mut cache, obj) {
        mini_sync::IngestOutcome::Accepted | mini_sync::IngestOutcome::AcceptedCarrier => Ok(()),
        _ => Err(err(
            "object signature/provenance is not verified; trust its author KEL first",
        )),
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeliveryAck {
    witness_sig: Vec<Sig>,
    payment: Option<Vec<u8>>,
}
struct FetchOptions<'a> {
    max_micro: u64,
    payment: Option<Vec<u8>>,
    days: Option<u16>,
    deadline: Option<Instant>,
    byte_limit: u64,
    traffic_left: Option<&'a mut u64>,
}
fn fetch(
    home: &Path,
    store: &Path,
    address: &str,
    peer: &Did,
    id: &str,
    options: FetchOptions<'_>,
) -> Result<Value> {
    let FetchOptions {
        max_micro,
        payment,
        days,
        deadline,
        byte_limit,
        mut traffic_left,
    } = options;
    let identity = crate::identity::load(home)?;
    let nonce = mini_crypto::random_32().map_err(err)?;
    let mut s = connect_until(
        home,
        address,
        peer,
        deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(120)),
    )?;
    send(
        &mut s.bearer,
        &mut s.channel,
        &Request::Fetch {
            id: id.into(),
            witness_device: identity.device_did().to_string(),
            nonce,
            max_micro,
            payment_digest: payment.as_deref().map(|p| *blake3::hash(p).as_bytes()),
            max_bytes: byte_limit.min(
                traffic_left
                    .as_deref()
                    .copied()
                    .unwrap_or(MAX_OBJECT as u64),
            ),
        },
    )?;
    let o: Offer = recv(&mut s.bearer, &mut s.channel)?;
    if o.id != id
        || o.bytes > MAX_OBJECT as u64
        || o.bytes > byte_limit
        || o.bytes
            > traffic_left
                .as_deref()
                .copied()
                .unwrap_or(MAX_OBJECT as u64)
        || o.bytes == 0
        || o.expires_at_ms <= now()
        || o.amount_micro > max_micro
        || cost(o.bytes, &o.price)? != o.amount_micro
    {
        return Err(err("invalid/stale/over-budget offer"));
    }
    check_payment(payment.as_deref(), &o)?;
    let mut bytes = Vec::with_capacity(o.bytes as usize);
    while bytes.len() < o.bytes as usize {
        let chunk = receive_bytes(&mut s.bearer, &mut s.channel, CHUNK)?;
        if let Some(left) = traffic_left.as_deref_mut() {
            *left = left
                .checked_sub(chunk.len() as u64)
                .ok_or_else(|| err("received body byte window exhausted"))?;
        }
        if chunk.is_empty() || bytes.len() + chunk.len() > o.bytes as usize {
            return Err(err("invalid object chunk length"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let obj = Object::from_bytes(&bytes).map_err(err)?;
    if obj.id().as_str() != id || !matches!(obj.payload, mini_objects::Payload::Public(_)) {
        return Err(err("received object mismatch or non-public payload"));
    }
    ingest(home, store, &obj)?;
    let mut e: Evidence = recv(&mut s.bearer, &mut s.channel)?;
    if e.content_id != id
        || e.bytes != o.bytes
        || e.digest != *blake3::hash(&bytes).as_bytes()
        || e.host_root != peer.as_str()
        || e.witness_root != identity.human_did().as_str()
        || e.witness_device != identity.device_did().as_str()
        || e.witness_nonce != nonce
        || e.payment.is_some()
        || !e.witness_sig.is_empty()
    {
        return Err(err("receipt does not match verified received content"));
    }
    e.payment = payment.clone();
    e.witness_sig = encode_sigs(&e.fields()?.sign(&identity.device));
    e.verify(home)?;
    // Save the content and the signed pending evidence before acknowledging delivery.
    crate::store::open_store(store)?.insert(&obj).map_err(err)?;
    save_evidence(home, &e)?;
    send(
        &mut s.bearer,
        &mut s.channel,
        &DeliveryAck {
            witness_sig: e.witness_sig.clone(),
            payment: payment.clone(),
        },
    )?;
    let ack: Value = recv(&mut s.bearer, &mut s.channel)?;
    if ack["receipt_id"] != e.id
        || ack["delivery_verified"] != true
        || ack["payment_final"] != false
    {
        return Err(err(
            "invalid provider acknowledgement; local evidence is retained",
        ));
    }
    let reseed_error = days
        .and_then(|days| {
            pool(home)
                .and_then(|p| p.admit(&obj, days, now()).map_err(err))
                .err()
        })
        .map(|e| e.to_string());
    Ok(
        json!({"object_id":id,"bytes":o.bytes,"receipt_id":e.id,"delivery_verified":true,"reseeded":days.is_some() && reseed_error.is_none(),"reseed_error":reseed_error,"payment_state":if payment.is_some(){"AcceptedLocal"}else{"NoPayment"},"payment_final":false,"amount_micro":o.amount_micro}),
    )
}
fn decode_hex(s: &str) -> Result<Vec<u8>> {
    if s.len() > 8192 || s.len() % 2 != 0 || !s.is_ascii() {
        return Err(err("invalid hex"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(err))
        .collect()
}
pub fn dispatch(home: &Path, store: &Path, mut args: Vec<String>, as_json: bool) -> Result<String> {
    if args.is_empty() {
        return Err(err("configure | add | collection-preview | collection-add | collection-fetch | media-export | revoke | prune | status | inventory | serve | contact | announce | catalog | locate | retrieve | search | follow | mesh-follow | follow-stop | peers | quote | fetch | price | earnings | payment-export"));
    }
    let verb = args.remove(0);
    if matches!(
        verb.as_str(),
        "collection-preview" | "collection-add" | "collection-fetch" | "media-export"
    ) {
        return render(collection::dispatch(home, store, &verb, args)?, as_json);
    }
    if matches!(
        verb.as_str(),
        "contact"
            | "announce"
            | "unannounce"
            | "catalog"
            | "locate"
            | "retrieve"
            | "search"
            | "mesh-follow"
    ) {
        return render(mesh::dispatch(home, store, &verb, args)?, as_json);
    }
    if verb == "serve" {
        args.insert(0, "--seed-only".into());
        args.insert(0, "run".into());
        return crate::service::dispatch(home, store, args, as_json);
    }
    let result = match verb.as_str() {
        "configure" => {
            let limit = number(&need(&mut args, "--max-bytes")?)?;
            done(&args)?;
            json!(pool(home)?.configure(limit).map_err(err)?)
        }
        "add" => {
            let consent = if let Some(i) = args.iter().position(|s| s == "--allow-public-seeding") {
                args.remove(i);
                true
            } else {
                false
            };
            if !consent {
                return Err(err(
                    "add requires --allow-public-seeding for this exact object",
                ));
            }
            let id = ObjectId::parse(&need(&mut args, "--id")?).map_err(err)?;
            let days = u16::try_from(number(&need(&mut args, "--days")?)?).map_err(err)?;
            done(&args)?;
            let source = crate::store::open_store(store)?;
            let obj = source.get(&id).map_err(err)?;
            if matches!(
                source.cache_tier(&id).map_err(err)?,
                mini_store::CacheTier::PrivateOnly | mini_store::CacheTier::ColdArchive
            ) {
                return Err(err(
                    "private/cold objects cannot enter the public seed pool",
                ));
            }
            ingest(home, store, &obj)?;
            json!(pool(home)?.admit(&obj, days, now()).map_err(err)?)
        }
        "revoke" => {
            let id = need(&mut args, "--id")?;
            done(&args)?;
            json!({"revoked":pool(home)?.revoke(&id).map_err(err)?})
        }
        "prune" => {
            done(&args)?;
            json!({"removed":pool(home)?.prune(now()).map_err(err)?})
        }
        "status" => {
            done(&args)?;
            json!({"pool":pool(home)?.status().map_err(err)?,"active":pool(home)?.inventory(now()).map_err(err)?.len(),"price":price(home)?,"payment_final":false,"scope":"explicit public objects only"})
        }
        "inventory" => {
            done(&args)?;
            json!({"items":pool(home)?.inventory(now()).map_err(err)?})
        }
        "follow-stop" => {
            done(&args)?;
            safe_dir(home)?;
            mini_durable::atomic_replace(&home.join("seed-follow.stop"), b"stop\n").map_err(err)?;
            json!({"stop_requested":true,"note":"A running follow checks this between transfers/rounds; an in-flight bounded request may finish."})
        }
        "follow" => {
            let resume = if let Some(i) = args.iter().position(|s| s == "--resume") {
                args.remove(i);
                true
            } else {
                false
            };
            let address = need(&mut args, "--addr")?;
            let peer = Did::parse(&need(&mut args, "--peer")?).map_err(err)?;
            let days = u16::try_from(number(&need(&mut args, "--days")?)?).map_err(err)?;
            let rounds = number(&need(&mut args, "--rounds")?)?;
            let interval = number(&need(&mut args, "--interval-ms")?)?;
            let max = number(&need(&mut args, "--max-objects")?)?;
            done(&args)?;
            if !(1..=3650).contains(&days)
                || !(1..=86400).contains(&rounds)
                || !(1000..=60000).contains(&interval)
                || !(1..=4096).contains(&max)
            {
                return Err(err("follow requires days1..3650, rounds1..86400, interval-ms1000..60000, max-objects1..4096"));
            }
            safe_dir(home)?;
            let _follow =
                mini_durable::try_lock_exclusive(&home.join("seed-follow.lock")).map_err(err)?;
            if resume && home.join("seed-follow.stop").try_exists().map_err(err)? {
                fs::remove_file(home.join("seed-follow.stop")).map_err(err)?;
                mini_durable::sync_directory(home).map_err(err)?;
            }
            // Remember every object this run already holds/sees through expiry: polling
            // must not redownload it to silently reset its chosen retention period.
            let mut observed: std::collections::BTreeSet<_> = pool(home)?
                .inventory(now())
                .map_err(err)?
                .into_iter()
                .map(|l| l.id)
                .collect();
            let mut transfers = Vec::new();
            for round in 0..rounds {
                if home.join("seed-follow.stop").try_exists().map_err(err)? {
                    break;
                }
                pool(home)?.prune(now()).map_err(err)?;
                let inventory = remote_inventory(home, &address, &peer)?;
                let existing: std::collections::BTreeSet<_> = pool(home)?
                    .inventory(now())
                    .map_err(err)?
                    .into_iter()
                    .map(|l| l.id)
                    .collect();
                for lease in inventory {
                    if home.join("seed-follow.stop").try_exists().map_err(err)? {
                        break;
                    }
                    if transfers.len() >= max as usize {
                        break;
                    }
                    if !observed.contains(&lease.id) && !existing.contains(&lease.id) {
                        observed.insert(lease.id.clone());
                        let result = fetch(
                            home,
                            store,
                            &address,
                            &peer,
                            &lease.id,
                            FetchOptions {
                                max_micro: 0,
                                payment: None,
                                days: Some(days),
                                deadline: None,
                                byte_limit: MAX_OBJECT as u64,
                                traffic_left: None,
                            },
                        )?;
                        let failed = result["reseed_error"].is_string();
                        transfers.push(result);
                        if failed {
                            return render(
                                json!({"transfers":transfers,"complete":false,"reason":"reseed failed; delivery evidence retained"}),
                                as_json,
                            );
                        }
                    }
                }
                if transfers.len() >= max as usize {
                    break;
                }
                if round + 1 < rounds {
                    std::thread::sleep(Duration::from_millis(interval));
                }
            }
            json!({"transfers":transfers,"complete":!home.join("seed-follow.stop").try_exists().map_err(err)?,"paid_requests":0,"stop_requested":home.join("seed-follow.stop").try_exists().map_err(err)?,"note":"Explicit peer polling; no discovery, paid fetch or automatic lease renewal."})
        }
        "price" => {
            let rate = number(&need(&mut args, "--micro-per-mib")?)?;
            let payee = decode_hex(&need(&mut args, "--payee-hex")?)?;
            let network = decode_hex(&need(&mut args, "--network-hex")?)?;
            done(&args)?;
            if network.len() != 32 || (rate > 0 && payee.is_empty()) {
                return Err(err("network must be32 bytes; paid offers require a payee"));
            }
            let mut network_id = [0; 32];
            network_id.copy_from_slice(&network);
            let p = Price {
                micro_per_mib: rate,
                payee,
                network_id,
            };
            let encoded = serde_json::to_vec(&p).map_err(err)?;
            if encoded.len() > 4096 {
                return Err(err("price configuration exceeds4096 bytes"));
            }
            safe_dir(home)?;
            mini_durable::atomic_replace(&home.join("seed-price.json"), &encoded).map_err(err)?;
            json!(p)
        }
        "peers" | "quote" | "fetch" => {
            let address = need(&mut args, "--addr")?;
            let peer = Did::parse(&need(&mut args, "--peer")?).map_err(err)?;
            if verb == "fetch" {
                let id = need(&mut args, "--id")?;
                ObjectId::parse(&id).map_err(err)?;
                let max = take(&mut args, "--max-micro")?
                    .map(|s| number(&s))
                    .transpose()?
                    .unwrap_or(0);
                let payment = take(&mut args, "--payment-claim")?
                    .map(|s| read_file(Path::new(&s), mini_settlement::MAX_PAYMENT_CLAIM_BYTES))
                    .transpose()?;
                let days = take(&mut args, "--reseed-days")?
                    .map(|s| number(&s).and_then(|n| u16::try_from(n).map_err(err)))
                    .transpose()?;
                if days.is_some_and(|d| !(1..=3650).contains(&d)) {
                    return Err(err("reseed-days must be1..3650"));
                }
                done(&args)?;
                fetch(
                    home,
                    store,
                    &address,
                    &peer,
                    &id,
                    FetchOptions {
                        max_micro: max,
                        payment,
                        days,
                        deadline: None,
                        byte_limit: MAX_OBJECT as u64,
                        traffic_left: None,
                    },
                )?
            } else {
                let request = if verb == "quote" {
                    Request::Quote {
                        id: need(&mut args, "--id")?,
                    }
                } else {
                    Request::Inventory
                };
                done(&args)?;
                let mut s = connect(home, &address, &peer)?;
                send(&mut s.bearer, &mut s.channel, &request)?;
                if verb == "quote" {
                    json!(recv::<Offer>(&mut s.bearer, &mut s.channel)?)
                } else {
                    let mut all = Vec::new();
                    for _ in 0..=32 {
                        let page: Value = recv(&mut s.bearer, &mut s.channel)?;
                        let items = page["items"]
                            .as_array()
                            .ok_or_else(|| err("bad inventory"))?;
                        if items.len() > 128 || all.len() + items.len() > 4096 {
                            return Err(err("inventory exceeds limit"));
                        }
                        all.extend(items.clone());
                        if page["done"] == true {
                            return render(json!({"items":all,"peer":peer.as_str()}), as_json);
                        }
                    }
                    return Err(err("inventory did not terminate"));
                }
            }
        }
        "earnings" => {
            done(&args)?;
            let all = records(home)?;
            let identity = crate::identity::load(home)?;
            let items:Vec<_>=all.iter().filter(|e|e.host_root==identity.human_did().as_str()).map(|e|json!({"receipt_id":e.id,"object_id":e.content_id,"bytes":e.bytes,"payment_state":if e.payment.is_some(){"AcceptedLocal"}else{"NoPayment"},"payment_final":false})).collect();
            json!({"deliveries":items,"finalized_micro":null,"canonical_status_checked":false,"note":"Signed promises are not balances. Submit exported claims through canonical settlement; this journal never finalizes money."})
        }
        "payment-export" => {
            let id = need(&mut args, "--receipt")?;
            let output = need(&mut args, "--output")?;
            done(&args)?;
            let all = records(home)?;
            let e = all
                .iter()
                .find(|e| e.id == id)
                .ok_or_else(|| err("receipt missing"))?;
            let bytes = e
                .payment
                .as_ref()
                .ok_or_else(|| err("receipt has no payment claim"))?;
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output)
                .map_err(err)?;
            f.write_all(bytes).and_then(|_| f.sync_all()).map_err(err)?;
            json!({"output":output,"payment_state":"AcceptedLocal","payment_final":false})
        }
        _ => return Err(err("unknown seed command")),
    };
    render(result, as_json)
}
fn render(value: Value, as_json: bool) -> Result<String> {
    if as_json {
        serde_json::to_string(&json!({"ok":true,"command":"seed","result":value})).map_err(err)
    } else {
        serde_json::to_string_pretty(&value).map_err(err)
    }
}

fn remote_inventory(home: &Path, address: &str, peer: &Did) -> Result<Vec<mini_seed::Lease>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Page {
        items: Vec<mini_seed::Lease>,
        done: bool,
    }
    let mut s = connect(home, address, peer)?;
    send(&mut s.bearer, &mut s.channel, &Request::Inventory)?;
    let mut all = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..=32 {
        let page: Page = recv(&mut s.bearer, &mut s.channel)?;
        if page.items.len() > 128 || all.len() + page.items.len() > 4096 {
            return Err(err("inventory exceeds limit"));
        }
        for item in page.items {
            ObjectId::parse(&item.id).map_err(err)?;
            if !seen.insert(item.id.clone())
                || item.bytes == 0
                || item.bytes > MAX_OBJECT as u64
                || item.expires_at_ms <= now()
            {
                return Err(err("invalid inventory lease"));
            }
            all.push(item);
        }
        if page.done {
            return Ok(all);
        }
    }
    Err(err("inventory did not terminate"))
}
