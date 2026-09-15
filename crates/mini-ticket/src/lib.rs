//! Signed service tickets and the ledger that collects them.
//!
//! A [`ServiceTicket`] is issued by the side that **received** bytes over
//! one encrypted session. It names the provider's `did:mini`, the kind of
//! service, the byte and object counts, the CH1 channel binding of the
//! session it describes, a fresh nonce, and — for media that became complete
//! during the exchange — the manifests and their sizes, so a creator's share
//! can be attributed to the manifest author. It is an ordinary signed,
//! content-addressed [`Object`], so it replicates, deduplicates and verifies
//! exactly like a post: the receiver's device signature and delegation are
//! checked by the same ingest every other object passes.
//!
//! [`Ledger::collect`] sums the tickets on a device that name one provider,
//! prices them with `mini-resource-pricing`'s micro-MINI convention at Tier 0,
//! and splits media bytes between host and creator. A [`RedemptionRequest`]
//! is a typed, signed object that only the named provider can author
//! ([`verify_redemption`] rejects any other author), listing the exact
//! tickets it claims.
//!
//! ## What this is not
//!
//! Nothing here moves money. Credit is *unsettled*: a request becomes a
//! payout only when the audited settlement layer (D-0037/D-0047) accepts it
//! through its own admission rules, which this crate neither implements nor
//! bypasses. A ticket proves that one receiver attested to one exchange; it
//! does not prove the provider is honest, unique, or human, and a receiver
//! can refuse to issue one. Tickets carry no governance weight (P1) and
//! nothing here depends on the value or governance crates.

#![forbid(unsafe_code)]

use did_mini::{Controller, Did};
use mini_media::read_manifest;
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_privacy_policy::PrivacyTier;
use mini_resource_pricing::{quote, PriceVector};
use mini_store::{Backend, Store};
use std::collections::{BTreeMap, BTreeSet};

pub const TICKET_TYPE: &str = "mininet/service-ticket/v1";
pub const REDEMPTION_TYPE: &str = "mininet/ticket-redemption/v1";
const TICKET_VERSION: u8 = 1;
const REDEMPTION_VERSION: u8 = 1;
/// Manifests one ticket may attribute; more are counted as plain bytes.
pub const MAX_TICKET_MANIFESTS: usize = 64;
/// Tickets one redemption request may reference (object link cap).
pub const MAX_REDEMPTION_TICKETS: usize = 200;
const MAX_DID_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TicketError {
    NotATicket,
    NotARedemption,
    Malformed,
    Overflow,
    TooManyManifests,
    TooManyTickets,
    NotTheNamedProvider,
    TicketMissing(ObjectId),
    Store(String),
    Object(String),
    Pricing(String),
}

impl std::fmt::Display for TicketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TicketError::NotATicket => write!(f, "object is not a service ticket"),
            TicketError::NotARedemption => write!(f, "object is not a redemption request"),
            TicketError::Malformed => write!(f, "ticket payload is malformed"),
            TicketError::Overflow => write!(f, "arithmetic overflow"),
            TicketError::TooManyManifests => write!(f, "too many manifests for one ticket"),
            TicketError::TooManyTickets => write!(f, "too many tickets for one redemption"),
            TicketError::NotTheNamedProvider => {
                write!(f, "redemption author is not the provider the tickets name")
            }
            TicketError::TicketMissing(id) => {
                write!(f, "ticket {} is not on this device", id.as_str())
            }
            TicketError::Store(e) => write!(f, "store: {e}"),
            TicketError::Object(e) => write!(f, "object: {e}"),
            TicketError::Pricing(e) => write!(f, "pricing: {e}"),
        }
    }
}

impl std::error::Error for TicketError {}

pub type Result<T> = std::result::Result<T, TicketError>;

/// What was served.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    /// Public `MINI/SYNC1` objects (posts, profiles, media chunks…).
    PublicSync,
    /// Envelopes for one private conversation route.
    PrivateSync,
}

impl Service {
    fn code(self) -> u8 {
        match self {
            Service::PublicSync => 1,
            Service::PrivateSync => 2,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Service::PublicSync),
            2 => Some(Service::PrivateSync),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Service::PublicSync => "public sync",
            Service::PrivateSync => "private conversation",
        }
    }
}

/// A media manifest that became complete on the receiver during the
/// exchange, with its total length in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedMedia {
    pub manifest: ObjectId,
    pub bytes: u64,
}

/// The fields a receiver attests to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketFields {
    pub provider: Did,
    pub service: Service,
    pub bytes_received: u64,
    pub objects_received: u32,
    pub channel_binding: [u8; 32],
    pub nonce: [u8; 32],
    pub completed_media: Vec<CompletedMedia>,
}

/// A decoded ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceTicket {
    pub id: ObjectId,
    /// The receiver who attested (the object's author).
    pub consumer: Did,
    pub fields: TicketFields,
    pub timestamp_ms: u64,
}

fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u32(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(TicketError::Malformed)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(TicketError::Malformed)?;
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_be_bytes(a))
    }

    fn bytes32(&mut self) -> Result<[u8; 32]> {
        let b = self.take(32)?;
        let mut a = [0u8; 32];
        a.copy_from_slice(b);
        Ok(a)
    }

    fn str(&mut self, max: usize) -> Result<String> {
        let len = self.u32()? as usize;
        if len > max {
            return Err(TicketError::Malformed);
        }
        String::from_utf8(self.take(len)?.to_vec()).map_err(|_| TicketError::Malformed)
    }

    fn done(&self) -> Result<()> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err(TicketError::Malformed)
        }
    }
}

fn encode_ticket(fields: &TicketFields) -> Result<Vec<u8>> {
    if fields.completed_media.len() > MAX_TICKET_MANIFESTS {
        return Err(TicketError::TooManyManifests);
    }
    let mut out = Vec::new();
    out.push(TICKET_VERSION);
    put_str(&mut out, fields.provider.as_str());
    out.push(fields.service.code());
    put_u64(&mut out, fields.bytes_received);
    put_u32(&mut out, fields.objects_received);
    out.extend_from_slice(&fields.channel_binding);
    out.extend_from_slice(&fields.nonce);
    put_u32(&mut out, fields.completed_media.len() as u32);
    for media in &fields.completed_media {
        put_str(&mut out, media.manifest.as_str());
        put_u64(&mut out, media.bytes);
    }
    Ok(out)
}

/// Sign a ticket as the receiver. `sequence` is the author's next object
/// sequence, exactly as for a post.
pub fn issue_ticket(
    consumer: &Did,
    device: &Controller,
    fields: &TicketFields,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let payload = encode_ticket(fields)?;
    let mut builder = ObjectBuilder::new(ObjectType::Custom(TICKET_TYPE.into()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload));
    for media in &fields.completed_media {
        builder = builder.link("manifest", media.manifest.clone());
    }
    builder
        .sign(consumer, device)
        .map_err(|error| TicketError::Object(error.to_string()))
}

/// Strictly decode a ticket object. Signature and delegation are *not*
/// checked here — that is the ingest pipeline's job on receipt.
pub fn read_ticket(object: &Object) -> Result<ServiceTicket> {
    if object.object_type != ObjectType::Custom(TICKET_TYPE.into()) {
        return Err(TicketError::NotATicket);
    }
    let Payload::Public(bytes) = &object.payload else {
        return Err(TicketError::Malformed);
    };
    let mut r = Reader { bytes, pos: 0 };
    if r.u8()? != TICKET_VERSION {
        return Err(TicketError::Malformed);
    }
    let provider = Did::parse(&r.str(MAX_DID_BYTES)?).map_err(|_| TicketError::Malformed)?;
    let service = Service::from_code(r.u8()?).ok_or(TicketError::Malformed)?;
    let bytes_received = r.u64()?;
    let objects_received = r.u32()?;
    let channel_binding = r.bytes32()?;
    let nonce = r.bytes32()?;
    let count = r.u32()? as usize;
    if count > MAX_TICKET_MANIFESTS {
        return Err(TicketError::Malformed);
    }
    let mut completed_media = Vec::with_capacity(count);
    let mut media_bytes: u64 = 0;
    for _ in 0..count {
        let manifest =
            ObjectId::parse(&r.str(MAX_DID_BYTES)?).map_err(|_| TicketError::Malformed)?;
        let bytes = r.u64()?;
        media_bytes = media_bytes
            .checked_add(bytes)
            .ok_or(TicketError::Overflow)?;
        completed_media.push(CompletedMedia { manifest, bytes });
    }
    r.done()?;
    // Every attributed manifest must also be a link, and attributed media
    // cannot exceed what was received at all.
    let links: BTreeSet<&str> = object
        .links
        .iter()
        .filter(|link| link.rel == "manifest")
        .map(|link| link.target.as_str())
        .collect();
    if links.len() != completed_media.len()
        || completed_media
            .iter()
            .any(|media| !links.contains(media.manifest.as_str()))
        || media_bytes > bytes_received
    {
        return Err(TicketError::Malformed);
    }
    Ok(ServiceTicket {
        id: object.id().clone(),
        consumer: object.author_human.clone(),
        fields: TicketFields {
            provider,
            service,
            bytes_received,
            objects_received,
            channel_binding,
            nonce,
            completed_media,
        },
        timestamp_ms: object.timestamp_ms,
    })
}

/// Owner-chosen rate and split. Defaults are deliberately modest; the
/// owner sets them, and nothing here enforces that a counterparty agrees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rate {
    /// Micro-MINI per megabyte served, Tier 0.
    pub micro_mini_per_mb: u64,
    /// Share of media bytes credited to the manifest author, in basis
    /// points; the rest goes to the host. Non-media bytes go to the host.
    pub creator_bps: u16,
}

impl Default for Rate {
    fn default() -> Self {
        Self {
            micro_mini_per_mb: 10,
            creator_bps: 3000,
        }
    }
}

impl Rate {
    pub fn validate(&self) -> Result<()> {
        if self.creator_bps > 10_000 {
            return Err(TicketError::Malformed);
        }
        Ok(())
    }

    /// Micro-MINI for `bytes` at this rate, priced through the shared Tier-0
    /// quote so the convention stays the one every other crate uses.
    pub fn price(&self, bytes: u64) -> Result<u64> {
        let mb = bytes.div_ceil(1_000_000);
        let prices = PriceVector {
            bandwidth_micro_mini_per_mb: self.micro_mini_per_mb,
            storage_micro_mini_per_mb_day: 0,
        };
        quote(&prices, PrivacyTier::Direct, mb, 0)
            .map(|quote| quote.max_micro_mini)
            .map_err(|error| TicketError::Pricing(error.to_string()))
    }
}

/// One line of a ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEntry {
    pub ticket: ServiceTicket,
    /// Micro-MINI credited to the host for this ticket.
    pub host_micro: u64,
    /// Micro-MINI credited to creators, by creator DID.
    pub creator_micro: BTreeMap<String, u64>,
}

/// Everything one identity is owed (unsettled) according to the tickets
/// on this device.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ledger {
    /// Tickets naming `me` as provider, deduplicated by consumer + nonce.
    pub as_host: Vec<LedgerEntry>,
    /// Micro-MINI owed to `me` as host.
    pub host_micro: u64,
    /// Micro-MINI owed to `me` as creator of media completed elsewhere.
    pub creator_micro: u64,
    /// Bytes `me` served according to the tickets.
    pub bytes_served: u64,
    /// Tickets `me` issued to others (what this device attested).
    pub issued: Vec<ServiceTicket>,
    /// Bytes `me` received according to issued tickets.
    pub bytes_received: u64,
    /// Micro-MINI `me` owes others for what it received, at `rate`.
    pub owed_micro: u64,
    /// Duplicate tickets ignored (same consumer and nonce).
    pub duplicates: usize,
    /// Objects of the ticket type that failed strict decoding.
    pub malformed: usize,
}

impl Ledger {
    /// Collect every ticket on the device that concerns `me`.
    pub fn collect<B: Backend>(store: &Store<B>, me: &Did, rate: Rate) -> Result<Self> {
        rate.validate()?;
        let ids = store
            .by_type(&ObjectType::Custom(TICKET_TYPE.into()))
            .map_err(|error| TicketError::Store(error.to_string()))?;
        let mut ledger = Ledger::default();
        let mut seen: BTreeSet<(String, [u8; 32])> = BTreeSet::new();
        let mut tickets: Vec<ServiceTicket> = Vec::new();
        for id in ids {
            let object = store
                .get(&id)
                .map_err(|error| TicketError::Store(error.to_string()))?;
            match read_ticket(&object) {
                Ok(ticket) => tickets.push(ticket),
                Err(_) => ledger.malformed += 1,
            }
        }
        tickets.sort_by(|a, b| {
            a.timestamp_ms
                .cmp(&b.timestamp_ms)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        for ticket in tickets {
            if !seen.insert((ticket.consumer.as_str().to_owned(), ticket.fields.nonce)) {
                ledger.duplicates += 1;
                continue;
            }
            if &ticket.consumer == me {
                ledger.bytes_received = ledger
                    .bytes_received
                    .checked_add(ticket.fields.bytes_received)
                    .ok_or(TicketError::Overflow)?;
                ledger.owed_micro = ledger
                    .owed_micro
                    .checked_add(rate.price(ticket.fields.bytes_received)?)
                    .ok_or(TicketError::Overflow)?;
                ledger.issued.push(ticket);
                continue;
            }
            // Creator attribution for media completed in this exchange.
            let mut creator_micro: BTreeMap<String, u64> = BTreeMap::new();
            let mut media_bytes: u64 = 0;
            for media in &ticket.fields.completed_media {
                let Ok(object) = store.get(&media.manifest) else {
                    continue;
                };
                if read_manifest(&object).is_err() {
                    continue;
                }
                media_bytes = media_bytes
                    .checked_add(media.bytes)
                    .ok_or(TicketError::Overflow)?;
                let share = rate
                    .price(media.bytes)?
                    .checked_mul(u64::from(rate.creator_bps))
                    .ok_or(TicketError::Overflow)?
                    / 10_000;
                let entry = creator_micro
                    .entry(object.author_human.as_str().to_owned())
                    .or_insert(0);
                *entry = entry.checked_add(share).ok_or(TicketError::Overflow)?;
            }
            let total = rate.price(ticket.fields.bytes_received)?;
            let creator_total: u64 = creator_micro.values().sum();
            let host_micro = total.saturating_sub(creator_total);
            if &ticket.fields.provider == me {
                ledger.host_micro = ledger
                    .host_micro
                    .checked_add(host_micro)
                    .ok_or(TicketError::Overflow)?;
                ledger.bytes_served = ledger
                    .bytes_served
                    .checked_add(ticket.fields.bytes_received)
                    .ok_or(TicketError::Overflow)?;
            }
            if let Some(mine) = creator_micro.get(me.as_str()) {
                ledger.creator_micro = ledger
                    .creator_micro
                    .checked_add(*mine)
                    .ok_or(TicketError::Overflow)?;
            }
            if &ticket.fields.provider == me || creator_micro.contains_key(me.as_str()) {
                ledger.as_host.push(LedgerEntry {
                    ticket,
                    host_micro,
                    creator_micro,
                });
            }
        }
        Ok(ledger)
    }
}

/// A decoded redemption request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedemptionRequest {
    pub id: ObjectId,
    /// The provider claiming; must equal every referenced ticket's provider.
    pub claimant: Did,
    pub tickets: Vec<ObjectId>,
    pub bytes: u64,
    pub micro_mini: u64,
    pub rate: Rate,
    pub timestamp_ms: u64,
}

/// Build a redemption request over `tickets`, signed by the claimant. Fails
/// unless every ticket is on the device and names `claimant` as provider —
/// nobody else can build a valid request, and a forged one is rejected by
/// [`verify_redemption`] on any device that holds the tickets.
#[allow(clippy::too_many_arguments)]
pub fn build_redemption<B: Backend>(
    store: &Store<B>,
    claimant: &Did,
    device: &Controller,
    tickets: &[ObjectId],
    rate: Rate,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    rate.validate()?;
    if tickets.is_empty() || tickets.len() > MAX_REDEMPTION_TICKETS {
        return Err(TicketError::TooManyTickets);
    }
    let mut bytes: u64 = 0;
    for id in tickets {
        let object = store
            .get(id)
            .map_err(|_| TicketError::TicketMissing(id.clone()))?;
        let ticket = read_ticket(&object)?;
        if &ticket.fields.provider != claimant {
            return Err(TicketError::NotTheNamedProvider);
        }
        bytes = bytes
            .checked_add(ticket.fields.bytes_received)
            .ok_or(TicketError::Overflow)?;
    }
    let micro_mini = rate.price(bytes)?;
    let mut payload = Vec::new();
    payload.push(REDEMPTION_VERSION);
    put_u64(&mut payload, bytes);
    put_u64(&mut payload, micro_mini);
    put_u64(&mut payload, rate.micro_mini_per_mb);
    payload.extend_from_slice(&rate.creator_bps.to_be_bytes());
    let mut builder = ObjectBuilder::new(ObjectType::Custom(REDEMPTION_TYPE.into()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload));
    for id in tickets {
        builder = builder.link("ticket", id.clone());
    }
    builder
        .sign(claimant, device)
        .map_err(|error| TicketError::Object(error.to_string()))
}

/// Decode a redemption request without checking it against tickets.
pub fn read_redemption(object: &Object) -> Result<RedemptionRequest> {
    if object.object_type != ObjectType::Custom(REDEMPTION_TYPE.into()) {
        return Err(TicketError::NotARedemption);
    }
    let Payload::Public(bytes) = &object.payload else {
        return Err(TicketError::Malformed);
    };
    let mut r = Reader { bytes, pos: 0 };
    if r.u8()? != REDEMPTION_VERSION {
        return Err(TicketError::Malformed);
    }
    let total = r.u64()?;
    let micro_mini = r.u64()?;
    let micro_mini_per_mb = r.u64()?;
    let bps = r.take(2)?;
    let creator_bps = u16::from_be_bytes([bps[0], bps[1]]);
    r.done()?;
    let tickets: Vec<ObjectId> = object
        .links
        .iter()
        .filter(|link| link.rel == "ticket")
        .map(|link| link.target.clone())
        .collect();
    if tickets.is_empty() || tickets.len() > MAX_REDEMPTION_TICKETS {
        return Err(TicketError::Malformed);
    }
    let rate = Rate {
        micro_mini_per_mb,
        creator_bps,
    };
    rate.validate()?;
    Ok(RedemptionRequest {
        id: object.id().clone(),
        claimant: object.author_human.clone(),
        tickets,
        bytes: total,
        micro_mini,
        rate,
        timestamp_ms: object.timestamp_ms,
    })
}

/// Check a redemption request against the tickets on this device: every
/// ticket must be present, name the claimant as provider, and the totals
/// must be exactly what the tickets add up to at the stated rate. This is
/// the "redeemable only by the named did:mini" rule, enforceable by anyone
/// holding the tickets.
pub fn verify_redemption<B: Backend>(store: &Store<B>, request: &RedemptionRequest) -> Result<()> {
    let mut bytes: u64 = 0;
    let mut seen: BTreeSet<(String, [u8; 32])> = BTreeSet::new();
    for id in &request.tickets {
        let object = store
            .get(id)
            .map_err(|_| TicketError::TicketMissing(id.clone()))?;
        let ticket = read_ticket(&object)?;
        if ticket.fields.provider != request.claimant {
            return Err(TicketError::NotTheNamedProvider);
        }
        if !seen.insert((ticket.consumer.as_str().to_owned(), ticket.fields.nonce)) {
            return Err(TicketError::Malformed);
        }
        bytes = bytes
            .checked_add(ticket.fields.bytes_received)
            .ok_or(TicketError::Overflow)?;
    }
    if bytes != request.bytes || request.rate.price(bytes)? != request.micro_mini {
        return Err(TicketError::Malformed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Capabilities;
    use mini_media::publish_media;
    use mini_store::MemoryBackend;

    fn person(seed: u8) -> (Controller, Controller) {
        let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    /// A ticket with a fresh random nonce and channel binding, as a real
    /// exchange would produce. Tests that need the *same* nonce twice reuse
    /// the returned value.
    fn fields(provider: &Did, bytes: u64, media: Vec<CompletedMedia>) -> TicketFields {
        TicketFields {
            provider: provider.clone(),
            service: Service::PublicSync,
            bytes_received: bytes,
            objects_received: 3,
            channel_binding: mini_crypto::random_32().unwrap(),
            nonce: mini_crypto::random_32().unwrap(),
            completed_media: media,
        }
    }

    #[test]
    fn ticket_round_trips_and_rejects_tampering() {
        let (host, _) = person(10);
        let (consumer, consumer_dev) = person(20);
        let f = fields(&host.did(), 5_000_000, Vec::new());
        let object = issue_ticket(&consumer.did(), &consumer_dev, &f, 1_000, 1).unwrap();
        let ticket = read_ticket(&object).unwrap();
        assert_eq!(ticket.consumer, consumer.did());
        assert_eq!(ticket.fields, f);
        // A post is not a ticket.
        let post = ObjectBuilder::new(ObjectType::POST)
            .payload(Payload::Public(b"hi".to_vec()))
            .sign(&consumer.did(), &consumer_dev)
            .unwrap();
        assert_eq!(read_ticket(&post), Err(TicketError::NotATicket));
        // Attributed media without a matching link is malformed.
        let bad = TicketFields {
            completed_media: vec![CompletedMedia {
                manifest: post.id().clone(),
                bytes: 10,
            }],
            ..f.clone()
        };
        let mut payload = encode_ticket(&bad).unwrap();
        payload[0] = TICKET_VERSION;
        let forged = ObjectBuilder::new(ObjectType::Custom(TICKET_TYPE.into()))
            .payload(Payload::Public(payload))
            .sign(&consumer.did(), &consumer_dev)
            .unwrap();
        assert_eq!(read_ticket(&forged), Err(TicketError::Malformed));
    }

    #[test]
    fn ledger_credits_host_and_creator_and_dedups() {
        let mut store = Store::new(MemoryBackend::new());
        let (host, host_dev) = person(10);
        let (creator, creator_dev) = person(30);
        let (consumer, consumer_dev) = person(20);
        let manifest = publish_media(
            &mut store,
            &creator.did(),
            &creator_dev,
            "video/mp4",
            &vec![1u8; 2_000_000],
            1,
            1,
        )
        .unwrap();
        let media = vec![CompletedMedia {
            manifest: manifest.id.clone(),
            bytes: 2_000_000,
        }];
        let f = fields(&host.did(), 3_000_000, media);
        let t1 = issue_ticket(&consumer.did(), &consumer_dev, &f, 10, 1).unwrap();
        store.insert(&t1).unwrap();
        // Same nonce again: a duplicate, ignored.
        let t1b = issue_ticket(&consumer.did(), &consumer_dev, &f, 11, 2).unwrap();
        store.insert(&t1b).unwrap();
        let f2 = fields(&host.did(), 1_000_000, Vec::new());
        store
            .insert(&issue_ticket(&consumer.did(), &consumer_dev, &f2, 12, 3).unwrap())
            .unwrap();

        let rate = Rate {
            micro_mini_per_mb: 100,
            creator_bps: 2500,
        };
        // Host: 3 MB + 1 MB = 400 micro, minus creator share of 2 MB (200 * 25% = 50).
        let host_ledger = Ledger::collect(&store, &host.did(), rate).unwrap();
        assert_eq!(host_ledger.duplicates, 1);
        assert_eq!(host_ledger.as_host.len(), 2);
        assert_eq!(host_ledger.bytes_served, 4_000_000);
        assert_eq!(host_ledger.host_micro, 350);
        assert_eq!(host_ledger.creator_micro, 0);
        // Creator sees its 50.
        let creator_ledger = Ledger::collect(&store, &creator.did(), rate).unwrap();
        assert_eq!(creator_ledger.creator_micro, 50);
        assert_eq!(creator_ledger.host_micro, 0);
        // Consumer owes 400 and issued two distinct tickets.
        let consumer_ledger = Ledger::collect(&store, &consumer.did(), rate).unwrap();
        assert_eq!(consumer_ledger.issued.len(), 2);
        assert_eq!(consumer_ledger.owed_micro, 400);
        assert_eq!(consumer_ledger.bytes_received, 4_000_000);

        // Redemption: only the host can build one over these tickets.
        let ids: Vec<ObjectId> = host_ledger
            .as_host
            .iter()
            .map(|entry| entry.ticket.id.clone())
            .collect();
        let request = build_redemption(&store, &host.did(), &host_dev, &ids, rate, 20, 5).unwrap();
        let decoded = read_redemption(&request).unwrap();
        assert_eq!(decoded.claimant, host.did());
        assert_eq!(decoded.bytes, 4_000_000);
        assert_eq!(decoded.micro_mini, 400);
        verify_redemption(&store, &decoded).unwrap();
        assert_eq!(
            build_redemption(&store, &creator.did(), &creator_dev, &ids, rate, 20, 5),
            Err(TicketError::NotTheNamedProvider)
        );
        // A request that claims more than the tickets add up to is refused.
        let inflated = RedemptionRequest {
            micro_mini: 401,
            ..decoded.clone()
        };
        assert_eq!(
            verify_redemption(&store, &inflated),
            Err(TicketError::Malformed)
        );
        let stranger = RedemptionRequest {
            claimant: creator.did(),
            ..decoded
        };
        assert_eq!(
            verify_redemption(&store, &stranger),
            Err(TicketError::NotTheNamedProvider)
        );
    }

    #[test]
    fn rate_prices_through_the_shared_convention() {
        let rate = Rate {
            micro_mini_per_mb: 7,
            creator_bps: 0,
        };
        assert_eq!(rate.price(0).unwrap(), 0);
        assert_eq!(rate.price(1).unwrap(), 7);
        assert_eq!(rate.price(2_500_000).unwrap(), 21);
        assert!(Rate {
            micro_mini_per_mb: 1,
            creator_bps: 10_001
        }
        .validate()
        .is_err());
    }
}
