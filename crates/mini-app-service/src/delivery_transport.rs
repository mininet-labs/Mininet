//! One explicitly configured peer, one exact public post, one durable receipt.
//! No discovery, store enumeration, identity exchange or fallback protocol.
use crate::outbox::DeliveryFailure;
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
}
impl BoundedBearer {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            deadline: Instant::now() + DEADLINE,
            remaining: MAX_SESSION,
        }
    }
    fn timeout(&self) -> mini_bearer::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| BearerError::Io("delivery session deadline exceeded".into()))
    }
    fn reserve(&mut self, count: usize) -> mini_bearer::Result<()> {
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
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
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
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
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
    Ok(object)
}
fn establish(
    stream: TcpStream,
    local: LocalSessionIdentity<'_>,
    root: &Kel,
    device: &Kel,
    now_ms: u64,
    initiator: bool,
) -> Result<AuthenticatedConnection<BoundedBearer>, DeliveryFailure> {
    let mut freshness = FreshnessPins::new();
    freshness.check_and_pin(root).map_err(rejected)?;
    freshness.check_and_pin(device).map_err(rejected)?;
    let mut replay = ReplayCache::new(4).map_err(rejected)?;
    let mut bearer = BoundedBearer::new(stream);
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

pub(crate) fn send(
    stream: TcpStream,
    local: LocalSessionIdentity<'_>,
    peer_root: &Kel,
    peer_device: &Kel,
    object_bytes: &[u8],
    now_ms: u64,
) -> Result<(), DeliveryFailure> {
    let object = validate_object(object_bytes)?;
    if object.author_human != local.root || object.author_device != local.device.did() {
        return Err(DeliveryFailure::Rejected);
    }
    let mut connection = establish(stream, local, peer_root, peer_device, now_ms, true)?;
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

pub(crate) fn receive(
    stream: TcpStream,
    local: LocalSessionIdentity<'_>,
    peer_root: &Kel,
    peer_device: &Kel,
    store: &mut Store<FsBackend>,
    now_ms: u64,
) -> Result<String, DeliveryFailure> {
    let mut connection = establish(stream, local, peer_root, peer_device, now_ms, false)?;
    let message = connection.recv(AAD).map_err(unavailable)?;
    let bytes = message
        .strip_prefix(POST)
        .ok_or(DeliveryFailure::Rejected)?;
    let object = validate_object(bytes)?;
    if object.author_human != peer_root.did() || object.author_device != peer_device.did() {
        return Err(DeliveryFailure::Rejected);
    }
    mini_objects::verify_provenance(&object, peer_root, peer_device).map_err(rejected)?;
    store.insert(&object).map_err(unavailable)?;
    if store.get(object.id()).map_err(unavailable)?.to_bytes() != bytes {
        return Err(DeliveryFailure::Rejected);
    }
    let mut receipt = ACK.to_vec();
    receipt.extend_from_slice(object.id().as_str().as_bytes());
    connection.send(&receipt, AAD).map_err(unavailable)?;
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
        ObjectBuilder::new(ObjectType::POST)
            .sequence(1)
            .payload(Payload::Public(b"one public post".to_vec()))
            .sign(&root.did(), device)
            .unwrap()
            .to_bytes()
    }
    #[test]
    fn authenticated_exact_post_retries_are_one_content_addressed_object() {
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
            for _ in 0..2 {
                let (stream, _) = listener.accept().unwrap();
                assert_eq!(
                    receive(
                        stream,
                        local(&receiver, &receiver_device),
                        &sender_root_kel,
                        &sender_device_kel,
                        &mut store,
                        1000
                    )
                    .unwrap(),
                    expected_id.as_str()
                );
                assert_eq!(store.get(&expected_id).unwrap().to_bytes(), expected);
            }
        });
        for _ in 0..2 {
            send(
                TcpStream::connect(address).unwrap(),
                local(&sender, &sender_device),
                &receiver_root_kel,
                &receiver_device_kel,
                &bytes,
                1000,
            )
            .unwrap();
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
                false
            )
            .is_err());
        });
        assert!(send(
            TcpStream::connect(address).unwrap(),
            local(&sender, &sender_device),
            &wrong.kel(),
            &wrong_device.kel(),
            &bytes,
            1000
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
    }
    #[test]
    fn bounded_bearer_rejects_length_budget_and_absolute_dribble_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let mut peer = TcpStream::connect(address).unwrap();
        let (stream, _) = listener.accept().unwrap();
        let mut bearer = BoundedBearer::new(stream);
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
