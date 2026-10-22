//! Owner-controlled public delivery. No requests are made merely by opening it.
use crate::app_service::{operation_id, Client};
use did_mini::{verify_delegation, Kel};
use eframe::egui;
use mini_app_protocol::{
    Command, DeliveryFailure, DeliveryPeerView, DeliveryRuntimeView, DeliveryState, DeliveryView,
    FeedCard, FeedOrder, FeedScope, Reply, MAX_DELIVERY_KEL_BYTES, MAX_FEED_ITEMS,
};
use std::{
    collections::VecDeque,
    sync::mpsc::{Receiver, TryRecvError},
    time::Duration,
};

const PREFIX: &str = "MININET-DELIVERY-PAIR1:";
const MAX_CODE: usize = PREFIX.len() + MAX_DELIVERY_KEL_BYTES * 4 + 1;
struct Pair {
    root: Vec<u8>,
    device: Vec<u8>,
    root_did: String,
    device_did: String,
}
fn parse_pair(code: &str) -> Result<Pair, String> {
    if code.len() > MAX_CODE {
        return Err("Pairing code exceeds the public record limit".into());
    }
    let (root, device) = code
        .trim()
        .strip_prefix(PREFIX)
        .and_then(|s| s.split_once(':'))
        .ok_or("Invalid public pairing code")?;
    fn decode(text: &str) -> Result<Vec<u8>, String> {
        if text.is_empty() || text.len() > MAX_DELIVERY_KEL_BYTES * 2 || text.len() % 2 != 0 {
            return Err("Invalid public record length".into());
        }
        text.as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                fn digit(b: u8) -> Option<u8> {
                    match b {
                        b'0'..=b'9' => Some(b - b'0'),
                        b'a'..=b'f' => Some(b - b'a' + 10),
                        _ => None,
                    }
                }
                Ok(
                    digit(pair[0]).ok_or("Pairing code must use lowercase hexadecimal")? * 16
                        + digit(pair[1]).ok_or("Pairing code must use lowercase hexadecimal")?,
                )
            })
            .collect()
    }
    let root = decode(root)?;
    let device = decode(device)?;
    let root_kel = Kel::from_bytes(&root).map_err(|e| e.to_string())?;
    let device_kel = Kel::from_bytes(&device).map_err(|e| e.to_string())?;
    root_kel.verify().map_err(|e| e.to_string())?;
    device_kel.verify().map_err(|e| e.to_string())?;
    verify_delegation(&root_kel, &device_kel).map_err(|e| e.to_string())?;
    if root_kel.to_bytes() != root || device_kel.to_bytes() != device {
        return Err("Noncanonical public pairing records".into());
    }
    Ok(Pair {
        root,
        device,
        root_did: root_kel.did().as_str().into(),
        device_did: device_kel.did().as_str().into(),
    })
}
fn pair_code(root: &[u8], device: &[u8]) -> Result<String, String> {
    fn hex(bytes: &[u8]) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            let _ = write!(s, "{b:02x}");
        }
        s
    }
    if root.len() > MAX_DELIVERY_KEL_BYTES || device.len() > MAX_DELIVERY_KEL_BYTES {
        return Err("Public pairing records exceed limits".into());
    }
    let code = format!("{PREFIX}{}:{}", hex(root), hex(device));
    parse_pair(&code)?;
    Ok(code)
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct QueueDraft {
    operation: String,
    peer: String,
    object: String,
}
impl QueueDraft {
    fn new(peer: String, object: String) -> Self {
        Self {
            operation: operation_id("delivery"),
            peer,
            object,
        }
    }
    fn command(&self) -> Command {
        Command::QueuePublicDelivery {
            operation_id: self.operation.clone(),
            peer_id: self.peer.clone(),
            object_id: self.object.clone(),
        }
    }
}
struct Pending {
    receiver: Receiver<Result<Reply, String>>,
    queue: bool,
}
pub(crate) struct PublicDeliveryPanel {
    pending: Option<Pending>,
    refresh: VecDeque<Command>,
    runtime: Option<DeliveryRuntimeView>,
    peers: Vec<DeliveryPeerView>,
    posts: Vec<FeedCard>,
    deliveries: Vec<DeliveryView>,
    error: String,
    notice: String,
    exported: String,
    pairing: String,
    parsed: Option<Pair>,
    label: String,
    endpoint: String,
    peer: String,
    post: String,
    listener_peer: String,
    bind: String,
    draft: Option<QueueDraft>,
    abandon: bool,
    page_after: Option<String>,
    recent: Option<DeliveryView>,
}
impl Default for PublicDeliveryPanel {
    fn default() -> Self {
        Self {
            pending: None,
            refresh: VecDeque::new(),
            runtime: None,
            peers: Vec::new(),
            posts: Vec::new(),
            deliveries: Vec::new(),
            error: String::new(),
            notice: String::new(),
            exported: String::new(),
            pairing: String::new(),
            parsed: None,
            label: String::new(),
            endpoint: "127.0.0.1:7400".into(),
            peer: String::new(),
            post: String::new(),
            listener_peer: String::new(),
            bind: "127.0.0.1:7400".into(),
            draft: None,
            abandon: false,
            page_after: None,
            recent: None,
        }
    }
}
impl PublicDeliveryPanel {
    pub(crate) fn has_pending(&self) -> bool {
        self.pending.is_some() || !self.refresh.is_empty()
    }
    pub(crate) fn observe_delivery(&mut self, value: DeliveryView) {
        if self
            .recent
            .as_ref()
            .is_some_and(|old| old.operation_id == value.operation_id)
        {
            self.recent = Some(value.clone());
        }
        if let Some(old) = self
            .deliveries
            .iter_mut()
            .find(|d| d.operation_id == value.operation_id)
        {
            *old = value;
        }
    }
    fn send(&mut self, client: &Client, command: Command) {
        let queue = matches!(command, Command::QueuePublicDelivery { .. });
        match client.request(command) {
            Ok(receiver) => {
                self.pending = Some(Pending { receiver, queue });
                self.error.clear();
            }
            Err(e) => {
                self.error = e;
                self.refresh.clear();
            }
        }
    }
    fn poll(&mut self, client: Option<&Client>) {
        if let Some(pending) = &self.pending {
            let result=match pending.receiver.try_recv() { Ok(result)=>Some(result),Err(TryRecvError::Empty)=>None,Err(TryRecvError::Disconnected)=>Some(Err("Application service disconnected; the operation may have completed. Retry the same delivery.".into())) };
            if let Some(result) = result {
                let queue = self.pending.take().unwrap().queue;
                match result {
                    Err(e) => {
                        self.error = e;
                        self.refresh.clear();
                    }
                    Ok(reply) => {
                        if queue
                            && !matches!((&reply, &self.draft), (Reply::Delivery(value), Some(draft)) if value.operation_id == draft.operation && value.peer_id == draft.peer && value.object_id == draft.object)
                        {
                            self.error =
                                "Unexpected delivery receipt; retained the exact request for retry"
                                    .into();
                            self.refresh.clear();
                            return;
                        }
                        match reply {
                            Reply::DeliveryRuntime(value) => self.runtime = Some(value),
                            Reply::DeliveryPeers(values) => {
                                // Authorization replies contain only one peer; merge without dropping others.
                                for value in values {
                                    if let Some(old) =
                                        self.peers.iter_mut().find(|p| p.peer_id == value.peer_id)
                                    {
                                        *old = value;
                                    } else {
                                        self.peers.push(value);
                                    }
                                }
                                self.peers.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
                            }
                            Reply::Feed(cards) => {
                                self.posts = cards
                                    .into_iter()
                                    .filter(|c| c.own && c.media.is_none())
                                    .collect()
                            }
                            Reply::Deliveries(values) => self.deliveries = values,
                            Reply::Delivery(value) => {
                                self.observe_delivery(value.clone());
                                self.recent = Some(value);
                            }
                            Reply::DeliveryIdentity(value) => {
                                match pair_code(&value.root_kel, &value.device_kel) {
                                    Ok(code) => self.exported = code,
                                    Err(e) => self.error = e,
                                }
                            }
                            Reply::Ack => {
                                self.notice =
                                    "Request completed. Refresh to inspect current state.".into()
                            }
                            _ => self.error = "Unexpected application service response".into(),
                        }
                        if queue && self.error.is_empty() {
                            self.draft = None;
                            self.abandon = false;
                            self.notice = "Delivery recorded; check its status below.".into();
                        }
                    }
                }
            }
        }
        if self.pending.is_none() {
            if let (Some(client), Some(command)) = (client, self.refresh.pop_front()) {
                self.send(client, command);
            }
        }
    }
    pub(crate) fn show(&mut self, ui: &mut egui::Ui, client: Option<&Client>) {
        self.poll(client);
        ui.heading("Public delivery");
        ui.label("Send one public text post to an explicitly authorized peer. Reachable numeric TCP addresses are required; NAT/relay routing and private chat are not included.");
        ui.label("Networking starts only after an owner grant and an unlocked identity. Public pairing codes contain no seeds; verify the full identities through a trusted channel.");
        if client.is_none() {
            ui.label("Application service unavailable. Restart Mininet with its packaged application service installed.");
        }
        if !self.error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
        }
        if !self.notice.is_empty() {
            ui.label(&self.notice);
        }
        let mut command = None;
        let busy = self.has_pending();
        ui.add_enabled_ui(client.is_some()&&!busy,|ui|{
            if ui.button("Refresh status, peers, own posts and deliveries").clicked(){
                self.peers.clear();self.page_after=None;self.refresh=refresh_commands();
            }
            if let Some(runtime)=&self.runtime {
                ui.label(format!("Networking: {} · listener: {}",if runtime.enabled {"owner enabled"}else{"disabled"},if runtime.listening{"listening"}else{"not listening"}));
                if let Some(address)=&runtime.listener_endpoint {ui.label(format!("Configured listener: {address} for {}",runtime.listener_peer.as_deref().unwrap_or("unknown")));}
                if ui.button(if runtime.enabled{"Disable public-delivery networking"}else{"Enable public-delivery networking (persists)"}).clicked(){command=Some(Command::SetDeliveryEnabled{enabled:!runtime.enabled});}
            }else{ui.label("Refresh to inspect the persisted networking grant before changing it.");}
            ui.collapsing("Public pairing and authorized peers",|ui|{
                if ui.button("Export my public pairing code").clicked(){command=Some(Command::ExportDeliveryIdentity);}
                if !self.exported.is_empty(){if ui.button("Copy my public pairing code").clicked(){ui.ctx().copy_text(self.exported.clone());}ui.label("Public root/device records only. Share through your chosen trusted channel.");}
                ui.label("Paste the other person's public pairing code");
                if ui.add(egui::TextEdit::multiline(&mut self.pairing).desired_rows(2).char_limit(MAX_CODE)).changed(){self.parsed=None;}
                if ui.button("Inspect public identities").clicked(){match parse_pair(&self.pairing){Ok(pair)=>{self.parsed=Some(pair);self.error.clear();},Err(e)=>{self.parsed=None;self.error=e;}}}
                ui.horizontal(|ui|{ui.label("Peer label");ui.add(egui::TextEdit::singleline(&mut self.label).char_limit(96));});
                ui.horizontal(|ui|{ui.label("Peer numeric IP:port");ui.add(egui::TextEdit::singleline(&mut self.endpoint).char_limit(128));});
                if let Some(pair)=&self.parsed {
                    ui.label(format!("Root DID: {}",pair.root_did));ui.label(format!("Device DID: {}",pair.device_did));
                    ui.label("These full identifiers are the cryptographic fingerprints to verify. One retained label/device per root is supported.");
                    if ui.button("Authorize this public identity and endpoint").clicked(){command=Some(Command::AuthorizeDeliveryPeer{peer_id:self.label.clone(),endpoint:self.endpoint.clone(),root_kel:pair.root.clone(),device_kel:pair.device.clone()});}
                }
                for peer in &self.peers {ui.group(|ui|{ui.label(format!("{} · {} · {}",peer.peer_id,peer.endpoint,if peer.enabled{"authorized"}else{"revoked/disabled"}));ui.label(&peer.root_did);ui.label(&peer.device_did);if peer.enabled&&ui.button(format!("Revoke {}",peer.peer_id)).clicked(){command=Some(Command::RevokeDeliveryPeer{peer_id:peer.peer_id.clone()});}});}
            });
            ui.collapsing("Incoming listener — advanced reachable TCP settings",|ui|{
                peer_select(ui,"listener-peer",&self.peers,&mut self.listener_peer);
                ui.horizontal(|ui|{ui.label("Bind numeric IP:port");ui.add(egui::TextEdit::singleline(&mut self.bind).char_limit(128));});
                ui.label("The listener accepts only this one expected peer. Binding 0.0.0.0 or [::] exposes the port on available interfaces.");
                if ui.add_enabled(self.peers.iter().any(|p|p.enabled&&p.peer_id==self.listener_peer),egui::Button::new("Configure expected-peer listener")).clicked(){command=Some(Command::SetDeliveryListener{peer_id:self.listener_peer.clone(),endpoint:self.bind.clone()});}
                if ui.button("Stop and remove listener configuration").clicked(){command=Some(Command::StopDeliveryListener);}
            });
            ui.separator();ui.label("Review one public post and recipient");
            ui.add_enabled_ui(self.draft.is_none(),|ui|{
                peer_select(ui,"delivery-peer",&self.peers,&mut self.peer);
                egui::ComboBox::from_id_salt("delivery-post").selected_text(if self.post.is_empty(){"Select one own public text post"}else{"Selected own public post"}).show_ui(ui,|ui|{for card in &self.posts {let summary:String=card.body.chars().take(70).collect();ui.selectable_value(&mut self.post,card.id.clone(),summary);}});
            });
            let selected=self.posts.iter().find(|p|p.id==self.post);
            let recipient=self.peers.iter().find(|p|p.peer_id==self.peer&&p.enabled);
            if let Some(card)=selected {ui.group(|ui|{ui.label(&card.body);ui.small(format!("Public object {}",card.id));});}
            if let Some(peer)=recipient {ui.label(format!("Recipient: {} at {}",peer.peer_id,peer.endpoint));ui.small(&peer.root_did);}
            if self.draft.is_some(){ui.label("A delivery request is retained for an exact retry. Its response may have been lost; retrying uses the same operation ID.");}
            if ui.add_enabled(self.draft.is_some() || (selected.is_some()&&recipient.is_some()),egui::Button::new(if self.draft.is_some(){"Retry the same delivery request"}else{"Queue this public post to this peer"})).clicked(){
                command=Some(queue_command(&mut self.draft,self.peer.clone(),self.post.clone()));
            }
            if self.draft.is_some(){ui.checkbox(&mut self.abandon,"I understand the previous request may already exist");if ui.add_enabled(self.abandon,egui::Button::new("Start a different delivery draft")).clicked(){self.draft=None;self.abandon=false;}}
            ui.separator();ui.label("Delivery status (64 operations per page)");
            ui.horizontal(|ui| {
                if ui.button("First delivery page").clicked() {self.page_after=None;command=Some(Command::ListDeliveriesAfter{after_operation_id:None,limit:64});}
                if ui.add_enabled(self.deliveries.len()==64,egui::Button::new("Next delivery page")).clicked(){self.page_after=self.deliveries.last().map(|d|d.operation_id.clone());command=Some(Command::ListDeliveriesAfter{after_operation_id:self.page_after.clone(),limit:64});}
            });
            if let Some(after)=&self.page_after {ui.small(format!("Page after operation {after}"));}
            ui.label("Delivered means the authenticated peer acknowledged durable storage; it is not a read receipt. Cancel cannot retract a delivered post.");
            if let Some(recent)=&self.recent {ui.label("Most recent requested delivery");ui.push_id("recent-delivery",|ui|{if let Some(action)=delivery_row(ui,recent){command=Some(action);}});}
            for delivery in &self.deliveries {if let Some(action)=delivery_row(ui,delivery){command=Some(action);}}
        });
        if self.has_pending() {
            ui.spinner();
            ui.label("Waiting for application service; network work is bounded but may delay other requests.");
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        if let (Some(client), Some(command)) = (client, command) {
            self.send(client, command);
            ui.ctx().request_repaint();
        }
        if !self.refresh.is_empty() {
            ui.ctx().request_repaint();
        }
    }
}
fn refresh_commands() -> VecDeque<Command> {
    VecDeque::from([
        Command::DeliveryRuntimeStatus,
        Command::DeliveryPeers,
        Command::FeedSnapshot {
            order: FeedOrder::Chronological,
            scope: FeedScope::Everyone,
            limit: MAX_FEED_ITEMS,
        },
        Command::ListDeliveriesAfter {
            after_operation_id: None,
            limit: 64,
        },
    ])
}
fn queue_command(draft: &mut Option<QueueDraft>, peer: String, object: String) -> Command {
    draft
        .get_or_insert_with(|| QueueDraft::new(peer, object))
        .command()
}
fn delivery_row(ui: &mut egui::Ui, delivery: &DeliveryView) -> Option<Command> {
    let mut command = None;
    ui.push_id(&delivery.operation_id, |ui| {
        ui.group(|ui| {
            let state = match delivery.state {
                DeliveryState::Queued => "Queued",
                DeliveryState::InFlight => "In flight",
                DeliveryState::RetryWaiting => "Waiting to retry",
                DeliveryState::Delivered => "Delivered — peer storage acknowledged",
                DeliveryState::Cancelled => "Cancelled",
            };
            ui.label(format!("To {} · {state}", delivery.peer_id));
            let failure = match delivery.last_failure {
                None => "No failure reported",
                Some(DeliveryFailure::Unavailable) => "Peer or connection unavailable",
                Some(DeliveryFailure::Rejected) => {
                    "Peer identity, post or acknowledgement was rejected"
                }
                Some(DeliveryFailure::Interrupted) => "Previous attempt was interrupted",
            };
            ui.label(format!("Attempts: {} · {failure}", delivery.attempts));
            if delivery.state == DeliveryState::RetryWaiting {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
                    .unwrap_or(0);
                let remaining = delivery.next_attempt_ms.saturating_sub(now);
                let seconds = remaining / 1000 + u64::from(remaining % 1000 != 0);
                if seconds == 0 {
                    ui.label("Retry is due when networking is enabled and identity is unlocked");
                } else {
                    ui.label(format!("Next retry eligible in {seconds} seconds"));
                }
            }
            ui.small(format!("Delivery reference: {}", delivery.operation_id));
            if !matches!(
                delivery.state,
                DeliveryState::Delivered | DeliveryState::Cancelled
            ) && ui.button("Cancel pending delivery").clicked()
            {
                command = Some(Command::CancelDelivery {
                    operation_id: delivery.operation_id.clone(),
                });
            }
        });
    });
    command
}
fn peer_select(ui: &mut egui::Ui, id: &str, peers: &[DeliveryPeerView], selected: &mut String) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(if selected.is_empty() {
            "Select an authorized peer"
        } else {
            selected.as_str()
        })
        .show_ui(ui, |ui| {
            for peer in peers.iter().filter(|p| p.enabled) {
                ui.selectable_value(selected, peer.peer_id.clone(), &peer.peer_id);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::{Capabilities, Controller};
    #[test]
    fn refresh_requests_respect_the_actual_service_protocol_limits() {
        let commands = refresh_commands();
        assert_eq!(commands.len(), 4);
        for command in commands {
            mini_app_protocol::Request::new(1, command)
                .validate()
                .unwrap();
        }
    }
    #[test]
    fn public_pair_codec_is_bounded_strict_and_checks_delegation() {
        let mut root = Controller::incept_single_from_seeds(&[1; 32], &[2; 32]).unwrap();
        let device =
            Controller::incept_device_single_from_seeds(&root.did(), &[3; 32], &[4; 32]).unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        let code = pair_code(&root.kel().to_bytes(), &device.kel().to_bytes()).unwrap();
        let parsed = parse_pair(&code).unwrap();
        assert_eq!(parsed.root_did, root.did().as_str());
        assert_eq!(parsed.device_did, device.did().as_str());
        assert!(parse_pair(&(code.clone() + ":00")).is_err());
        assert!(parse_pair(&"x".repeat(MAX_CODE + 1)).is_err());
        assert!(parse_pair("MININET-DELIVERY-PAIR1:ff:00").is_err());
        root.revoke_device(&device.did()).unwrap();
        assert!(pair_code(&root.kel().to_bytes(), &device.kel().to_bytes()).is_err());
    }
    #[test]
    fn uncertain_queue_retry_preserves_exact_operation_and_selection() {
        let mut draft = None;
        let original = queue_command(&mut draft, "peer".into(), "object".into());
        // A lost/error response leaves the draft in place, even if refreshed
        // selections change before a retry is requested.
        let retry = queue_command(
            &mut draft,
            "different peer".into(),
            "different object".into(),
        );
        assert_eq!(original, retry);
        draft = None;
        let next = queue_command(
            &mut draft,
            "different peer".into(),
            "different object".into(),
        );
        assert_ne!(original, next);
    }
}
