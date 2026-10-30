//! One service-owned, bounded public delivery scheduler and networking grant.
use crate::{
    delivery_peers::Peer,
    delivery_transport, delivery_view,
    outbox::{DeliveryFailure, DeliveryState},
    Core, IdentityVault,
};
use mini_app_protocol::{
    Command, DeliveryRuntimeView, DeliveryView, ErrorCode, Request, ServiceError, ServiceEventKind,
};
use std::{
    fs,
    io::Read,
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    time::Duration,
};
const MAGIC: &[u8; 8] = b"MINIDR01";
fn bad(text: &str) -> ServiceError {
    ServiceError::new(ErrorCode::BadRequest, text)
}
fn io(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::new(ErrorCode::Io, error.to_string())
}
#[derive(Debug)]
pub(crate) struct Policy {
    path: PathBuf,
    pub enabled: bool,
    pub listener: Option<(String, SocketAddr)>,
}
impl Policy {
    pub fn open(path: PathBuf) -> Result<Self, ServiceError> {
        let mut policy = Self {
            path,
            enabled: false,
            listener: None,
        };
        match fs::symlink_metadata(&policy.path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(policy),
            Err(e) => return Err(io(e)),
            Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 1024 => {
                return Err(bad("unsafe delivery policy file"))
            }
            _ => (),
        }
        let mut bytes = Vec::new();
        fs::File::open(&policy.path)
            .map_err(io)?
            .take(1025)
            .read_to_end(&mut bytes)
            .map_err(io)?;
        if bytes.len() < 10
            || bytes.len() > 1024
            || &bytes[..8] != MAGIC
            || bytes[8] > 1
            || bytes[9] > 1
        {
            return Err(bad("invalid delivery policy"));
        }
        policy.enabled = bytes[8] == 1;
        if bytes[9] == 1 {
            let mut input = &bytes[10..];
            let request = mini_app_protocol::read_request(&mut input)
                .map_err(io)?
                .ok_or_else(|| bad("missing listener policy"))?;
            let Command::SetDeliveryListener { peer_id, endpoint } = request.command else {
                return Err(bad("invalid listener policy"));
            };
            if !input.is_empty() {
                return Err(bad("trailing listener policy"));
            }
            policy.listener = Some((peer_id, listener_address(&endpoint)?));
        } else if bytes.len() != 10 {
            return Err(bad("trailing delivery policy"));
        }
        Ok(policy)
    }
    fn save(
        &mut self,
        enabled: bool,
        listener: Option<(String, SocketAddr)>,
    ) -> Result<(), ServiceError> {
        let mut bytes = MAGIC.to_vec();
        bytes.push(u8::from(enabled));
        bytes.push(u8::from(listener.is_some()));
        if let Some((peer_id, address)) = &listener {
            mini_app_protocol::write_request(
                &mut bytes,
                &Request::new(
                    0,
                    Command::SetDeliveryListener {
                        peer_id: peer_id.clone(),
                        endpoint: address.to_string(),
                    },
                ),
            )
            .map_err(io)?;
        }
        // On uncertain durability fail closed in memory; restart reloads disk.
        self.enabled = false;
        mini_durable::atomic_replace(&self.path, &bytes).map_err(io)?;
        self.enabled = enabled;
        self.listener = listener;
        Ok(())
    }
    pub fn set_enabled(&mut self, enabled: bool) -> Result<(), ServiceError> {
        self.save(enabled, self.listener.clone())
    }
    pub fn set_listener(&mut self, peer_id: &str, endpoint: &str) -> Result<(), ServiceError> {
        self.save(
            self.enabled,
            Some((peer_id.into(), listener_address(endpoint)?)),
        )
    }
    pub fn clear_listener(&mut self) -> Result<(), ServiceError> {
        self.save(self.enabled, None)
    }
}
fn listener_address(text: &str) -> Result<SocketAddr, ServiceError> {
    let address: SocketAddr = text
        .parse()
        .map_err(|_| bad("listener must use numeric IP:port"))?;
    if address.port() == 0
        || address.ip().is_multicast()
        || matches!(address.ip(), std::net::IpAddr::V4(ip) if ip.is_broadcast())
    {
        return Err(bad("invalid listener address"));
    }
    Ok(address)
}
impl<V: IdentityVault> Core<V> {
    pub(crate) fn authorized_delivery_peer(&self, id: &str) -> Result<Peer, ServiceError> {
        self.peers
            .get(id)?
            .filter(|p| p.enabled)
            .ok_or_else(|| bad("delivery peer is absent or revoked"))
    }
    pub(crate) fn delivery_runtime_status(&self) -> DeliveryRuntimeView {
        DeliveryRuntimeView {
            enabled: self.delivery_policy.enabled,
            listener_peer: self.delivery_policy.listener.as_ref().map(|p| p.0.clone()),
            listener_endpoint: self
                .delivery_policy
                .listener
                .as_ref()
                .map(|p| p.1.to_string()),
            listening: self.delivery_listener.is_some(),
        }
    }
    pub(crate) fn attempt_delivery(
        &mut self,
        id: &str,
        now_ms: u64,
    ) -> Result<DeliveryView, ServiceError> {
        self.ensure_signing_available()?;
        if !self.delivery_policy.enabled {
            return Err(bad("public delivery networking is disabled"));
        }
        let current = self
            .outbox
            .get(id)?
            .ok_or_else(|| bad("unknown delivery operation"))?;
        if matches!(
            current.state,
            DeliveryState::Delivered | DeliveryState::Cancelled
        ) {
            return Ok(delivery_view(&current));
        }
        let record = self.outbox.begin_attempt(id, now_ms)?;
        self.emit(ServiceEventKind::DeliveryChanged(delivery_view(&record)));
        let identity = self.identity.as_ref().expect("checked unlocked identity");
        let result = (|| {
            let peer = self
                .authorized_delivery_peer(&current.peer_id)
                .map_err(|_| DeliveryFailure::Rejected)?;
            let routing = mini_crypto::AgreementSecretKey::generate()
                .map_err(|_| DeliveryFailure::Unavailable)?
                .public_key();
            let local = mini_transport_security::LocalSessionIdentity::new(
                identity.root.did(),
                &identity.device,
                routing,
            );
            let stream = TcpStream::connect_timeout(&peer.endpoint, Duration::from_secs(5))
                .map_err(|_| DeliveryFailure::Unavailable)?;
            delivery_transport::send(
                stream,
                local,
                &peer.root,
                &peer.device,
                &record.object_bytes,
                crate::now_ms(),
            )
        })();
        let completed = match result {
            Ok(()) => self.outbox.acknowledge(id)?,
            Err(failure) => self.outbox.retry(id, crate::now_ms(), failure)?,
        };
        let view = delivery_view(&completed);
        self.emit(ServiceEventKind::DeliveryChanged(view.clone()));
        Ok(view)
    }
    /// One bounded scheduling turn. The host calls this only while running;
    /// locked identities and disabled networking perform no socket operations.
    /// At most one receive and one outgoing attempt run per turn.
    pub fn poll_delivery(&mut self) -> Result<(), ServiceError> {
        if !self.delivery_policy.enabled || self.identity.is_none() {
            self.delivery_listener = None;
            return Ok(());
        }
        // Refuse sockets before touching them when durable state is uncertain.
        if let Err(error) = self.peers.ensure_healthy() {
            self.delivery_listener = None;
            return Err(error);
        }
        let due = match self.outbox.next_due(crate::now_ms()) {
            Ok(due) => due,
            Err(error) => {
                self.delivery_listener = None;
                return Err(error);
            }
        };
        let incoming = (|| {
            if let Some((peer_id, address)) = self.delivery_policy.listener.clone() {
                if let Ok(peer) = self.authorized_delivery_peer(&peer_id) {
                    if self.delivery_listener.is_none() {
                        let listener = TcpListener::bind(address).map_err(io)?;
                        listener.set_nonblocking(true).map_err(io)?;
                        self.delivery_listener = Some(listener);
                    }
                    let accepted = self
                        .delivery_listener
                        .as_ref()
                        .expect("bound listener")
                        .accept();
                    match accepted {
                        Ok((stream, _)) => {
                            // Windows accepted sockets inherit the listener's
                            // non-blocking mode; the handshake needs blocking I/O.
                            stream.set_nonblocking(false).map_err(io)?;
                            let identity = self.identity.as_ref().expect("unlocked");
                            let routing = mini_crypto::AgreementSecretKey::generate()
                                .map_err(io)?
                                .public_key();
                            let local = mini_transport_security::LocalSessionIdentity::new(
                                identity.root.did(),
                                &identity.device,
                                routing,
                            );
                            let _ = delivery_transport::receive(
                                stream,
                                local,
                                &peer.root,
                                &peer.device,
                                &mut self.store,
                                crate::now_ms(),
                            );
                            // Storage may have committed even when the ack was lost.
                            // Refreshing the feed makes no delivery/read receipt claim.
                            self.emit(ServiceEventKind::FeedChanged);
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => (),
                        Err(error) => return Err(io(error)),
                    }
                } else {
                    self.delivery_listener = None;
                }
            }
            Ok(())
        })();
        if let Some(id) = due {
            self.attempt_delivery(&id, crate::now_ms())?;
        }
        incoming
    }
}
