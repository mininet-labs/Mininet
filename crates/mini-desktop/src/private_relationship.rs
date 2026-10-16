//! Durable pairwise identities and confidential target exchange.
//!
//! Existing CH1 + SessionAuthClaim authentication protects the exchange;
//! existing did-mini delegation/provenance protects published FOLLOW objects.
//! Expected peer identity is an explicit owner-selected trust input. Ordinary
//! connection cards do not authorize pairing. No traffic-anonymity claim.

use std::fs::{File, OpenOptions};
use std::path::PathBuf;

use did_mini::{verify_delegation, Capabilities, Controller, Did, Kel};
use mini_bearer::Bearer;
use mini_crypto::{SignatureSuite, SigningKey};
use mini_objects::{ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};
use mini_transport_security::{AuthenticatedConnection, TransportPurpose};
use zeroize::Zeroizing;

const VERSION: u8 = 1;
const MAX_RECORDS: usize = 32;
const MAX_KEL: usize = 64 * 1024;
const MAX_LINKAGE: usize = 48 * 1024;
const MAX_VAULT: usize = 8 * 1024 * 1024;
const MAX_FRAME: usize = 192 * 1024;
const AAD: &[u8] = b"mininet/private-relationship/exchange/v1";
const MAGIC: &[u8] = b"MINIRPX1";

type Result<T> = core::result::Result<T, String>;

/// Implementations must protect plaintext and atomically replace saved state.
/// The production implementation holds one exclusive lease for its lifetime.
pub(crate) trait Vault {
    fn load(&mut self) -> Result<Option<Zeroizing<Vec<u8>>>>;
    fn save(&mut self, bytes: &[u8]) -> Result<()>;
}

pub(crate) struct FileVault {
    path: PathBuf,
    lease: File,
}

impl FileVault {
    pub(crate) fn open(path: PathBuf) -> Result<Self> {
        let parent = path.parent().ok_or("relationship vault has no parent")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let lease = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("lock"))
            .map_err(|e| e.to_string())?;
        lease
            .try_lock()
            .map_err(|e| format!("relationship identity is already in use: {e}"))?;
        Ok(Self { path, lease })
    }
}

impl Drop for FileVault {
    fn drop(&mut self) {
        let _ = self.lease.unlock();
    }
}

impl Vault for FileVault {
    fn load(&mut self) -> Result<Option<Zeroizing<Vec<u8>>>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let size = std::fs::metadata(&self.path)
            .map_err(|e| e.to_string())?
            .len();
        if size > (MAX_VAULT + 64 * 1024) as u64 {
            return Err("relationship vault exceeds its limit".into());
        }
        let bytes = mini_windows_vault::load_user_data(&self.path).map_err(|e| e.to_string())?;
        Ok(Some(Zeroizing::new(bytes)))
    }

    fn save(&mut self, bytes: &[u8]) -> Result<()> {
        mini_windows_vault::save_user_data(&self.path, bytes).map_err(|e| e.to_string())
    }
}

#[derive(Debug)]
struct Record {
    peer: Did,
    root: Controller,
    device: Controller,
    target: Option<(Kel, Kel)>,
    last_binding: Option<[u8; 32]>,
    next_sequence: u64,
}

#[derive(Debug)]
struct State {
    owner: Did,
    records: Vec<Record>,
}

pub(crate) struct Relationships<V: Vault> {
    vault: V,
    state: State,
}

impl<V: Vault> Relationships<V> {
    pub(crate) fn open(mut vault: V, owner: Did) -> Result<Self> {
        let state = match vault.load()? {
            Some(bytes) => decode(&bytes)?,
            None => State {
                owner: owner.clone(),
                records: Vec::new(),
            },
        };
        if state.owner != owner {
            return Err("relationship vault belongs to another identity".into());
        }
        Ok(Self { vault, state })
    }

    fn commit(&mut self, staged: State) -> Result<()> {
        let bytes = encode(&staged)?;
        self.vault.save(&bytes)?;
        self.state = staged;
        Ok(())
    }

    fn staged(&self) -> Result<State> {
        decode(&encode(&self.state)?)
    }

    fn ensure(&mut self, owner: &Controller, peer: &Did) -> Result<usize> {
        if owner.did() != self.state.owner || peer == &self.state.owner {
            return Err("invalid relationship owner or counterpart".into());
        }
        if let Some(index) = self.state.records.iter().position(|r| &r.peer == peer) {
            return Ok(index);
        }
        if self.state.records.len() >= MAX_RECORDS {
            return Err("relationship capacity reached".into());
        }
        let mut root =
            mini_social::derive_relationship_pseudonym(owner, peer).map_err(|e| e.to_string())?;
        let current = Zeroizing::new(mini_crypto::random_32().map_err(|e| e.to_string())?);
        let next = Zeroizing::new(mini_crypto::random_32().map_err(|e| e.to_string())?);
        let device = Controller::incept_device_single_from_seeds(&root.did(), &current, &next)
            .map_err(|e| e.to_string())?;
        root.delegate_device(&device.did(), Capabilities::POST)
            .map_err(|e| e.to_string())?;
        let mut staged = self.staged()?;
        let index = staged.records.len();
        staged.records.push(Record {
            peer: peer.clone(),
            root,
            device,
            target: None,
            last_binding: None,
            next_sequence: 0,
        });
        // Persist the sole delegation history before any network/publication.
        self.commit(staged)?;
        Ok(index)
    }

    /// Owner-triggered pairing over an already authenticated encrypted channel.
    /// The expected DID must be pinned independently (for example verified QR);
    /// the connection itself never selects whom the owner intends to trust.
    pub(crate) fn pair<B: Bearer>(
        &mut self,
        connection: &mut AuthenticatedConnection<B>,
        owner: &Controller,
        owner_device: &Controller,
        expected_peer: &Did,
        initiator: bool,
        now_ms: u64,
    ) -> Result<Did> {
        if connection.peer().root != *expected_peer
            || connection.peer().purpose != TransportPurpose::PeerExchange
        {
            return Err("private pairing peer or purpose mismatch".into());
        }
        let caps =
            verify_delegation(&owner.kel(), &owner_device.kel()).map_err(|e| e.to_string())?;
        if owner.did() != self.state.owner || !caps.contains(Capabilities::POST) {
            return Err("private pairing signer is not authorized".into());
        }
        let index = self.ensure(owner, expected_peer)?;
        let binding = connection.channel_binding();
        if self.state.records[index].last_binding == Some(binding) {
            return Err("private pairing session was already consumed".into());
        }
        let outgoing = self.announcement(index, owner, owner_device, binding, now_ms)?;
        let incoming = if initiator {
            connection.send(&outgoing, AAD).map_err(|e| e.to_string())?;
            connection.recv(AAD).map_err(|e| e.to_string())?
        } else {
            connection.recv(AAD).map_err(|e| e.to_string())?
        };
        let target = self.accept(index, &incoming, binding, now_ms)?;
        if !initiator {
            // Acceptance is durable before acknowledging the peer.
            connection.send(&outgoing, AAD).map_err(|e| e.to_string())?;
        }
        Ok(target)
    }

    fn announcement(
        &self,
        index: usize,
        owner: &Controller,
        device: &Controller,
        binding: [u8; 32],
        now_ms: u64,
    ) -> Result<Zeroizing<Vec<u8>>> {
        let record = &self.state.records[index];
        let linkage = mini_social::create_relationship_linkage(
            &owner.kel(),
            device,
            &record.root.did(),
            &record.peer,
            now_ms,
        )
        .map_err(|e| e.to_string())?;
        let mut out = Zeroizing::new(Vec::new());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&binding);
        put(&mut out, &record.root.kel().to_bytes())?;
        put(&mut out, &record.device.kel().to_bytes())?;
        put(&mut out, &linkage)?;
        if out.len() > MAX_FRAME {
            return Err("private pairing frame exceeds its limit".into());
        }
        Ok(out)
    }

    fn accept(
        &mut self,
        index: usize,
        bytes: &[u8],
        binding: [u8; 32],
        now_ms: u64,
    ) -> Result<Did> {
        if bytes.len() > MAX_FRAME {
            return Err("private pairing frame exceeds its limit".into());
        }
        let mut reader = Reader { bytes, offset: 0 };
        if reader.take(MAGIC.len())? != MAGIC || reader.array()? != binding {
            return Err("private pairing channel binding mismatch".into());
        }
        let root = Kel::from_bytes(reader.field(MAX_KEL)?).map_err(|e| e.to_string())?;
        let device = Kel::from_bytes(reader.field(MAX_KEL)?).map_err(|e| e.to_string())?;
        let linkage =
            mini_social::verify_relationship_linkage(reader.field(MAX_LINKAGE)?, &self.state.owner)
                .map_err(|e| e.to_string())?;
        reader.finish()?;
        let record = &self.state.records[index];
        let caps = verify_delegation(&root, &device).map_err(|e| e.to_string())?;
        if record.last_binding == Some(binding) {
            return Err("private pairing session was already consumed".into());
        }
        if linkage.root != record.peer
            || linkage.pseudonym != root.did()
            || !caps.contains(Capabilities::POST)
            || linkage.issued_at_ms > now_ms.saturating_add(30_000)
            || now_ms.saturating_sub(linkage.issued_at_ms) > 300_000
        {
            return Err("private pairing identity, capability or freshness mismatch".into());
        }
        if let Some((old_root, old_device)) = &record.target {
            if !extends(old_root, &root) || !extends(old_device, &device) {
                return Err("private pairing target changed or KEL forked".into());
            }
        }
        let target = root.did();
        let mut staged = self.staged()?;
        staged.records[index].target = Some((root, device));
        staged.records[index].last_binding = Some(binding);
        self.commit(staged)?;
        Ok(target)
    }

    /// Publish only a previously authenticated, durably committed target.
    /// Sequence reservation is persisted before any public store write.
    pub(crate) fn follow<B: Backend>(
        &mut self,
        store: &mut Store<B>,
        target: &Did,
        follow: bool,
        now_ms: u64,
    ) -> Result<ObjectId> {
        let index = self.state.records.iter().position(|r| {
            r.target.as_ref().is_some_and(|(root, _)| root.did() == *target)
        }).ok_or("private follow requires durable pseudonym/device state and authenticated target pairing")?;
        let record = &self.state.records[index];
        let next = record
            .next_sequence
            .checked_add(1)
            .ok_or("private follow sequence exhausted")?;
        let root_kel = record.root.kel();
        let device_kel = record.device.kel();
        let mut payload = vec![u8::from(follow)];
        let text = target.as_str().as_bytes();
        payload.extend_from_slice(&(text.len() as u32).to_be_bytes());
        payload.extend_from_slice(text);
        let object = ObjectBuilder::new(ObjectType::FOLLOW)
            .timestamp_ms(now_ms)
            .sequence(record.next_sequence)
            .payload(Payload::Public(payload))
            .sign(&record.root.did(), &record.device)
            .map_err(|e| e.to_string())?;
        mini_objects::verify_provenance(&object, &root_kel, &device_kel)
            .map_err(|e| e.to_string())?;
        let carriers = [
            mini_sync::kel_carrier(&device_kel, &record.root.did(), &record.device),
            mini_sync::kel_carrier(&root_kel, &record.root.did(), &record.device),
        ];
        let carriers = carriers
            .into_iter()
            .collect::<core::result::Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let mut staged = self.staged()?;
        staged.records[index].next_sequence = next;
        self.commit(staged)?;
        // Only pseudonym KELs and FOLLOW leave the vault. No real-root linkage.
        for carrier in carriers {
            store.insert(&carrier).map_err(|e| e.to_string())?;
        }
        store.insert(&object).map_err(|e| e.to_string())?;
        Ok(object.id().clone())
    }
}

fn extends(old: &Kel, new: &Kel) -> bool {
    old.did() == new.did()
        && new.events().len() >= old.events().len()
        && new.events()[..old.events().len()] == *old.events()
}

fn encode(state: &State) -> Result<Zeroizing<Vec<u8>>> {
    let mut out = Zeroizing::new(vec![VERSION]);
    put(&mut out, state.owner.as_str().as_bytes())?;
    out.push(state.records.len() as u8);
    for record in &state.records {
        put(&mut out, record.peer.as_str().as_bytes())?;
        put_controller(&mut out, &record.root)?;
        put_controller(&mut out, &record.device)?;
        out.extend_from_slice(&record.next_sequence.to_be_bytes());
        out.push(u8::from(record.last_binding.is_some()));
        if let Some(binding) = record.last_binding {
            out.extend_from_slice(&binding);
        }
        out.push(u8::from(record.target.is_some()));
        if let Some((root, device)) = &record.target {
            put(&mut out, &root.to_bytes())?;
            put(&mut out, &device.to_bytes())?;
        }
    }
    if out.len() > MAX_VAULT {
        return Err("relationship vault exceeds its limit".into());
    }
    Ok(out)
}

fn put_controller(out: &mut Vec<u8>, controller: &Controller) -> Result<()> {
    let (current, next) = controller.export_current_and_next_keys_for_storage();
    if current.len() != 1
        || next.len() != 1
        || current[0].suite() != SignatureSuite::Ed25519
        || next[0].suite() != SignatureSuite::Ed25519
    {
        return Err("relationship storage supports single Ed25519 controllers only".into());
    }
    put(out, &controller.kel().to_bytes())?;
    for key in current.iter().chain(next.iter()) {
        let seed = Zeroizing::new(key.to_seed_bytes());
        out.extend_from_slice(seed.as_ref());
    }
    Ok(())
}

fn decode(bytes: &[u8]) -> Result<State> {
    if bytes.len() > MAX_VAULT {
        return Err("relationship vault exceeds its limit".into());
    }
    let mut reader = Reader { bytes, offset: 0 };
    if reader.take(1)? != [VERSION] {
        return Err("unsupported relationship vault".into());
    }
    let owner = reader.did()?;
    let count = reader.take(1)?[0] as usize;
    if count > MAX_RECORDS {
        return Err("relationship capacity reached".into());
    }
    let mut records: Vec<Record> = Vec::with_capacity(count);
    for _ in 0..count {
        let peer = reader.did()?;
        if peer == owner || records.iter().any(|r| r.peer == peer) {
            return Err("duplicate or invalid relationship peer".into());
        }
        let root = reader.controller()?;
        let device = reader.controller()?;
        let caps = verify_delegation(&root.kel(), &device.kel()).map_err(|e| e.to_string())?;
        if !caps.contains(Capabilities::POST) {
            return Err("invalid relationship signer".into());
        }
        let next_sequence =
            u64::from_be_bytes(reader.take(8)?.try_into().map_err(|_| "bad sequence")?);
        let last_binding = match reader.take(1)?[0] {
            0 => None,
            1 => Some(reader.array()?),
            _ => return Err("invalid replay state".into()),
        };
        let target = match reader.take(1)?[0] {
            0 => None,
            1 => {
                let r = Kel::from_bytes(reader.field(MAX_KEL)?).map_err(|e| e.to_string())?;
                let d = Kel::from_bytes(reader.field(MAX_KEL)?).map_err(|e| e.to_string())?;
                let caps = verify_delegation(&r, &d).map_err(|e| e.to_string())?;
                if !caps.contains(Capabilities::POST) || last_binding.is_none() {
                    return Err("invalid authenticated target".into());
                }
                Some((r, d))
            }
            _ => return Err("invalid target state".into()),
        };
        records.push(Record {
            peer,
            root,
            device,
            target,
            last_binding,
            next_sequence,
        });
    }
    reader.finish()?;
    Ok(State { owner, records })
}

fn put(out: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_KEL.max(MAX_LINKAGE) {
        return Err("relationship field too large".into());
    }
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or("relationship length overflow")?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or("truncated relationship state")?;
        self.offset = end;
        Ok(value)
    }
    fn field(&mut self, max: usize) -> Result<&'a [u8]> {
        let len = u32::from_be_bytes(self.take(4)?.try_into().map_err(|_| "bad length")?) as usize;
        if len > max {
            return Err("relationship field too large".into());
        }
        self.take(len)
    }
    fn array(&mut self) -> Result<[u8; 32]> {
        self.take(32)?.try_into().map_err(|_| "bad array".into())
    }
    fn did(&mut self) -> Result<Did> {
        let text = std::str::from_utf8(self.field(256)?).map_err(|e| e.to_string())?;
        Did::parse(text).map_err(|e| e.to_string())
    }
    fn controller(&mut self) -> Result<Controller> {
        let kel = Kel::from_bytes(self.field(MAX_KEL)?).map_err(|e| e.to_string())?;
        let current = Zeroizing::new(self.array()?);
        let next = Zeroizing::new(self.array()?);
        Controller::restore(
            &kel,
            vec![SigningKey::from_seed(&current)],
            vec![SigningKey::from_seed(&next)],
        )
        .map_err(|e| e.to_string())
    }
    fn finish(&self) -> Result<()> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err("trailing relationship bytes".into())
        }
    }
}

/// Kept here so the workspace never accepts an arbitrary unverified remote
/// object merely because it arrived through this relationship's network path.
#[cfg(test)]
fn verified_copy<B: Backend>(objects: &[mini_objects::Object], store: &mut Store<B>) -> Result<()> {
    let mut cache = mini_sync::KelCache::new();
    cache.hydrate_from_store(store).map_err(|e| e.to_string())?;
    for object in objects {
        if !matches!(
            mini_sync::Ingest::check(&mut cache, object),
            mini_sync::IngestOutcome::Accepted | mini_sync::IngestOutcome::AcceptedCarrier
        ) {
            return Err("verified relationship ingest rejected object".into());
        }
        store.insert(object).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_bearer::{Initiator, Responder, TcpBearer};
    use mini_crypto::AgreementSecretKey;
    use mini_transport_security::{
        authenticate_established_initiator, authenticate_established_responder,
        LocalSessionIdentity, PeerExpectation, ReplayCache,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct MemoryVault {
        bytes: Arc<Mutex<Option<Vec<u8>>>>,
        fail: Arc<AtomicBool>,
    }
    impl Vault for MemoryVault {
        fn load(&mut self) -> Result<Option<Zeroizing<Vec<u8>>>> {
            Ok(self.bytes.lock().unwrap().clone().map(Zeroizing::new))
        }
        fn save(&mut self, bytes: &[u8]) -> Result<()> {
            if self.fail.load(Ordering::SeqCst) {
                return Err("injected save failure".into());
            }
            *self.bytes.lock().unwrap() = Some(bytes.to_vec());
            Ok(())
        }
    }
    fn identity(seed: u8) -> (Controller, Controller) {
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

    #[test]
    fn restart_preserves_one_delegation_and_rejects_replay_tamper_and_wrong_identity() {
        let (alice, ad) = identity(10);
        let (bob, bd) = identity(30);
        let (carol, _) = identity(50);
        let av = MemoryVault::default();
        let bv = MemoryVault::default();
        let mut a = Relationships::open(av.clone(), alice.did()).unwrap();
        let mut b = Relationships::open(bv.clone(), bob.did()).unwrap();
        let ai = a.ensure(&alice, &bob.did()).unwrap();
        let bi = b.ensure(&bob, &alice.did()).unwrap();
        let old_root = a.state.records[ai].root.kel().to_bytes();
        let old_device = a.state.records[ai].device.did();
        let frame = a.announcement(ai, &alice, &ad, [1; 32], 1000).unwrap();
        let before = encode(&b.state).unwrap();
        assert!(b.accept(bi, &frame, [2; 32], 1000).is_err());
        let mut tampered = frame.to_vec();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(b.accept(bi, &tampered, [1; 32], 1000).is_err());
        assert_eq!(encode(&b.state).unwrap().as_slice(), before.as_slice());
        let mut wrong = Relationships::open(MemoryVault::default(), carol.did()).unwrap();
        let wi = wrong.ensure(&carol, &alice.did()).unwrap();
        assert!(wrong.accept(wi, &frame, [1; 32], 1000).is_err());
        b.accept(bi, &frame, [1; 32], 1000).unwrap();
        drop(b);
        let mut b = Relationships::open(bv, bob.did()).unwrap();
        assert!(b.accept(bi, &frame, [1; 32], 1000).is_err());
        drop(a);
        let mut a = Relationships::open(av, alice.did()).unwrap();
        let ai = a.ensure(&alice, &bob.did()).unwrap();
        assert_eq!(a.state.records[ai].root.kel().to_bytes(), old_root);
        assert_eq!(a.state.records[ai].device.did(), old_device);
        assert_eq!(a.state.records[ai].root.kel().delegated_devices().len(), 1);
        assert!(Relationships::open(a.vault, carol.did()).is_err());
        let _ = bd;
    }

    #[test]
    fn failed_persistence_never_commits_target_sequence_or_public_objects() {
        let (alice, ad) = identity(10);
        let (bob, bd) = identity(30);
        let av = MemoryVault::default();
        let bv = MemoryVault::default();
        let mut a = Relationships::open(av.clone(), alice.did()).unwrap();
        av.fail.store(true, Ordering::SeqCst);
        assert!(a.ensure(&alice, &bob.did()).is_err());
        assert!(a.state.records.is_empty());
        av.fail.store(false, Ordering::SeqCst);
        let ai = a.ensure(&alice, &bob.did()).unwrap();
        let mut b = Relationships::open(bv, bob.did()).unwrap();
        let bi = b.ensure(&bob, &alice.did()).unwrap();
        let frame = b.announcement(bi, &bob, &bd, [1; 32], 1000).unwrap();
        av.fail.store(true, Ordering::SeqCst);
        assert!(a.accept(ai, &frame, [1; 32], 1000).is_err());
        assert!(a.state.records[ai].target.is_none());
        av.fail.store(false, Ordering::SeqCst);
        let target = a.accept(ai, &frame, [1; 32], 1000).unwrap();
        av.fail.store(true, Ordering::SeqCst);
        let mut store = Store::new(mini_store::MemoryBackend::new());
        assert!(a.follow(&mut store, &target, true, 1000).is_err());
        assert_eq!(a.state.records[ai].next_sequence, 0);
        assert!(store.by_type(&ObjectType::FOLLOW).unwrap().is_empty());
        assert!(store
            .by_type(&ObjectType::Custom(mini_sync::KEL_CARRIER.into()))
            .unwrap()
            .is_empty());
        let _ = ad;
    }

    #[test]
    fn authenticated_tcp_pairing_then_two_store_verified_ingestion_and_restart() {
        let (alice, ad) = identity(10);
        let (bob, bd) = identity(30);
        let alice_root = alice.kel();
        let alice_device = ad.kel();
        let bob_root = bob.kel();
        let bob_device = bd.kel();
        let expected_bob = bob.did();
        let av = MemoryVault::default();
        let bv = MemoryVault::default();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bearer = TcpBearer::from_stream(stream).unwrap();
            let hello = bearer.recv().unwrap();
            let (channel, response) = Responder::respond(&hello).unwrap();
            bearer.send(&response).unwrap();
            let mut pins = did_mini::FreshnessPins::new();
            let mut replay = ReplayCache::new(32).unwrap();
            let routing = AgreementSecretKey::from_seed(&[90; 32]).public_key();
            let mut connection = authenticate_established_responder(
                bearer,
                channel,
                LocalSessionIdentity::new(bob.did(), &bd, routing),
                TransportPurpose::PeerExchange,
                1000,
                2000,
                1500,
                PeerExpectation::identity(&alice_root, &alice_device),
                &mut pins,
                &mut replay,
            )
            .unwrap();
            let mut state = Relationships::open(bv.clone(), bob.did()).unwrap();
            let target = state
                .pair(&mut connection, &bob, &bd, &alice_root.did(), false, 1500)
                .unwrap();
            let mut store = Store::new(mini_store::MemoryBackend::new());
            state.follow(&mut store, &target, true, 1500).unwrap();
            (store, state.state.records[0].root.did(), bv, bob.did())
        });
        let stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let (initiator, hello) = Initiator::start().unwrap();
        bearer.send(&hello).unwrap();
        let response = bearer.recv().unwrap();
        let channel = initiator.finish(&response).unwrap();
        let mut pins = did_mini::FreshnessPins::new();
        let mut replay = ReplayCache::new(32).unwrap();
        let routing = AgreementSecretKey::from_seed(&[91; 32]).public_key();
        let mut connection = authenticate_established_initiator(
            bearer,
            channel,
            LocalSessionIdentity::new(alice.did(), &ad, routing),
            TransportPurpose::PeerExchange,
            1000,
            2000,
            1500,
            PeerExpectation::identity(&bob_root, &bob_device),
            &mut pins,
            &mut replay,
        )
        .unwrap();
        let mut a = Relationships::open(av.clone(), alice.did()).unwrap();
        // An ordinary or substituted target cannot change the authenticated peer.
        assert!(a
            .pair(&mut connection, &alice, &ad, &alice.did(), true, 1500)
            .is_err());
        let target = a
            .pair(&mut connection, &alice, &ad, &expected_bob, true, 1500)
            .unwrap();
        let mut astore = Store::new(mini_store::MemoryBackend::new());
        a.follow(&mut astore, &target, true, 1500).unwrap();
        let (mut bstore, b_pseudonym, bv, bob_did) = server.join().unwrap();
        assert_eq!(target, b_pseudonym);
        let ap = a.state.records[0].root.did();
        let objects = [
            mini_sync::kel_carrier(
                &a.state.records[0].device.kel(),
                &ap,
                &a.state.records[0].device,
            )
            .unwrap(),
            mini_sync::kel_carrier(
                &a.state.records[0].root.kel(),
                &ap,
                &a.state.records[0].device,
            )
            .unwrap(),
            astore
                .get(&astore.by_type(&ObjectType::FOLLOW).unwrap()[0])
                .unwrap(),
        ];
        verified_copy(&objects, &mut bstore).unwrap();
        assert_eq!(
            mini_social::following(&bstore, &ap).unwrap(),
            vec![b_pseudonym.clone()]
        );
        let mut cache = mini_sync::KelCache::new();
        cache.hydrate_from_store(&bstore).unwrap();
        assert_eq!(
            mini_sync::Ingest::check(&mut cache, &objects[2]),
            mini_sync::IngestOutcome::Accepted
        );
        drop(a);
        let mut a = Relationships::open(av, alice.did()).unwrap();
        a.follow(&mut astore, &b_pseudonym, false, 1600).unwrap();
        assert_eq!(a.state.records[0].next_sequence, 2);
        assert!(mini_social::following(&astore, &ap).unwrap().is_empty());
        let b = Relationships::open(bv, bob_did).unwrap();
        assert_eq!(b.state.records[0].target.as_ref().unwrap().0.did(), ap);
    }

    #[test]
    fn exclusive_lease_prevents_concurrent_forked_identity_creation() {
        let unique = format!(
            "mininet-relationship-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let directory = std::env::temp_dir().join(unique);
        let path = directory.join("relationships.dpapi");
        let first = FileVault::open(path.clone()).unwrap();
        assert!(FileVault::open(path.clone()).is_err());
        drop(first);
        assert!(FileVault::open(path).is_ok());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
