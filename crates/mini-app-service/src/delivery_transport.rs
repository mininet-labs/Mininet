//! One explicitly configured peer, one exact public post, one durable receipt.
//! No discovery, store enumeration, identity exchange or fallback protocol.
use crate::{delivery_interrupt::DeliveryInterrupt, outbox::DeliveryFailure};
use did_mini::{FreshnessPins, Kel};
use mini_bearer::{Bearer, BearerError, Initiator, Responder};
use mini_objects::{Object, ObjectType, Payload};
use mini_store::{FsBackend, Store};
use mini_transport_security::{
    authenticate_established_initiator, authenticate_established_responder,
    AuthenticatedConnection, LocalSessionIdentity, PeerExpectation, ReplayCache, TransportPurpose,
};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

const MAX_OBJECT: usize = 128 * 1024;
const MAX_FRAME: usize = 160 * 1024;
const MAX_SESSION: usize = 512 * 1024;
const DEADLINE: Duration = Duration::from_secs(5);
const AAD: &[u8] = b"MININET/APP-PUBLIC-DELIVERY/1";
const POST: &[u8] = b"MINIPOST1";
const ACK: &[u8] = b"MINIACK1";

/// The deadline is absolute across handshake, authentication, payload and ack;
/// recomputing socket timeouts per syscall also bounds a slow-dribbling peer.
#[derive(Debug)]
struct BoundedBearer {
    stream: TcpStream,
    deadline: Instant,
    remaining: usize,
    interrupt: DeliveryInterrupt,
}
impl BoundedBearer {
    fn new(stream: TcpStream, interrupt: &DeliveryInterrupt) -> Self {
        Self {
            stream,
            deadline: Instant::now() + DEADLINE,
            remaining: MAX_SESSION,
            interrupt: interrupt.clone(),
        }
    }
    fn timeout(&self) -> mini_bearer::Result<Duration> {
        if self.interrupt.is_stopped() {
            return Err(BearerError::Io("owner stopped delivery".into()));
        }
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|duration| !duration.is_zero())
            .map(|duration| duration.min(Duration::from_millis(100)))
            .ok_or_else(|| BearerError::Io("delivery session deadline exceeded".into()))
    }
    fn reserve(&mut self, count: usize) -> mini_bearer::Result<()> {
        self.timeout()?;
        self.remaining = self
            .remaining
            .checked_sub(count)
            .ok_or_else(|| BearerError::Io("delivery session byte budget exceeded".into()))?;
        Ok(())
    }
    fn read_exact(&mut self, bytes: &mut [u8]) -> mini_bearer::Result<()> {
        let mut offset = 0;
        while offset < bytes.len() {
            self.stream.set_read_timeout(Some(self.timeout()?))?;
            match self.stream.read(&mut bytes[offset..]) {
                Ok(0) => return Err(BearerError::Closed),
                Ok(count) => offset += count,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::Interrupted
                            | std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    fn write_all(&mut self, bytes: &[u8]) -> mini_bearer::Result<()> {
        let mut offset = 0;
        while offset < bytes.len() {
            self.stream.set_write_timeout(Some(self.timeout()?))?;
            match self.stream.write(&bytes[offset..]) {
                Ok(0) => return Err(BearerError::Closed),
                Ok(count) => offset += count,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::Interrupted
                            | std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}
impl Bearer for BoundedBearer {
    fn send(&mut self, frame: &[u8]) -> mini_bearer::Result<()> {
        if frame.len() > MAX_FRAME {
            return Err(BearerError::FrameTooLarge {
                max: MAX_FRAME,
                got: frame.len(),
            });
        }
        self.reserve(4 + frame.len())?;
        self.write_all(&(frame.len() as u32).to_be_bytes())?;
        self.write_all(frame)
    }
    fn recv(&mut self) -> mini_bearer::Result<Vec<u8>> {
        self.reserve(4)?;
        let mut prefix = [0; 4];
        self.read_exact(&mut prefix)?;
        let length = u32::from_be_bytes(prefix) as usize;
        if length > MAX_FRAME {
            return Err(BearerError::FrameTooLarge {
                max: MAX_FRAME,
                got: length,
            });
        }
        self.reserve(length)?;
        let mut bytes = vec![0; length];
        self.read_exact(&mut bytes)?;
        Ok(bytes)
    }
    fn try_recv(&mut self) -> mini_bearer::Result<Option<Vec<u8>>> {
        Err(BearerError::Io(
            "nonblocking receive unsupported for bounded delivery".into(),
        ))
    }
    fn max_frame_bytes(&self) -> Option<usize> {
        Some(MAX_FRAME)
    }
}
fn rejected<T>(_: T) -> DeliveryFailure {
    DeliveryFailure::Rejected
}
fn unavailable<T>(_: T) -> DeliveryFailure {
    DeliveryFailure::Unavailable
}
fn validate_object(bytes: &[u8]) -> Result<Object, DeliveryFailure> {
    if bytes.is_empty() || bytes.len() > MAX_OBJECT {
        return Err(DeliveryFailure::Rejected);
    }
    let object = Object::from_bytes(bytes).map_err(rejected)?;
    if object.to_bytes() != bytes
        || object.object_type != ObjectType::POST
        || !object.links.is_empty()
        || !matches!(&object.payload, Payload::Public(_))
    {
        return Err(DeliveryFailure::Rejected);
    }
    object.verify_integrity(object.id()).map_err(rejected)?;
    mini_social::decode_post(&object).map_err(rejected)?;
    Ok(object)
}
fn establish(
    stream: TcpStream,
    local: LocalSessionIdentity<'_>,
    root: &Kel,
    device: &Kel,
    now_ms: u64,
    initiator: bool,
    interrupt: &DeliveryInterrupt,
) -> Result<AuthenticatedConnection<BoundedBearer>, DeliveryFailure> {
    check_stop(interrupt)?;
    let mut freshness = FreshnessPins::new();
    freshness.check_and_pin(root).map_err(rejected)?;
    freshness.check_and_pin(device).map_err(rejected)?;
    let mut replay = ReplayCache::new(4).map_err(rejected)?;
    let mut bearer = BoundedBearer::new(stream, interrupt);
    check_stop(interrupt)?;
    let channel = if initiator {
        let (initiator, hello) = Initiator::start().map_err(unavailable)?;
        bearer.send(&hello).map_err(unavailable)?;
        initiator
            .finish(&bearer.recv().map_err(unavailable)?)
            .map_err(rejected)?
    } else {
        let hello = bearer.recv().map_err(unavailable)?;
        let (channel, response) = Responder::respond(&hello).map_err(rejected)?;
        bearer.send(&response).map_err(unavailable)?;
        channel
    };
    check_stop(interrupt)?;
    let authenticate = if initiator {
        authenticate_established_initiator
    } else {
        authenticate_established_responder
    };
    authenticate(
        bearer,
        channel,
        local,
        TransportPurpose::StateSync,
        now_ms,
        now_ms.saturating_add(5_000),
        now_ms,
        PeerExpectation::identity(root, device),
        &mut freshness,
        &mut replay,
    )
    .map_err(rejected)
}

fn check_stop(interrupt: &DeliveryInterrupt) -> Result<(), DeliveryFailure> {
    if interrupt.is_stopped() {
        Err(DeliveryFailure::Interrupted)
    } else {
        Ok(())
    }
}
fn stopped_result<T>(
    result: Result<T, DeliveryFailure>,
    interrupt: &DeliveryInterrupt,
) -> Result<T, DeliveryFailure> {
    match result {
        Err(_) if interrupt.is_stopped() => Err(DeliveryFailure::Interrupted),
        other => other,
    }
}
pub(crate) fn send(
    stream: TcpStream,
    local: LocalSessionIdentity<'_>,
    peer_root: &Kel,
    peer_device: &Kel,
    object_bytes: &[u8],
    now_ms: u64,
    interrupt: &DeliveryInterrupt,
) -> Result<(), DeliveryFailure> {
    stopped_result(
        send_inner(
            stream,
            local,
            peer_root,
            peer_device,
            object_bytes,
            now_ms,
            interrupt,
        ),
        interrupt,
    )
}
pub(crate) fn receive(
    stream: TcpStream,
    local: LocalSessionIdentity<'_>,
    peer_root: &Kel,
    peer_device: &Kel,
    store: &mut Store<FsBackend>,
    now_ms: u64,
    interrupt: &DeliveryInterrupt,
) -> Result<String, DeliveryFailure> {
    stopped_result(
        receive_inner(
            stream,
            local,
            peer_root,
            peer_device,
            store,
            now_ms,
            interrupt,
        ),
        interrupt,
    )
}
fn send_inner(
    stream: TcpStream,
    local: LocalSessionIdentity<'_>,
    peer_root: &Kel,
    peer_device: &Kel,
    object_bytes: &[u8],
    now_ms: u64,
    interrupt: &DeliveryInterrupt,
) -> Result<(), DeliveryFailure> {
    check_stop(interrupt)?;
    let object = validate_object(object_bytes)?;
    if object.author_human != local.root || object.author_device != local.device.did() {
        return Err(DeliveryFailure::Rejected);
    }
    let mut connection = establish(
        stream,
        local,
        peer_root,
        peer_device,
        now_ms,
        true,
        interrupt,
    )?;
    check_stop(interrupt)?;
    let mut message = Vec::with_capacity(POST.len() + object_bytes.len());
    message.extend_from_slice(POST);
    message.extend_from_slice(object_bytes);
    connection.send(&message, AAD).map_err(unavailable)?;
    let receipt = connection.recv(AAD).map_err(unavailable)?;
    if receipt.strip_prefix(ACK) != Some(object.id().as_str().as_bytes()) {
        return Err(DeliveryFailure::Rejected);
    }
    Ok(())
}

fn receive_inner(
    stream: TcpStream,
    local: LocalSessionIdentity<'_>,
    peer_root: &Kel,
    peer_device: &Kel,
    store: &mut Store<FsBackend>,
    now_ms: u64,
    interrupt: &DeliveryInterrupt,
) -> Result<String, DeliveryFailure> {
    check_stop(interrupt)?;
    let mut connection = establish(
        stream,
        local,
        peer_root,
        peer_device,
        now_ms,
        false,
        interrupt,
    )?;
    let message = connection.recv(AAD).map_err(unavailable)?;
    let bytes = message
        .strip_prefix(POST)
        .ok_or(DeliveryFailure::Rejected)?;
    check_stop(interrupt)?;
    let id = ingest_post(peer_root, peer_device, store, bytes, interrupt)?;
    check_stop(interrupt)?;
    let mut receipt = ACK.to_vec();
    receipt.extend_from_slice(id.as_bytes());
    connection.send(&receipt, AAD).map_err(unavailable)?;
    Ok(id)
}

fn ingest_post(
    peer_root: &Kel,
    peer_device: &Kel,
    store: &mut Store<FsBackend>,
    bytes: &[u8],
    interrupt: &DeliveryInterrupt,
) -> Result<String, DeliveryFailure> {
    check_stop(interrupt)?;
    let object = validate_object(bytes)?;
    if object.author_human != peer_root.did() || object.author_device != peer_device.did() {
        return Err(DeliveryFailure::Rejected);
    }
    mini_objects::verify_provenance(&object, peer_root, peer_device).map_err(rejected)?;
    check_stop(interrupt)?;
    store.insert(&object).map_err(unavailable)?;
    if store.get(object.id()).map_err(unavailable)?.to_bytes() != bytes {
        return Err(DeliveryFailure::Rejected);
    }
    Ok(object.id().as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::{Capabilities, Controller};
    use mini_crypto::AgreementSecretKey;
    use mini_objects::ObjectBuilder;
    use std::net::TcpListener;
    use std::thread;
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
    fn local<'a>(root: &Controller, device: &'a Controller) -> LocalSessionIdentity<'a> {
        LocalSessionIdentity::new(
            root.did(),
            device,
            AgreementSecretKey::from_seed(&[99; 32]).public_key(),
        )
    }
    fn post(root: &Controller, device: &Controller) -> Vec<u8> {
        mini_social::build_post(&root.did(), device, "one public post", 1000, 1)
            .unwrap()
            .to_bytes()
    }
    #[test]
    fn lost_and_forged_acknowledgements_never_deliver_and_retry_is_one_exact_object() {
        let (sender, sender_device) = identity(10);
        let (receiver, receiver_device) = identity(30);
        let bytes = post(&sender, &sender_device);
        let expected_id = Object::from_bytes(&bytes).unwrap().id().clone();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let sender_root_kel = sender.kel();
        let sender_device_kel = sender_device.kel();
        let receiver_root_kel = receiver.kel();
        let receiver_device_kel = receiver_device.kel();
        let path = std::env::temp_dir().join(format!(
            "mini-delivery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cleanup = path.clone();
        let expected = bytes.clone();
        let worker = thread::spawn(move || {
            let mut store = Store::new(FsBackend::open(&path).unwrap());
            for round in 0..4 {
                let (stream, _) = listener.accept().unwrap();
                if round < 2 {
                    let mut connection = establish(
                        stream,
                        local(&receiver, &receiver_device),
                        &sender_root_kel,
                        &sender_device_kel,
                        1000,
                        false,
                        &DeliveryInterrupt::default(),
                    )
                    .unwrap();
                    let message = connection.recv(AAD).unwrap();
                    ingest_post(
                        &sender_root_kel,
                        &sender_device_kel,
                        &mut store,
                        message.strip_prefix(POST).unwrap(),
                        &DeliveryInterrupt::default(),
                    )
                    .unwrap();
                    if round == 1 {
                        let mut wrong = ACK.to_vec();
                        wrong.extend_from_slice(b"wrong-object-id");
                        connection.send(&wrong, AAD).unwrap();
                    }
                    // Round zero deliberately loses the ACK after durable insert.
                    drop(connection);
                    assert_eq!(store.all_ids().unwrap(), vec![expected_id.clone()]);
                    continue;
                }
                assert_eq!(
                    receive(
                        stream,
                        local(&receiver, &receiver_device),
                        &sender_root_kel,
                        &sender_device_kel,
                        &mut store,
                        1000,
                        &DeliveryInterrupt::default()
                    )
                    .unwrap(),
                    expected_id.as_str()
                );
                assert_eq!(store.get(&expected_id).unwrap().to_bytes(), expected);
                assert_eq!(store.all_ids().unwrap(), vec![expected_id.clone()]);
            }
        });
        for round in 0..4 {
            let result = send(
                TcpStream::connect(address).unwrap(),
                local(&sender, &sender_device),
                &receiver_root_kel,
                &receiver_device_kel,
                &bytes,
                1000,
                &DeliveryInterrupt::default(),
            );
            if round < 2 {
                assert!(result.is_err());
            } else {
                result.unwrap();
            }
        }
        worker.join().unwrap();
        std::fs::remove_dir_all(cleanup).unwrap();
    }
    #[test]
    fn wrong_recipient_cannot_acknowledge_and_invalid_object_is_refused() {
        let (sender, sender_device) = identity(11);
        let (receiver, receiver_device) = identity(31);
        let (wrong, wrong_device) = identity(51);
        let bytes = post(&sender, &sender_device);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let root = sender.kel();
        let device = sender_device.kel();
        let worker = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            assert!(establish(
                stream,
                local(&receiver, &receiver_device),
                &root,
                &device,
                1000,
                false,
                &DeliveryInterrupt::default()
            )
            .is_err());
        });
        assert!(send(
            TcpStream::connect(address).unwrap(),
            local(&sender, &sender_device),
            &wrong.kel(),
            &wrong_device.kel(),
            &bytes,
            1000,
            &DeliveryInterrupt::default()
        )
        .is_err());
        worker.join().unwrap();
        assert!(validate_object(&vec![0; MAX_OBJECT + 1]).is_err());
        let encrypted = ObjectBuilder::new(ObjectType::POST)
            .payload(Payload::Encrypted(vec![1]))
            .sign(&sender.did(), &sender_device)
            .unwrap()
            .to_bytes();
        assert!(validate_object(&encrypted).is_err());
        let malformed_text = ObjectBuilder::new(ObjectType::POST)
            .payload(Payload::Public(vec![0xff]))
            .sign(&sender.did(), &sender_device)
            .unwrap()
            .to_bytes();
        assert!(validate_object(&malformed_text).is_err());
    }
    #[test]
    fn late_stop_preserves_verified_success_and_classifies_only_failed_work() {
        let interrupt = DeliveryInterrupt::default();
        let _guard = interrupt.stop();
        assert_eq!(
            stopped_result(
                Ok::<_, DeliveryFailure>("verified exact acknowledgement"),
                &interrupt
            ),
            Ok("verified exact acknowledgement")
        );
        assert_eq!(
            stopped_result(Err::<(), _>(DeliveryFailure::Unavailable), &interrupt),
            Err(DeliveryFailure::Interrupted)
        );
        drop(_guard);
        assert_eq!(
            stopped_result(Err::<(), _>(DeliveryFailure::Rejected), &interrupt),
            Err(DeliveryFailure::Rejected)
        );
    }
    #[test]
    fn established_partial_frame_stops_promptly_without_inserting_or_acknowledging() {
        let (sender, sender_device) = identity(12);
        let (receiver, receiver_device) = identity(32);
        let sender_root = sender.kel();
        let sender_kel = sender_device.kel();
        let receiver_root = receiver.kel();
        let receiver_kel = receiver_device.kel();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let interrupt = DeliveryInterrupt::default();
        let worker_interrupt = interrupt.clone();
        let path = std::env::temp_dir().join(format!(
            "mini-delivery-stop-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cleanup = path.clone();
        let worker = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut store = Store::new(FsBackend::open(&path).unwrap());
            let result = receive(
                stream,
                local(&receiver, &receiver_device),
                &sender_root,
                &sender_kel,
                &mut store,
                1000,
                &worker_interrupt,
            );
            assert!(store.all_ids().unwrap().is_empty());
            result
        });
        let stream = TcpStream::connect(address).unwrap();
        let mut raw = stream.try_clone().unwrap();
        let mut connection = establish(
            stream,
            local(&sender, &sender_device),
            &receiver_root,
            &receiver_kel,
            1000,
            true,
            &DeliveryInterrupt::default(),
        )
        .unwrap();
        // Authentication completed; leave the next encrypted frame incomplete.
        raw.write_all(&64u32.to_be_bytes()).unwrap();
        raw.write_all(&[1, 2]).unwrap();
        thread::sleep(Duration::from_millis(30));
        let start = Instant::now();
        let guard = interrupt.stop();
        assert_eq!(worker.join().unwrap(), Err(DeliveryFailure::Interrupted));
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(connection.recv(AAD).is_err());
        drop(guard);
        std::fs::remove_dir_all(cleanup).unwrap();
    }
    #[test]
    fn held_stop_prevents_any_outbound_frame_and_allows_no_authorization() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (stream, _) = listener.accept().unwrap();
        let interrupt = DeliveryInterrupt::default();
        let mut bearer = BoundedBearer::new(stream, &interrupt);
        let guard = interrupt.stop();
        assert!(bearer.send(b"application frame").is_err());
        peer.set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let mut byte = [0];
        assert!(peer.read(&mut byte).is_err());
        drop(guard);
        bearer.send(b"permitted only by caller policy").unwrap();
        assert_eq!(peer.read(&mut byte).unwrap(), 1);
    }
    #[test]
    fn bounded_bearer_rejects_length_budget_and_absolute_dribble_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let mut peer = TcpStream::connect(address).unwrap();
        let (stream, _) = listener.accept().unwrap();
        let mut bearer = BoundedBearer::new(stream, &DeliveryInterrupt::default());
        peer.write_all(&((MAX_FRAME + 1) as u32).to_be_bytes())
            .unwrap();
        assert!(matches!(
            bearer.recv(),
            Err(BearerError::FrameTooLarge { .. })
        ));
        bearer.remaining = 1;
        assert!(bearer.send(&[1]).is_err());
        bearer.remaining = MAX_SESSION;
        bearer.deadline = Instant::now() + Duration::from_millis(70);
        let worker = thread::spawn(move || {
            for byte in 10u32.to_be_bytes() {
                if peer.write_all(&[byte]).is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(30));
            }
        });
        let start = Instant::now();
        assert!(bearer.recv().is_err());
        assert!(start.elapsed() < Duration::from_millis(250));
        worker.join().unwrap();
    }
}
