//! Per-user Mininet application core.
//!
//! This is the first W1 authority boundary for the Windows product. The
//! renderer sends capability-shaped commands; this service owns the local
//! single-writer lock, identity controllers while unlocked, signed mutation
//! sequencing, crash-recoverable publication, and feed materialization.
//!
//! Public social objects are journaled as their exact signed bytes before the
//! store is mutated. Recovery can therefore replay the same content-addressed
//! object after a crash without allocating a second sequence or producing a
//! second signature. Completed operation ids remain durable receipts, making
//! renderer retries idempotent across service restarts.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod delivery_interrupt;
pub use delivery_interrupt::{DeliveryInterrupt, DeliveryStopGuard};
mod delivery_peers;
mod delivery_runtime;
mod delivery_transport;
mod outbox;
mod profile_photo;

use did_mini::{Capabilities, Controller, Did};
use mini_app_protocol::{
    AccountStatus, Command, ErrorCode, FeedCard, FeedOrder, FeedReason, FeedScope, ObjectKind,
    ProfileView, PublicProfileDetails, PublicWallDraft, PublishedObject, Reply, ServiceError,
    ServiceEvent, ServiceEventKind, MAX_COMMENT_BYTES, MAX_COMMUNITY_CHARTER_BYTES,
    MAX_COMMUNITY_NAME_BYTES, MAX_DID_BYTES, MAX_OBJECT_ID_BYTES, MAX_OPERATION_ID_BYTES,
    MAX_POST_BYTES, MAX_PROFILE_BIO_BYTES, MAX_PROFILE_NAME_BYTES,
};
mod beta_journal;
mod beta_wallet;
use mini_objects::{Object, ObjectId, ObjectType, Payload};
use mini_social::{
    build_comment, build_community, build_follow, build_membership, build_post, build_profile,
    build_profile_details, build_reaction, comments, feed, following, reaction_counts,
    resolve_post, resolve_profile, FeedFilter, FeedReason as SocialFeedReason, PostKind,
};
use mini_store::{Backend, FsBackend, Store};
use mini_windows_vault::{SeedPair, VaultError};
use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

const EVENT_BACKLOG: usize = 256;
const JOURNAL_VERSION: u8 = 1;
const MAX_PENDING_OPERATIONS: usize = 1024;
const MAX_JOURNAL_RECORD_BYTES: usize = 256 * 1024;
pub(crate) const MAX_SIGNED_OBJECT_BYTES: usize = 128 * 1024;
const PENDING_MAGIC: &[u8; 8] = b"MINIOP1P";
const RECEIPT_MAGIC: &[u8; 8] = b"MINIOP1R";
const SEQUENCE_MAGIC: &[u8; 8] = b"MINISEQ1";

#[derive(Debug)]
pub(crate) struct CoreIdentity {
    pub(crate) root: Controller,
    pub(crate) device: Controller,
}

/// Seed-envelope provider for the application core.
///
/// Production uses the current Windows-user DPAPI boundary. Tests can inject a
/// deterministic implementation without teaching the service another key
/// format or weakening production code paths.
pub trait IdentityVault: core::fmt::Debug {
    fn root_exists(&self) -> bool;
    fn load_root(&mut self) -> Result<SeedPair, ServiceError>;
    fn load_or_create_root(&mut self) -> Result<SeedPair, ServiceError>;
    fn load_or_create_device(&mut self) -> Result<SeedPair, ServiceError>;
}

/// Production DPAPI-backed identity vault rooted in one Mininet data directory.
#[derive(Debug, Clone)]
pub struct WindowsIdentityVault {
    root: PathBuf,
}

impl WindowsIdentityVault {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn map_vault(error: VaultError) -> ServiceError {
        let code = match error {
            VaultError::Io(_) => ErrorCode::Io,
            VaultError::UnsupportedPlatform
            | VaultError::ProtectionFailed
            | VaultError::InvalidEnvelope
            | VaultError::Entropy => ErrorCode::Identity,
        };
        ServiceError::new(code, error.to_string())
    }
}

impl IdentityVault for WindowsIdentityVault {
    fn root_exists(&self) -> bool {
        self.root.join("identity.dpapi").exists()
    }

    fn load_root(&mut self) -> Result<SeedPair, ServiceError> {
        mini_windows_vault::load_existing(&self.root.join("identity.dpapi"))
            .map_err(Self::map_vault)
    }

    fn load_or_create_root(&mut self) -> Result<SeedPair, ServiceError> {
        mini_windows_vault::load_or_create(&self.root.join("identity.dpapi"))
            .map_err(Self::map_vault)
    }

    fn load_or_create_device(&mut self) -> Result<SeedPair, ServiceError> {
        mini_windows_vault::load_or_create(&self.root.join("device.dpapi")).map_err(Self::map_vault)
    }
}

/// Long-lived application authority. The production specialization holds an
/// exclusive core-service-instance lock for its lifetime. During W1 migration,
/// older renderer/network paths can still mutate the filesystem store; the
/// final single-writer guarantee arrives when those paths move behind IPC.
#[derive(Debug)]
pub struct Core<V: IdentityVault> {
    store: Store<FsBackend>,
    vault: V,
    identity: Option<CoreIdentity>,
    human: Option<Did>,
    next_sequence: u64,
    journal: MutationJournal,
    beta_ops: PathBuf,
    outbox: outbox::Outbox,
    peers: delivery_peers::PeerBook,
    delivery_policy: delivery_runtime::Policy,
    delivery_listener: Option<std::net::TcpListener>,
    delivery_interrupt: DeliveryInterrupt,
    events: VecDeque<ServiceEvent>,
    next_event_id: u64,
    _process_lock: Option<File>,
}

impl Core<WindowsIdentityVault> {
    /// Open the production core and refuse a second live core-service instance.
    pub fn open(root: PathBuf) -> Result<Self, ServiceError> {
        let vault = WindowsIdentityVault::new(root.clone());
        Self::open_inner(root, vault, true)
    }
}

impl<V: IdentityVault> Core<V> {
    fn open_inner(root: PathBuf, mut vault: V, exclusive: bool) -> Result<Self, ServiceError> {
        mini_durable::create_dir_all(&root)
            .map_err(|error| ServiceError::new(ErrorCode::Io, error.to_string()))?;
        let process_lock = if exclusive {
            Some(
                mini_durable::try_lock_exclusive(&root.join("app-service.lock")).map_err(
                    |error| {
                        ServiceError::new(
                            ErrorCode::Busy,
                            format!("another application core owns this profile: {error}"),
                        )
                    },
                )?,
            )
        } else {
            None
        };

        let mut store = Store::new(
            FsBackend::open(&root)
                .map_err(|error| ServiceError::new(ErrorCode::Storage, error.to_string()))?,
        );
        let journal = MutationJournal::open(root.join("app-service"))?;
        journal.recover(&mut store)?;
        let beta_ops = root.join("app-service/beta-ops");
        beta_journal::open(&beta_ops)?;
        beta_journal::commit_pending(&beta_ops, &mut store)?;
        let outbox = outbox::Outbox::open(root.join("app-service/outbox"))?;
        let peers = delivery_peers::PeerBook::open(root.join("app-service/delivery-peers"))?;
        let delivery_policy =
            delivery_runtime::Policy::open(root.join("app-service/delivery-policy"))?;

        let human = if vault.root_exists() {
            let seeds = vault.load_root()?;
            Some(root_from_seeds(&seeds)?.did())
        } else {
            None
        };
        let store_next = next_object_sequence(&store, human.as_ref())?;
        let next_sequence = journal.ensure_sequence_floor(store_next)?;

        Ok(Self {
            store,
            vault,
            identity: None,
            human,
            next_sequence,
            journal,
            beta_ops,
            outbox,
            peers,
            delivery_policy,
            delivery_listener: None,
            delivery_interrupt: DeliveryInterrupt::default(),
            events: VecDeque::new(),
            next_event_id: 1,
            _process_lock: process_lock,
        })
    }

    /// A denial-only handle for the host's bounded IPC reader. It conveys no
    /// identity, signing, store, or networking-start capability.
    pub fn delivery_interrupt_handle(&self) -> DeliveryInterrupt {
        self.delivery_interrupt.clone()
    }

    pub fn handle(&mut self, command: Command) -> Result<Reply, ServiceError> {
        mini_app_protocol::Request::new(0, command.clone())
            .validate()
            .map_err(|error| ServiceError::new(ErrorCode::BadRequest, error.to_string()))?;
        match command {
            Command::BetaRegisterAccount {
                operation_id,
                epoch,
            } => self.beta_register_account(&operation_id, epoch),
            Command::BetaBalances { epoch, campaigns } => self.beta_balances(epoch, &campaigns),
            Command::BetaTransfer {
                operation_id,
                epoch,
                campaigns,
                from,
                to,
                amount_micro,
                memo,
            } => self.beta_transfer(
                &operation_id,
                epoch,
                &campaigns,
                from,
                to,
                amount_micro,
                &memo,
            ),
            Command::Status => self.status().map(Reply::Status),
            Command::CreateRoot => {
                self.create_root()?;
                self.status().map(Reply::Status)
            }
            Command::UnlockIdentity => {
                self.unlock()?;
                self.status().map(Reply::Status)
            }
            Command::LockIdentity => {
                self.lock();
                self.status().map(Reply::Status)
            }
            Command::CurrentProfile => self.current_profile().map(Reply::Profile),
            Command::PublishProfile {
                operation_id,
                display_name,
                bio,
            } => self
                .publish_profile(&operation_id, &display_name, &bio)
                .map(Reply::Published),
            Command::FeedSnapshot {
                order,
                scope,
                limit,
            } => self
                .feed_snapshot(order, scope, usize::from(limit))
                .map(Reply::Feed),
            Command::PublishMediaPost {
                operation_id,
                media_id,
                caption,
            } => self
                .publish_media_post(&operation_id, &media_id, &caption)
                .map(Reply::Published),
            Command::PublishWall {
                operation_id,
                draft,
            } => self
                .publish_wall(&operation_id, draft)
                .map(Reply::Published),
            Command::PublishProfileWithPhoto {
                operation_id,
                draft,
                photo,
            } => self
                .publish_profile_with_photo(&operation_id, draft, &photo)
                .map(Reply::Published),
            Command::PublishProfileDetails {
                operation_id,
                draft,
            } => self
                .publish_profile_details(&operation_id, draft)
                .map(Reply::Published),
            Command::PreviewPublicDelivery { object_id } => self
                .own_public_delivery_object(&object_id)
                .map(|(_, view)| Reply::PublicDeliveryObject(view)),
            Command::PublishCommunity {
                operation_id,
                name,
                charter,
            } => self
                .publish_community(&operation_id, &name, &charter)
                .map(Reply::Published),
            Command::SetMembership {
                operation_id,
                community_id,
                joined,
            } => self
                .set_membership(&operation_id, &community_id, joined)
                .map(Reply::Published),
            Command::PublishComment {
                operation_id,
                parent_id,
                text,
            } => self
                .publish_comment(&operation_id, &parent_id, &text)
                .map(Reply::Published),
            Command::SetLike {
                operation_id,
                target_id,
                active,
            } => self
                .set_like(&operation_id, &target_id, active)
                .map(Reply::Published),
            Command::SetFollow {
                operation_id,
                target_did,
                following,
            } => self
                .set_follow(&operation_id, &target_did, following)
                .map(Reply::Published),
            Command::PublishPost { operation_id, text } => self
                .publish_post(&operation_id, &text)
                .map(Reply::Published),
            Command::DrainEvents { limit } => {
                Ok(Reply::Events(self.drain_events(usize::from(limit))))
            }
            Command::DeliveryRuntimeStatus => {
                Ok(Reply::DeliveryRuntime(self.delivery_runtime_status()))
            }
            Command::ExportDeliveryIdentity => {
                self.ensure_signing_available()?;
                let identity = self.identity.as_ref().expect("checked identity");
                Ok(Reply::DeliveryIdentity(
                    mini_app_protocol::DeliveryIdentity {
                        root_kel: identity.root.kel().to_bytes(),
                        device_kel: identity.device.kel().to_bytes(),
                    },
                ))
            }
            Command::AuthorizeDeliveryPeer {
                peer_id,
                endpoint,
                root_kel,
                device_kel,
            } => {
                self.ensure_signing_available()?;
                let peer = self
                    .peers
                    .authorize(&peer_id, &endpoint, &root_kel, &device_kel)?;
                self.delivery_listener = None;
                Ok(Reply::DeliveryPeers(vec![peer.view()]))
            }
            Command::RevokeDeliveryPeer { peer_id } => {
                self.delivery_listener = None;
                self.peers.revoke(&peer_id)?;
                self.delivery_listener = None;
                Ok(Reply::Ack)
            }
            Command::DeliveryPeers => Ok(Reply::DeliveryPeers(
                self.peers
                    .list()?
                    .iter()
                    .map(delivery_peers::Peer::view)
                    .collect(),
            )),
            Command::AttemptDelivery { operation_id } => self
                .attempt_delivery(&operation_id, now_ms())
                .map(Reply::Delivery),
            Command::SetDeliveryEnabled { enabled } => {
                if enabled {
                    self.ensure_signing_available()?;
                }
                self.delivery_policy.set_enabled(enabled)?;
                self.delivery_listener = None;
                Ok(Reply::DeliveryRuntime(self.delivery_runtime_status()))
            }
            Command::SetDeliveryListener { peer_id, endpoint } => {
                self.ensure_signing_available()?;
                self.authorized_delivery_peer(&peer_id)?;
                self.delivery_policy.set_listener(&peer_id, &endpoint)?;
                self.delivery_listener = None;
                Ok(Reply::DeliveryRuntime(self.delivery_runtime_status()))
            }
            Command::StopDeliveryListener => {
                self.delivery_policy.clear_listener()?;
                self.delivery_listener = None;
                Ok(Reply::DeliveryRuntime(self.delivery_runtime_status()))
            }
            Command::QueuePublicDelivery {
                operation_id,
                peer_id,
                object_id,
            } => self
                .queue_public_delivery(&operation_id, &peer_id, &object_id)
                .map(Reply::Delivery),
            Command::DeliveryStatus { operation_id } => {
                let record = self.outbox.get(&operation_id)?.ok_or_else(|| {
                    ServiceError::new(ErrorCode::BadRequest, "unknown delivery operation")
                })?;
                Ok(Reply::Delivery(delivery_view(&record)))
            }
            Command::ListDeliveriesAfter {
                after_operation_id,
                limit,
            } => Ok(Reply::Deliveries(
                self.outbox
                    .list_after(after_operation_id.as_deref(), usize::from(limit))?
                    .iter()
                    .map(delivery_view)
                    .collect(),
            )),
            Command::ListDeliveries { limit } => Ok(Reply::Deliveries(
                self.outbox
                    .list(usize::from(limit))?
                    .iter()
                    .map(delivery_view)
                    .collect(),
            )),
            Command::CancelDelivery { operation_id } => {
                let record = self.outbox.cancel(&operation_id)?;
                let view = delivery_view(&record);
                self.emit(ServiceEventKind::DeliveryChanged(view.clone()));
                Ok(Reply::Delivery(view))
            }
            Command::Shutdown => Ok(Reply::Ack),
        }
    }

    fn queue_public_delivery(
        &mut self,
        operation_id: &str,
        peer_id: &str,
        object_id: &str,
    ) -> Result<mini_app_protocol::DeliveryView, ServiceError> {
        self.ensure_signing_available()?;
        self.authorized_delivery_peer(peer_id)?;
        let (object, _) = self.own_public_delivery_object(object_id)?;
        let record = self
            .outbox
            .enqueue(operation_id, peer_id, object_id, &object.to_bytes())?;
        let view = delivery_view(&record);
        self.emit(ServiceEventKind::DeliveryChanged(view.clone()));
        Ok(view)
    }

    fn own_public_delivery_object(
        &self,
        object_id: &str,
    ) -> Result<(Object, mini_app_protocol::DeliveryObjectView), ServiceError> {
        self.ensure_signing_available()?;
        if object_id.is_empty() || object_id.len() > MAX_OBJECT_ID_BYTES {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "delivery object ID exceeds limits",
            ));
        }
        let id = ObjectId::parse(object_id).map_err(storage_or_social)?;
        let object = self.store.get(&id).map_err(storage_or_social)?;
        let identity = self.identity.as_ref().expect("checked unlocked identity");
        if object.author_human != identity.root.did()
            || object.author_device != identity.device.did()
        {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "delivery requires an own signed public object",
            ));
        }
        let view = delivery_transport::public_object_view(&object).map_err(|_| {
            ServiceError::new(
                ErrorCode::BadRequest,
                "unsupported or malformed public delivery object",
            )
        })?;
        mini_objects::verify_provenance(&object, &identity.root.kel(), &identity.device.kel())
            .map_err(storage_or_social)?;
        Ok((object, view))
    }

    fn status(&self) -> Result<AccountStatus, ServiceError> {
        Ok(AccountStatus {
            root_created: self.human.is_some(),
            identity_unlocked: self.identity.is_some(),
            human_did: self.human.as_ref().map(|did| did.as_str().to_string()),
            profile: self.current_profile()?,
        })
    }

    fn current_profile(&self) -> Result<Option<ProfileView>, ServiceError> {
        let Some(human) = self.human.as_ref() else {
            return Ok(None);
        };
        resolve_profile(&self.store, human)
            .map_err(storage_or_social)?
            .map(profile_view)
            .transpose()
    }

    fn create_root(&mut self) -> Result<(), ServiceError> {
        if self.human.is_some() {
            return Ok(());
        }
        let root_seeds = self.vault.load_or_create_root()?;
        let device_seeds = self.vault.load_or_create_device()?;
        let identity = identity_from_seeds(&root_seeds, &device_seeds)?;
        let did = identity.root.did();
        self.human = Some(did.clone());
        self.identity = Some(identity);
        let store_next = next_object_sequence(&self.store, Some(&did))?;
        self.next_sequence = self.journal.ensure_sequence_floor(store_next)?;
        self.emit(ServiceEventKind::RootCreated {
            did: did.as_str().to_string(),
        });
        self.emit(ServiceEventKind::IdentityChanged { unlocked: true });
        Ok(())
    }

    fn unlock(&mut self) -> Result<(), ServiceError> {
        if self.identity.is_some() {
            return Ok(());
        }
        if !self.vault.root_exists() {
            return Err(ServiceError::new(
                ErrorCode::RootMissing,
                "create a Mininet root first",
            ));
        }
        let root_seeds = self.vault.load_root()?;
        let device_seeds = self.vault.load_or_create_device()?;
        let identity = identity_from_seeds(&root_seeds, &device_seeds)?;
        let did = identity.root.did();
        self.human = Some(did.clone());
        self.identity = Some(identity);
        let store_next = next_object_sequence(&self.store, Some(&did))?;
        self.next_sequence = self
            .journal
            .ensure_sequence_floor(self.next_sequence.max(store_next))?;
        self.emit(ServiceEventKind::IdentityChanged { unlocked: true });
        Ok(())
    }

    fn lock(&mut self) {
        self.delivery_listener = None;
        if self.identity.take().is_some() {
            self.emit(ServiceEventKind::IdentityChanged { unlocked: false });
        }
    }

    fn publish_post(
        &mut self,
        operation_id: &str,
        text: &str,
    ) -> Result<PublishedObject, ServiceError> {
        validate_operation(operation_id)?;
        if text.is_empty() || text.len() > MAX_POST_BYTES {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "post text is empty or exceeds the protocol limit",
            ));
        }
        let input = MutationInput::Post {
            text: text.to_string(),
        };
        if let Some(existing) = self.journal.existing(operation_id, &input)? {
            return self.finish_existing(existing, ObjectKind::Post);
        }

        self.ensure_signing_available()?;
        let sequence = self.reserve_sequence()?;
        let (human, device) = self.signing_identity()?;
        let object =
            build_post(human, device, text, now_ms(), sequence).map_err(storage_or_social)?;
        let record = PendingMutation {
            operation_id: operation_id.to_string(),
            input,
            objects: vec![object.to_bytes()],
        };
        self.journal.write_pending(&record)?;
        let published = self.journal.commit_and_complete(&mut self.store, &record)?;
        self.emit_published(&published);
        Ok(published)
    }

    fn publish_media_post(
        &mut self,
        operation_id: &str,
        media_id: &str,
        caption: &str,
    ) -> Result<PublishedObject, ServiceError> {
        validate_operation(operation_id)?;
        if media_id.is_empty()
            || media_id.len() > MAX_OBJECT_ID_BYTES
            || caption.len() > MAX_POST_BYTES
        {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "media caption post exceeds limits",
            ));
        }
        let media = ObjectId::parse(media_id).map_err(storage_or_social)?;
        let input = MutationInput::MediaPost {
            media_id: media.as_str().into(),
            caption: caption.into(),
        };
        if let Some(existing) = self.journal.existing(operation_id, &input)? {
            return self.finish_existing(existing, ObjectKind::MediaPost);
        }
        self.ensure_signing_available()?;
        let header = self.store.get(&media).map_err(storage_error)?;
        if !matches!(&header.payload, Payload::Public(_)) {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "media sharing requires an existing public header",
            ));
        }
        match &header.object_type {
            kind if *kind == ObjectType::MEDIA_MANIFEST => {
                mini_media::read_manifest(&header).map_err(storage_or_social)?;
                if header.links.iter().any(|link| link.rel != "chunk") {
                    return Err(ServiceError::new(
                        ErrorCode::BadRequest,
                        "noncanonical media manifest links",
                    ));
                }
            }
            ObjectType::Custom(name) if name == mini_media::SUPERBLOCK_TYPE => {
                mini_media::read_superblock(&header).map_err(storage_or_social)?;
                if header.links.iter().any(|link| link.rel != "part") {
                    return Err(ServiceError::new(
                        ErrorCode::BadRequest,
                        "noncanonical superblock links",
                    ));
                }
            }
            ObjectType::Custom(name) if name == mini_media::COLLECTION_TYPE => {
                mini_media::read_collection(&header).map_err(storage_or_social)?;
                if header.links.iter().any(|link| link.rel != "part") {
                    return Err(ServiceError::new(
                        ErrorCode::BadRequest,
                        "noncanonical collection links",
                    ));
                }
            }
            _ => {
                return Err(ServiceError::new(
                    ErrorCode::BadRequest,
                    "unsupported media header type",
                ))
            }
        }
        let sequence = self.reserve_sequence()?;
        let (human, device) = self.signing_identity()?;
        let object =
            mini_social::build_media_post(human, device, media, caption, now_ms(), sequence)
                .map_err(storage_or_social)?;
        let record = PendingMutation {
            operation_id: operation_id.into(),
            input,
            objects: vec![object.to_bytes()],
        };
        self.journal.write_pending(&record)?;
        let published = self.journal.commit_and_complete(&mut self.store, &record)?;
        self.emit_published(&published);
        Ok(published)
    }

    fn publish_wall(
        &mut self,
        operation_id: &str,
        draft: PublicWallDraft,
    ) -> Result<PublishedObject, ServiceError> {
        validate_operation(operation_id)?;
        draft.validate().map_err(storage_or_social)?;
        let input = MutationInput::Wall {
            draft: draft.clone(),
        };
        if let Some(existing) = self.journal.existing(operation_id, &input)? {
            return self.finish_existing(existing, ObjectKind::Wall);
        }
        self.ensure_signing_available()?;
        let sequence = self.reserve_sequence()?;
        let (owner, device) = self.signing_identity()?;
        let links = draft
            .public_links
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let visibility = match draft.visibility {
            mini_app_protocol::WallVisibility::Public => mini_social::VisibilityPolicy::Public,
            mini_app_protocol::WallVisibility::Unlisted => mini_social::VisibilityPolicy::Unlisted,
        };
        let (wall, head) = mini_social::build_wall(
            owner,
            device,
            &draft.display_name,
            &draft.bio,
            None,
            &links,
            &[],
            visibility,
            now_ms(),
            sequence,
        )
        .map_err(storage_or_social)?;
        let record = PendingMutation {
            operation_id: operation_id.into(),
            input,
            objects: vec![wall.to_bytes(), head.to_bytes()],
        };
        self.journal.write_pending(&record)?;
        let published = self.journal.commit_and_complete(&mut self.store, &record)?;
        self.emit_published(&published);
        Ok(published)
    }

    fn publish_profile_with_photo(
        &mut self,
        operation_id: &str,
        draft: PublicProfileDetails,
        photo: &[u8],
    ) -> Result<PublishedObject, ServiceError> {
        validate_operation(operation_id)?;
        draft.validate().map_err(storage_or_social)?;
        if draft.avatar.is_some()
            || photo.is_empty()
            || photo.len() > mini_app_protocol::MAX_PROFILE_PHOTO_BYTES
        {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "photo profile requires a bounded new JPEG and no existing avatar",
            ));
        }
        let input = MutationInput::PhotoProfile {
            draft: draft.clone(),
            photo_digest: profile_photo::digest(photo),
            photo_len: photo.len() as u32,
        };
        if let Some(existing) = self.journal.existing(operation_id, &input)? {
            return self.finish_existing(existing, ObjectKind::Profile);
        }
        self.ensure_signing_available()?;
        profile_photo::validate(photo)?;
        let base = self.reserve_sequences(2)?;
        let (human, device) = self.signing_identity()?;
        let objects = profile_photo::build(human, device, &draft, photo, base, now_ms())?;
        let record = PendingMutation {
            operation_id: operation_id.into(),
            input,
            objects,
        };
        self.journal.write_pending(&record)?;
        let published = self.journal.commit_and_complete(&mut self.store, &record)?;
        self.emit_published(&published);
        Ok(published)
    }

    fn publish_profile_details(
        &mut self,
        operation_id: &str,
        draft: PublicProfileDetails,
    ) -> Result<PublishedObject, ServiceError> {
        validate_operation(operation_id)?;
        draft
            .validate()
            .map_err(|error| ServiceError::new(ErrorCode::BadRequest, error.to_string()))?;
        let (avatar, fields) = profile_details_values(&draft)?;
        let input = MutationInput::ProfileDetails {
            draft: draft.clone(),
        };
        if let Some(existing) = self.journal.existing(operation_id, &input)? {
            return self.finish_existing(existing, ObjectKind::Profile);
        }
        self.ensure_signing_available()?;
        if let Some(id) = &avatar {
            let object = self.store.get(id).map_err(storage_error)?;
            let manifest = mini_media::read_manifest(&object).map_err(storage_or_social)?;
            if !matches!(
                manifest.content_type.as_str(),
                "image/png" | "image/jpeg" | "image/webp" | "image/gif"
            ) || manifest.total_len > 8 * 1024 * 1024
                || manifest.chunks.len() > 8
                || object.links.iter().any(|link| link.rel != "chunk")
            {
                return Err(ServiceError::new(
                    ErrorCode::BadRequest,
                    "avatar must be an existing public image manifest",
                ));
            }
        }
        let public = mini_social::PublicProfileDraft {
            display_name: &draft.display_name,
            bio: &draft.bio,
            avatar: avatar.as_ref(),
            location: draft.location.as_deref(),
            age: draft.age,
            fields: &fields,
        };
        let sequence = self.reserve_sequence()?;
        let (human, device) = self.signing_identity()?;
        let (profile, head) = build_profile_details(human, device, &public, now_ms(), sequence)
            .map_err(storage_or_social)?;
        let record = PendingMutation {
            operation_id: operation_id.into(),
            input,
            objects: vec![profile.to_bytes(), head.to_bytes()],
        };
        self.journal.write_pending(&record)?;
        let published = self.journal.commit_and_complete(&mut self.store, &record)?;
        self.emit_published(&published);
        Ok(published)
    }

    fn publish_community(
        &mut self,
        operation_id: &str,
        name: &str,
        charter: &str,
    ) -> Result<PublishedObject, ServiceError> {
        validate_operation(operation_id)?;
        if name.len() > MAX_COMMUNITY_NAME_BYTES
            || name.trim().is_empty()
            || charter.len() > MAX_COMMUNITY_CHARTER_BYTES
        {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "community name or charter is empty or exceeds limits",
            ));
        }
        let input = MutationInput::Community {
            name: name.into(),
            charter: charter.into(),
        };
        if let Some(existing) = self.journal.existing(operation_id, &input)? {
            return self.finish_existing(existing, ObjectKind::Community);
        }
        self.ensure_signing_available()?;
        let sequence = self.reserve_sequence()?;
        let (owner, device) = self.signing_identity()?;
        let object = build_community(
            owner,
            device,
            name,
            charter,
            mini_social::MembershipMode::Open,
            now_ms(),
            sequence,
        )
        .map_err(storage_or_social)?;
        let record = PendingMutation {
            operation_id: operation_id.into(),
            input,
            objects: vec![object.to_bytes()],
        };
        self.journal.write_pending(&record)?;
        let published = self.journal.commit_and_complete(&mut self.store, &record)?;
        self.emit_published(&published);
        Ok(published)
    }

    fn set_membership(
        &mut self,
        operation_id: &str,
        community_id: &str,
        joined: bool,
    ) -> Result<PublishedObject, ServiceError> {
        validate_operation(operation_id)?;
        if community_id.is_empty() || community_id.len() > MAX_OBJECT_ID_BYTES {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "membership community ID exceeds limits",
            ));
        }
        let community = ObjectId::parse(community_id)
            .map_err(|error| ServiceError::new(ErrorCode::BadRequest, error.to_string()))?;
        let input = MutationInput::Membership {
            community_id: community.as_str().into(),
            joined,
        };
        if let Some(existing) = self.journal.existing(operation_id, &input)? {
            return self.finish_existing(existing, ObjectKind::Membership);
        }
        self.ensure_signing_available()?;
        let card =
            mini_social::resolve_community(&self.store, &community).map_err(storage_or_social)?;
        if joined && card.membership != mini_social::MembershipMode::Open {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "community requires approval; publishing a join is not an admission decision",
            ));
        }
        let sequence = self.reserve_sequence()?;
        let (member, device) = self.signing_identity()?;
        let object = build_membership(member, device, &community, joined, now_ms(), sequence)
            .map_err(storage_or_social)?;
        let record = PendingMutation {
            operation_id: operation_id.into(),
            input,
            objects: vec![object.to_bytes()],
        };
        self.journal.write_pending(&record)?;
        let published = self.journal.commit_and_complete(&mut self.store, &record)?;
        self.emit_published(&published);
        Ok(published)
    }

    fn publish_comment(
        &mut self,
        operation_id: &str,
        parent_id: &str,
        text: &str,
    ) -> Result<PublishedObject, ServiceError> {
        validate_operation(operation_id)?;
        if parent_id.is_empty()
            || parent_id.len() > MAX_OBJECT_ID_BYTES
            || text.len() > MAX_COMMENT_BYTES
            || text.trim().is_empty()
        {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "comment parent or text exceeds limits or is empty",
            ));
        }
        let parent = ObjectId::parse(parent_id)
            .map_err(|error| ServiceError::new(ErrorCode::BadRequest, error.to_string()))?;
        let input = MutationInput::Comment {
            parent_id: parent.as_str().into(),
            text: text.into(),
        };
        if let Some(existing) = self.journal.existing(operation_id, &input)? {
            return self.finish_existing(existing, ObjectKind::Comment);
        }
        self.ensure_signing_available()?;
        let content = self.store.get(&parent).map_err(storage_error)?;
        if !matches!(&content.payload, Payload::Public(_)) {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "comments require an existing public parent",
            ));
        }
        let sequence = self.reserve_sequence()?;
        let (human, device) = self.signing_identity()?;
        let object = build_comment(human, device, &parent, text, now_ms(), sequence)
            .map_err(storage_or_social)?;
        let record = PendingMutation {
            operation_id: operation_id.into(),
            input,
            objects: vec![object.to_bytes()],
        };
        self.journal.write_pending(&record)?;
        let published = self.journal.commit_and_complete(&mut self.store, &record)?;
        self.emit_published(&published);
        Ok(published)
    }

    fn set_like(
        &mut self,
        operation_id: &str,
        target_id: &str,
        active: bool,
    ) -> Result<PublishedObject, ServiceError> {
        validate_operation(operation_id)?;
        if target_id.is_empty() || target_id.len() > MAX_OBJECT_ID_BYTES {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "like target ID exceeds limits",
            ));
        }
        let target = ObjectId::parse(target_id)
            .map_err(|error| ServiceError::new(ErrorCode::BadRequest, error.to_string()))?;
        let input = MutationInput::Like {
            target_id: target.as_str().into(),
            active,
        };
        if let Some(existing) = self.journal.existing(operation_id, &input)? {
            return self.finish_existing(existing, ObjectKind::Reaction);
        }
        self.ensure_signing_available()?;
        let content = self.store.get(&target).map_err(storage_error)?;
        if !matches!(content.object_type, ObjectType::POST | ObjectType::COMMENT)
            || !matches!(&content.payload, Payload::Public(_))
        {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "likes require an existing public post or comment",
            ));
        }
        let sequence = self.reserve_sequence()?;
        let (human, device) = self.signing_identity()?;
        let object = build_reaction(
            human,
            device,
            &target,
            mini_social::ReactionKind::Like,
            active,
            now_ms(),
            sequence,
        )
        .map_err(storage_or_social)?;
        let record = PendingMutation {
            operation_id: operation_id.into(),
            input,
            objects: vec![object.to_bytes()],
        };
        self.journal.write_pending(&record)?;
        let published = self.journal.commit_and_complete(&mut self.store, &record)?;
        self.emit_published(&published);
        Ok(published)
    }

    fn set_follow(
        &mut self,
        operation_id: &str,
        target_did: &str,
        following: bool,
    ) -> Result<PublishedObject, ServiceError> {
        validate_operation(operation_id)?;
        if target_did.is_empty() || target_did.len() > MAX_DID_BYTES {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "follow target DID exceeds limits",
            ));
        }
        let target = Did::parse(target_did)
            .map_err(|error| ServiceError::new(ErrorCode::BadRequest, error.to_string()))?;
        let input = MutationInput::Follow {
            target_did: target.as_str().to_string(),
            following,
        };
        if let Some(existing) = self.journal.existing(operation_id, &input)? {
            return self.finish_existing(existing, ObjectKind::Follow);
        }
        self.ensure_signing_available()?;
        let sequence = self.reserve_sequence()?;
        let (human, device) = self.signing_identity()?;
        let object = build_follow(human, device, &target, following, now_ms(), sequence)
            .map_err(storage_or_social)?;
        let record = PendingMutation {
            operation_id: operation_id.to_string(),
            input,
            objects: vec![object.to_bytes()],
        };
        self.journal.write_pending(&record)?;
        let published = self.journal.commit_and_complete(&mut self.store, &record)?;
        self.emit_published(&published);
        Ok(published)
    }

    fn publish_profile(
        &mut self,
        operation_id: &str,
        display_name: &str,
        bio: &str,
    ) -> Result<PublishedObject, ServiceError> {
        validate_operation(operation_id)?;
        if display_name.is_empty()
            || display_name.len() > MAX_PROFILE_NAME_BYTES
            || bio.len() > MAX_PROFILE_BIO_BYTES
        {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "profile fields exceed the protocol limits",
            ));
        }
        let input = MutationInput::Profile {
            display_name: display_name.to_string(),
            bio: bio.to_string(),
        };
        if let Some(existing) = self.journal.existing(operation_id, &input)? {
            return self.finish_existing(existing, ObjectKind::Profile);
        }

        self.ensure_signing_available()?;
        let sequence = self.reserve_sequence()?;
        let (human, device) = self.signing_identity()?;
        let (profile, head) =
            build_profile(human, device, display_name, bio, None, now_ms(), sequence)
                .map_err(storage_or_social)?;
        let record = PendingMutation {
            operation_id: operation_id.to_string(),
            input,
            objects: vec![profile.to_bytes(), head.to_bytes()],
        };
        self.journal.write_pending(&record)?;
        let published = self.journal.commit_and_complete(&mut self.store, &record)?;
        self.emit_published(&published);
        Ok(published)
    }

    fn finish_existing(
        &mut self,
        existing: ExistingMutation,
        expected_kind: ObjectKind,
    ) -> Result<PublishedObject, ServiceError> {
        let mut published = match existing {
            ExistingMutation::Complete(receipt) => PublishedObject {
                kind: receipt.kind,
                object_id: receipt.object_id,
                duplicate: true,
            },
            ExistingMutation::Pending(record) => {
                let mut published = self.journal.commit_and_complete(&mut self.store, &record)?;
                published.duplicate = true;
                published
            }
        };
        if published.kind != expected_kind {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "operation id was already used for another mutation kind",
            ));
        }
        published.duplicate = true;
        Ok(published)
    }

    fn ensure_signing_available(&self) -> Result<(), ServiceError> {
        if self.human.is_none() {
            return Err(ServiceError::new(
                ErrorCode::RootMissing,
                "create a Mininet root first",
            ));
        }
        if self.identity.is_none() {
            return Err(ServiceError::new(
                ErrorCode::IdentityLocked,
                "identity is locked",
            ));
        }
        Ok(())
    }

    fn signing_identity(&self) -> Result<(&Did, &Controller), ServiceError> {
        self.ensure_signing_available()?;
        let human = self.human.as_ref().expect("checked above");
        let identity = self.identity.as_ref().expect("checked above");
        Ok((human, &identity.device))
    }

    fn reserve_sequence(&mut self) -> Result<u64, ServiceError> {
        self.reserve_sequences(1)
    }
    fn reserve_sequences(&mut self, count: u64) -> Result<u64, ServiceError> {
        if count == 0 {
            return Err(ServiceError::new(
                ErrorCode::BadRequest,
                "empty sequence reservation",
            ));
        }
        let store_next = if let Some(human) = self.human.as_ref() {
            next_object_sequence(&self.store, Some(human))?
        } else {
            0
        };
        let sequence = self.next_sequence.max(store_next);
        let next = sequence.checked_add(count).ok_or_else(|| {
            ServiceError::new(
                ErrorCode::Storage,
                "local object sequence space is exhausted",
            )
        })?;
        self.journal.persist_next_sequence(next)?;
        self.next_sequence = next;
        Ok(sequence)
    }

    fn feed_snapshot(
        &self,
        order: FeedOrder,
        scope: FeedScope,
        limit: usize,
    ) -> Result<Vec<FeedCard>, ServiceError> {
        let human = self.human.as_ref().ok_or_else(|| {
            ServiceError::new(ErrorCode::RootMissing, "create a Mininet root first")
        })?;
        let filter = match order {
            FeedOrder::Chronological => FeedFilter::Chronological,
            FeedOrder::MostSupported => FeedFilter::MostSupported,
        };

        let seeds = match scope {
            FeedScope::Following => feed(&self.store, human, filter, limit)
                .map_err(storage_or_social)?
                .into_iter()
                .map(|item| FeedSeed {
                    id: item.id,
                    author: item.author,
                    timestamp_ms: item.timestamp_ms,
                    reason: match item.reason {
                        SocialFeedReason::Own => FeedReason::Own,
                        SocialFeedReason::Followed => FeedReason::Followed,
                    },
                    support_count: item.support_count,
                })
                .collect::<Vec<_>>(),
            FeedScope::Everyone => {
                let followed = following(&self.store, human).map_err(storage_or_social)?;
                let mut seeds = Vec::new();
                for id in self
                    .store
                    .by_type(&ObjectType::POST)
                    .map_err(storage_error)?
                {
                    let Ok(post) = resolve_post(&self.store, &id) else {
                        continue;
                    };
                    let support_count = reaction_counts(&self.store, &id)
                        .map_err(storage_or_social)?
                        .into_iter()
                        .map(|(_, count)| count)
                        .sum();
                    let reason = if &post.author == human {
                        FeedReason::Own
                    } else if followed.contains(&post.author) {
                        FeedReason::Followed
                    } else {
                        FeedReason::Received
                    };
                    seeds.push(FeedSeed {
                        id,
                        author: post.author,
                        timestamp_ms: post.timestamp_ms,
                        reason,
                        support_count,
                    });
                }
                match order {
                    FeedOrder::Chronological => seeds.sort_by(|left, right| {
                        right
                            .timestamp_ms
                            .cmp(&left.timestamp_ms)
                            .then_with(|| right.id.as_str().cmp(left.id.as_str()))
                    }),
                    FeedOrder::MostSupported => seeds.sort_by(|left, right| {
                        right
                            .support_count
                            .cmp(&left.support_count)
                            .then_with(|| right.timestamp_ms.cmp(&left.timestamp_ms))
                            .then_with(|| right.id.as_str().cmp(left.id.as_str()))
                    }),
                }
                seeds.truncate(limit);
                seeds
            }
        };

        let mut profiles: HashMap<String, (String, Option<String>)> = HashMap::new();
        seeds
            .into_iter()
            .map(|seed| {
                let did = seed.author.as_str().to_string();
                let (author, avatar) = if let Some(cached) = profiles.get(&did) {
                    cached.clone()
                } else {
                    let profile =
                        resolve_profile(&self.store, &seed.author).map_err(storage_or_social)?;
                    let cached = profile
                        .map(|profile| {
                            (
                                profile.display_name,
                                profile.avatar.map(|id| id.as_str().to_string()),
                            )
                        })
                        .unwrap_or_else(|| ("Mininet participant".to_string(), None));
                    profiles.insert(did.clone(), cached.clone());
                    cached
                };
                let post = resolve_post(&self.store, &seed.id).map_err(storage_or_social)?;
                let comment_count = comments(&self.store, &seed.id)
                    .map_err(storage_or_social)?
                    .len();
                Ok(FeedCard {
                    id: seed.id.as_str().to_string(),
                    author,
                    did,
                    body: post.text,
                    timestamp_ms: seed.timestamp_ms,
                    reason: seed.reason,
                    support_count: u32::try_from(seed.support_count).unwrap_or(u32::MAX),
                    comment_count: u32::try_from(comment_count).unwrap_or(u32::MAX),
                    media: match post.kind {
                        PostKind::Media { media } => Some(media.as_str().to_string()),
                        _ => None,
                    },
                    own: &post.author == human,
                    avatar,
                })
            })
            .collect()
    }

    fn emit_published(&mut self, published: &PublishedObject) {
        self.emit(ServiceEventKind::ObjectPublished {
            kind: published.kind,
            object_id: published.object_id.clone(),
        });
        self.emit(ServiceEventKind::FeedChanged);
    }

    fn emit(&mut self, kind: ServiceEventKind) {
        let event = ServiceEvent {
            event_id: self.next_event_id,
            kind,
        };
        self.next_event_id = self.next_event_id.saturating_add(1);
        if self.events.len() == EVENT_BACKLOG {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    fn drain_events(&mut self, limit: usize) -> Vec<ServiceEvent> {
        let count = limit.min(self.events.len());
        self.events.drain(..count).collect()
    }
}

#[derive(Debug)]
struct FeedSeed {
    id: ObjectId,
    author: Did,
    timestamp_ms: u64,
    reason: FeedReason,
    support_count: usize,
}

fn profile_view(profile: mini_social::Profile) -> Result<ProfileView, ServiceError> {
    Ok(ProfileView {
        did: profile.human.as_str().to_string(),
        display_name: profile.display_name,
        bio: profile.bio,
        avatar: profile.avatar.map(|id| id.as_str().to_string()),
    })
}

fn delivery_view(record: &outbox::DeliveryRecord) -> mini_app_protocol::DeliveryView {
    use mini_app_protocol::{DeliveryFailure as F, DeliveryState as S};
    mini_app_protocol::DeliveryView {
        operation_id: record.operation_id.clone(),
        peer_id: record.peer_id.clone(),
        object_id: record.object_id.clone(),
        state: match record.state {
            outbox::DeliveryState::Queued => S::Queued,
            outbox::DeliveryState::InFlight => S::InFlight,
            outbox::DeliveryState::RetryWaiting => S::RetryWaiting,
            outbox::DeliveryState::Delivered => S::Delivered,
            outbox::DeliveryState::Cancelled => S::Cancelled,
        },
        attempts: record.attempts,
        next_attempt_ms: record.next_attempt_ms,
        last_failure: record.last_failure.map(|failure| match failure {
            outbox::DeliveryFailure::Unavailable => F::Unavailable,
            outbox::DeliveryFailure::Rejected => F::Rejected,
            outbox::DeliveryFailure::Interrupted => F::Interrupted,
        }),
    }
}

fn root_from_seeds(seeds: &SeedPair) -> Result<Controller, ServiceError> {
    Controller::incept_single_from_seeds(&seeds.current, &seeds.next)
        .map_err(|error| ServiceError::new(ErrorCode::Identity, error.to_string()))
}

pub(crate) fn identity_from_seeds(
    root_seeds: &SeedPair,
    device_seeds: &SeedPair,
) -> Result<CoreIdentity, ServiceError> {
    let mut root = root_from_seeds(root_seeds)?;
    let device = Controller::incept_device_single_from_seeds(
        &root.did(),
        &device_seeds.current,
        &device_seeds.next,
    )
    .map_err(|error| ServiceError::new(ErrorCode::Identity, error.to_string()))?;
    root.delegate_device(&device.did(), Capabilities::primary())
        .map_err(|error| ServiceError::new(ErrorCode::Identity, error.to_string()))?;
    Ok(CoreIdentity { root, device })
}

pub(crate) fn next_object_sequence<B: Backend>(
    store: &Store<B>,
    author: Option<&Did>,
) -> Result<u64, ServiceError> {
    let Some(author) = author else {
        return Ok(1);
    };
    let mut maximum = 0u64;
    for id in store.all_ids().map_err(storage_error)? {
        let object = store.get(&id).map_err(storage_error)?;
        if &object.author_human == author {
            maximum = maximum.max(object.sequence);
        }
    }
    maximum.checked_add(1).ok_or_else(|| {
        ServiceError::new(
            ErrorCode::Storage,
            "local object sequence space is exhausted",
        )
    })
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn validate_operation(operation_id: &str) -> Result<(), ServiceError> {
    if operation_id.is_empty() || operation_id.len() > MAX_OPERATION_ID_BYTES {
        return Err(ServiceError::new(
            ErrorCode::BadRequest,
            "operation id is empty or too long",
        ));
    }
    if !operation_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(ServiceError::new(
            ErrorCode::BadRequest,
            "operation id contains unsupported characters",
        ));
    }
    Ok(())
}

fn storage_error(error: impl core::fmt::Display) -> ServiceError {
    ServiceError::new(ErrorCode::Storage, error.to_string())
}

fn storage_or_social(error: impl core::fmt::Display) -> ServiceError {
    ServiceError::new(ErrorCode::Storage, error.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MutationInput {
    MediaPost {
        media_id: String,
        caption: String,
    },
    Wall {
        draft: PublicWallDraft,
    },
    PhotoProfile {
        draft: PublicProfileDetails,
        photo_digest: [u8; 32],
        photo_len: u32,
    },
    ProfileDetails {
        draft: PublicProfileDetails,
    },
    Profile {
        display_name: String,
        bio: String,
    },
    Post {
        text: String,
    },
    Follow {
        target_did: String,
        following: bool,
    },
    Like {
        target_id: String,
        active: bool,
    },
    Comment {
        parent_id: String,
        text: String,
    },
    Membership {
        community_id: String,
        joined: bool,
    },
    Community {
        name: String,
        charter: String,
    },
}

impl MutationInput {
    fn kind(&self) -> ObjectKind {
        match self {
            Self::ProfileDetails { .. } | Self::PhotoProfile { .. } => ObjectKind::Profile,
            Self::Wall { .. } => ObjectKind::Wall,
            Self::Profile { .. } => ObjectKind::Profile,
            Self::Post { .. } => ObjectKind::Post,
            Self::MediaPost { .. } => ObjectKind::MediaPost,
            Self::Follow { .. } => ObjectKind::Follow,
            Self::Like { .. } => ObjectKind::Reaction,
            Self::Comment { .. } => ObjectKind::Comment,
            Self::Membership { .. } => ObjectKind::Membership,
            Self::Community { .. } => ObjectKind::Community,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingMutation {
    operation_id: String,
    input: MutationInput,
    objects: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MutationReceipt {
    operation_id: String,
    input: MutationInput,
    kind: ObjectKind,
    object_id: String,
}

#[derive(Debug)]
enum ExistingMutation {
    Complete(MutationReceipt),
    Pending(PendingMutation),
}

#[derive(Debug, Clone)]
struct MutationJournal {
    pending: PathBuf,
    receipts: PathBuf,
    sequence: PathBuf,
}

impl MutationJournal {
    fn open(root: PathBuf) -> Result<Self, ServiceError> {
        let pending = root.join("pending");
        let receipts = root.join("receipts");
        mini_durable::create_dir_all(&pending)
            .and_then(|_| mini_durable::create_dir_all(&receipts))
            .map_err(|error| ServiceError::new(ErrorCode::Io, error.to_string()))?;
        Ok(Self {
            sequence: root.join("sequence.state"),
            pending,
            receipts,
        })
    }

    fn ensure_sequence_floor(&self, floor: u64) -> Result<u64, ServiceError> {
        let current = match fs::read(&self.sequence) {
            Ok(bytes) => decode_sequence(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => floor,
            Err(error) => {
                return Err(ServiceError::new(ErrorCode::Io, error.to_string()));
            }
        };
        let next = current.max(floor);
        if next != current || !self.sequence.exists() {
            self.persist_next_sequence(next)?;
        }
        Ok(next)
    }

    fn persist_next_sequence(&self, next: u64) -> Result<(), ServiceError> {
        let mut bytes = Vec::with_capacity(SEQUENCE_MAGIC.len() + 1 + 8);
        bytes.extend_from_slice(SEQUENCE_MAGIC);
        bytes.push(JOURNAL_VERSION);
        bytes.extend_from_slice(&next.to_be_bytes());
        mini_durable::atomic_replace(&self.sequence, &bytes)
            .map_err(|error| ServiceError::new(ErrorCode::Io, error.to_string()))
    }

    fn existing(
        &self,
        operation_id: &str,
        input: &MutationInput,
    ) -> Result<Option<ExistingMutation>, ServiceError> {
        if let Some(receipt) = self.read_receipt(operation_id)? {
            if &receipt.input != input {
                return Err(ServiceError::new(
                    ErrorCode::BadRequest,
                    "operation id was already used with different content",
                ));
            }
            return Ok(Some(ExistingMutation::Complete(receipt)));
        }
        if let Some(pending) = self.read_pending(operation_id)? {
            if &pending.input != input {
                return Err(ServiceError::new(
                    ErrorCode::BadRequest,
                    "operation id has a pending mutation with different content",
                ));
            }
            return Ok(Some(ExistingMutation::Pending(pending)));
        }
        Ok(None)
    }

    fn write_pending(&self, record: &PendingMutation) -> Result<(), ServiceError> {
        validate_operation(&record.operation_id)?;
        let bytes = encode_pending(record)?;
        mini_durable::atomic_replace(&self.pending_path(&record.operation_id), &bytes)
            .map_err(|error| ServiceError::new(ErrorCode::Io, error.to_string()))
    }

    fn commit_and_complete(
        &self,
        store: &mut Store<FsBackend>,
        record: &PendingMutation,
    ) -> Result<PublishedObject, ServiceError> {
        let published = commit_record(store, record)?;
        let receipt = MutationReceipt {
            operation_id: record.operation_id.clone(),
            input: record.input.clone(),
            kind: published.kind,
            object_id: published.object_id.clone(),
        };
        self.write_receipt(&receipt)?;
        self.remove_pending(&record.operation_id)?;
        Ok(published)
    }

    fn recover(&self, store: &mut Store<FsBackend>) -> Result<(), ServiceError> {
        let mut entries = fs::read_dir(&self.pending)
            .map_err(|error| ServiceError::new(ErrorCode::Io, error.to_string()))?;
        let mut count = 0usize;
        while let Some(entry) = entries
            .next()
            .transpose()
            .map_err(|error| ServiceError::new(ErrorCode::Io, error.to_string()))?
        {
            if !entry
                .file_type()
                .map_err(|error| ServiceError::new(ErrorCode::Io, error.to_string()))?
                .is_file()
            {
                continue;
            }
            let path = entry.path();
            let file_name = entry.file_name();
            let file_name = file_name.to_str().ok_or_else(|| {
                ServiceError::new(
                    ErrorCode::Storage,
                    "pending mutation filename is not valid UTF-8",
                )
            })?;
            if file_name.ends_with(".tmp") {
                mini_durable::remove_file(&path)
                    .map_err(|error| ServiceError::new(ErrorCode::Io, error.to_string()))?;
                continue;
            }
            if !file_name.ends_with(".op") {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "unknown file in application pending-mutation directory",
                ));
            }
            count = count.saturating_add(1);
            if count > MAX_PENDING_OPERATIONS {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "too many pending application mutations; refusing unbounded recovery",
                ));
            }
            let record = read_bounded(&path).and_then(|bytes| decode_pending(&bytes))?;
            let expected_path = self.pending_path(&record.operation_id);
            if path != expected_path {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "pending mutation filename does not match its operation id",
                ));
            }
            if let Some(receipt) = self.read_receipt(&record.operation_id)? {
                if receipt.input != record.input {
                    return Err(ServiceError::new(
                        ErrorCode::Storage,
                        "pending mutation disagrees with its durable receipt",
                    ));
                }
                self.remove_pending(&record.operation_id)?;
                continue;
            }
            let _ = self.commit_and_complete(store, &record)?;
        }
        Ok(())
    }

    fn write_receipt(&self, receipt: &MutationReceipt) -> Result<(), ServiceError> {
        let bytes = encode_receipt(receipt)?;
        mini_durable::atomic_replace(&self.receipt_path(&receipt.operation_id), &bytes)
            .map_err(|error| ServiceError::new(ErrorCode::Io, error.to_string()))
    }

    fn read_pending(&self, operation_id: &str) -> Result<Option<PendingMutation>, ServiceError> {
        read_optional(&self.pending_path(operation_id))?
            .map(|bytes| decode_pending(&bytes))
            .transpose()
    }

    fn read_receipt(&self, operation_id: &str) -> Result<Option<MutationReceipt>, ServiceError> {
        read_optional(&self.receipt_path(operation_id))?
            .map(|bytes| decode_receipt(&bytes))
            .transpose()
    }

    fn remove_pending(&self, operation_id: &str) -> Result<(), ServiceError> {
        let path = self.pending_path(operation_id);
        match mini_durable::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ServiceError::new(ErrorCode::Io, error.to_string())),
        }
    }

    fn pending_path(&self, operation_id: &str) -> PathBuf {
        self.pending
            .join(format!("{}.op", hex_encode(operation_id.as_bytes())))
    }

    fn receipt_path(&self, operation_id: &str) -> PathBuf {
        self.receipts
            .join(format!("{}.receipt", hex_encode(operation_id.as_bytes())))
    }
}

fn profile_details_values(
    draft: &PublicProfileDetails,
) -> Result<(Option<ObjectId>, Vec<mini_social::PublicProfileField>), ServiceError> {
    draft.validate().map_err(storage_or_social)?;
    let avatar = draft
        .avatar
        .as_deref()
        .map(ObjectId::parse)
        .transpose()
        .map_err(storage_or_social)?;
    let fields = draft
        .fields
        .iter()
        .map(|field| mini_social::PublicProfileField {
            label: field.label.clone(),
            value: field.value.clone(),
        })
        .collect();
    Ok((avatar, fields))
}

fn validate_profile_details_pair(
    profile: &Object,
    head: &Object,
    draft: &PublicProfileDetails,
) -> Result<(), ServiceError> {
    let (avatar, fields) = profile_details_values(draft)?;
    let decoded = mini_social::decode_profile(profile).map_err(storage_or_social)?;
    if !profile.links.is_empty()
        || !matches!(&profile.payload,Payload::Public(bytes) if bytes.starts_with(b"MINIPROF2"))
        || head.object_type != ObjectType::HEAD
        || head.links.len() != 1
        || head.links[0].rel != "target"
        || head.links[0].target != *profile.id()
        || !matches!(&head.payload,Payload::Public(bytes) if bytes==b"profile")
        || profile.author_human != head.author_human
        || profile.author_device != head.author_device
        || profile.sequence != head.sequence
        || profile.timestamp_ms != head.timestamp_ms
        || decoded.display_name != draft.display_name
        || decoded.bio != draft.bio
        || decoded.avatar != avatar
        || decoded.location != draft.location
        || decoded.age != draft.age
        || decoded.fields != fields
    {
        return Err(ServiceError::new(
            ErrorCode::Storage,
            "profile details journal does not match exact draft and head",
        ));
    }
    Ok(())
}

fn commit_record(
    store: &mut Store<FsBackend>,
    record: &PendingMutation,
) -> Result<PublishedObject, ServiceError> {
    match &record.input {
        MutationInput::MediaPost { media_id, caption } => {
            if record.objects.len() != 1 {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "media post journal has wrong object count",
                ));
            }
            let object = Object::from_bytes(&record.objects[0]).map_err(storage_error)?;
            let post = mini_social::decode_post(&object).map_err(storage_or_social)?;
            if object.to_bytes() != record.objects[0]
                || post.text != *caption
                || !matches!(&post.kind,mini_social::PostKind::Media{media} if media.as_str()==media_id)
                || object.links.len() != 1
                || object.links[0].rel != "media"
            {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "media post journal differs from exact target and caption",
                ));
            }
            store.insert(&object).map_err(storage_error)?;
            Ok(PublishedObject {
                kind: ObjectKind::MediaPost,
                object_id: object.id().as_str().into(),
                duplicate: false,
            })
        }
        MutationInput::Wall { draft } => {
            draft.validate().map_err(storage_or_social)?;
            if record.objects.len() != 2 {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "wall journal has wrong object count",
                ));
            }
            let wall = Object::from_bytes(&record.objects[0]).map_err(storage_error)?;
            let head = Object::from_bytes(&record.objects[1]).map_err(storage_error)?;
            let decoded = mini_social::decode_wall(&wall).map_err(storage_or_social)?;
            let visibility = match draft.visibility {
                mini_app_protocol::WallVisibility::Public => mini_social::VisibilityPolicy::Public,
                mini_app_protocol::WallVisibility::Unlisted => {
                    mini_social::VisibilityPolicy::Unlisted
                }
            };
            if wall.to_bytes() != record.objects[0]
                || head.to_bytes() != record.objects[1]
                || decoded.display_name != draft.display_name
                || decoded.bio != draft.bio
                || decoded.public_links != draft.public_links
                || decoded.visibility != visibility
                || decoded.avatar.is_some()
                || !decoded.pinned.is_empty()
                || head.object_type != ObjectType::HEAD
                || head.links.len() != 1
                || head.links[0].rel != "target"
                || head.links[0].target != *wall.id()
                || !matches!(&head.payload,Payload::Public(bytes) if bytes.as_slice()==b"wall")
                || wall.author_human != head.author_human
                || wall.author_device != head.author_device
                || wall.sequence != head.sequence
                || wall.timestamp_ms != head.timestamp_ms
            {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "wall journal differs from exact public draft and wall head",
                ));
            }
            store.insert(&wall).map_err(storage_error)?;
            store.apply_head(&head).map_err(storage_error)?;
            Ok(PublishedObject {
                kind: ObjectKind::Wall,
                object_id: wall.id().as_str().into(),
                duplicate: false,
            })
        }
        MutationInput::PhotoProfile {
            draft,
            photo_digest,
            photo_len,
        } => {
            let objects =
                profile_photo::validate_record(draft, photo_digest, *photo_len, &record.objects)?;
            for object in &objects[..3] {
                store.insert(object).map_err(storage_error)?;
            }
            store.apply_head(&objects[3]).map_err(storage_error)?;
            Ok(PublishedObject {
                kind: ObjectKind::Profile,
                object_id: objects[2].id().as_str().into(),
                duplicate: false,
            })
        }
        MutationInput::ProfileDetails { draft } => {
            if record.objects.len() != 2 {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "profile details journal has wrong object count",
                ));
            }
            let profile = Object::from_bytes(&record.objects[0]).map_err(storage_error)?;
            let head = Object::from_bytes(&record.objects[1]).map_err(storage_error)?;
            if profile.to_bytes() != record.objects[0] || head.to_bytes() != record.objects[1] {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "noncanonical profile objects",
                ));
            }
            validate_profile_details_pair(&profile, &head, draft)?;
            store.insert(&profile).map_err(storage_error)?;
            store.apply_head(&head).map_err(storage_error)?;
            Ok(PublishedObject {
                kind: ObjectKind::Profile,
                object_id: profile.id().as_str().into(),
                duplicate: false,
            })
        }
        MutationInput::Community { name, charter } => {
            if record.objects.len() != 1 {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "community journal has wrong object count",
                ));
            }
            let object = Object::from_bytes(&record.objects[0]).map_err(storage_error)?;
            let card = mini_social::decode_community(&object).map_err(storage_or_social)?;
            if object.to_bytes() != record.objects[0]
                || card.name != *name
                || card.charter != *charter
                || name.trim().is_empty()
                || card.membership != mini_social::MembershipMode::Open
            {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "community journal does not match open creation input",
                ));
            }
            store.insert(&object).map_err(storage_error)?;
            Ok(PublishedObject {
                kind: ObjectKind::Community,
                object_id: object.id().as_str().into(),
                duplicate: false,
            })
        }
        MutationInput::Membership {
            community_id,
            joined,
        } => {
            if record.objects.len() != 1 {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "membership journal has wrong object count",
                ));
            }
            let object = Object::from_bytes(&record.objects[0]).map_err(storage_error)?;
            let (target, state) =
                mini_social::decode_membership(&object).map_err(storage_or_social)?;
            if object.to_bytes() != record.objects[0]
                || target.as_str() != community_id
                || state != *joined
            {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "membership journal does not match mutation input",
                ));
            }
            store.insert(&object).map_err(storage_error)?;
            Ok(PublishedObject {
                kind: ObjectKind::Membership,
                object_id: object.id().as_str().into(),
                duplicate: false,
            })
        }
        MutationInput::Comment { parent_id, text } => {
            if record.objects.len() != 1 {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "comment journal has wrong object count",
                ));
            }
            let object = Object::from_bytes(&record.objects[0]).map_err(storage_error)?;
            let comment = mini_social::decode_comment(&object).map_err(storage_or_social)?;
            if object.to_bytes() != record.objects[0]
                || comment.parent.as_str() != parent_id
                || comment.text != *text
                || text.trim().is_empty()
            {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "comment journal does not match mutation input",
                ));
            }
            store.insert(&object).map_err(storage_error)?;
            Ok(PublishedObject {
                kind: ObjectKind::Comment,
                object_id: object.id().as_str().into(),
                duplicate: false,
            })
        }
        MutationInput::Like { target_id, active } => {
            if record.objects.len() != 1 {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "like journal has wrong object count",
                ));
            }
            let object = Object::from_bytes(&record.objects[0]).map_err(storage_error)?;
            let (target, kind, state) =
                mini_social::decode_reaction(&object).map_err(storage_or_social)?;
            if object.to_bytes() != record.objects[0]
                || target.as_str() != target_id
                || kind != mini_social::ReactionKind::Like
                || state != *active
            {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "like journal does not match mutation input",
                ));
            }
            store.insert(&object).map_err(storage_error)?;
            Ok(PublishedObject {
                kind: ObjectKind::Reaction,
                object_id: object.id().as_str().into(),
                duplicate: false,
            })
        }
        MutationInput::Follow {
            target_did,
            following,
        } => {
            if record.objects.len() != 1 {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "follow journal has wrong object count",
                ));
            }
            let object = Object::from_bytes(&record.objects[0]).map_err(storage_error)?;
            let (action, target) =
                mini_social::decode_follow(&object).map_err(storage_or_social)?;
            if object.to_bytes() != record.objects[0]
                || action != *following
                || target.as_str() != target_did
            {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "follow journal does not match mutation input",
                ));
            }
            store.insert(&object).map_err(storage_error)?;
            Ok(PublishedObject {
                kind: ObjectKind::Follow,
                object_id: object.id().as_str().into(),
                duplicate: false,
            })
        }
        MutationInput::Post { text } => {
            if record.objects.len() != 1 {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "post journal record has the wrong object count",
                ));
            }
            let post = Object::from_bytes(&record.objects[0]).map_err(storage_error)?;
            if post.object_type != ObjectType::POST {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "post journal record contains a non-post object",
                ));
            }
            let decoded = mini_social::decode_post(&post).map_err(storage_or_social)?;
            if decoded.text != *text {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "post journal content does not match its mutation input",
                ));
            }
            let object_id = post.id().as_str().to_string();
            store.insert(&post).map_err(storage_error)?;
            Ok(PublishedObject {
                kind: ObjectKind::Post,
                object_id,
                duplicate: false,
            })
        }
        MutationInput::Profile { display_name, bio } => {
            if record.objects.len() != 2 {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "profile journal record has the wrong object count",
                ));
            }
            let profile = Object::from_bytes(&record.objects[0]).map_err(storage_error)?;
            let head = Object::from_bytes(&record.objects[1]).map_err(storage_error)?;
            let decoded = mini_social::decode_profile(&profile).map_err(storage_or_social)?;
            let head_targets_profile = head.links.len() == 1
                && head.links[0].rel == "target"
                && &head.links[0].target == profile.id();
            let head_names_profile = matches!(
                &head.payload,
                Payload::Public(bytes) if bytes.as_slice() == b"profile"
            );
            if head.object_type != ObjectType::HEAD
                || profile.author_human != head.author_human
                || profile.sequence != head.sequence
                || !head_targets_profile
                || !head_names_profile
                || decoded.display_name.as_str() != display_name.as_str()
                || decoded.bio.as_str() != bio.as_str()
                || decoded.avatar.is_some()
                || decoded.location.is_some()
                || decoded.age.is_some()
                || !decoded.fields.is_empty()
            {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "profile journal does not match its requested mutation",
                ));
            }
            let object_id = profile.id().as_str().to_string();
            store.insert(&profile).map_err(storage_error)?;
            store.apply_head(&head).map_err(storage_error)?;
            Ok(PublishedObject {
                kind: ObjectKind::Profile,
                object_id,
                duplicate: false,
            })
        }
    }
}

fn encode_pending(record: &PendingMutation) -> Result<Vec<u8>, ServiceError> {
    let mut encoder = JournalEncoder::new(PENDING_MAGIC);
    encoder.string(&record.operation_id, MAX_OPERATION_ID_BYTES)?;
    encoder.input(&record.input)?;
    if record.objects.len()
        > if matches!(record.input, MutationInput::PhotoProfile { .. }) {
            4
        } else {
            2
        }
    {
        return Err(ServiceError::new(
            ErrorCode::Storage,
            "too many signed objects in one journal record",
        ));
    }
    encoder.u8(record.objects.len() as u8);
    for object in &record.objects {
        encoder.bytes(object, MAX_SIGNED_OBJECT_BYTES)?;
    }
    encoder.finish()
}

fn decode_pending(bytes: &[u8]) -> Result<PendingMutation, ServiceError> {
    let mut decoder = JournalDecoder::new(bytes, PENDING_MAGIC)?;
    let operation_id = decoder.string(MAX_OPERATION_ID_BYTES)?;
    validate_operation(&operation_id)?;
    let input = decoder.input()?;
    let count = usize::from(decoder.u8()?);
    if count
        > if matches!(input, MutationInput::PhotoProfile { .. }) {
            4
        } else {
            2
        }
    {
        return Err(ServiceError::new(
            ErrorCode::Storage,
            "journal object count exceeds the service limit",
        ));
    }
    let mut objects = Vec::with_capacity(count);
    for _ in 0..count {
        objects.push(decoder.bytes(MAX_SIGNED_OBJECT_BYTES)?);
    }
    decoder.finish()?;
    Ok(PendingMutation {
        operation_id,
        input,
        objects,
    })
}

fn encode_receipt(receipt: &MutationReceipt) -> Result<Vec<u8>, ServiceError> {
    let mut encoder = JournalEncoder::new(RECEIPT_MAGIC);
    encoder.string(&receipt.operation_id, MAX_OPERATION_ID_BYTES)?;
    encoder.input(&receipt.input)?;
    encoder.u8(match receipt.kind {
        ObjectKind::Profile => 0,
        ObjectKind::Post => 1,
        ObjectKind::Follow => 2,
        ObjectKind::Reaction => 3,
        ObjectKind::Comment => 4,
        ObjectKind::Membership => 5,
        ObjectKind::Community => 6,
        ObjectKind::Wall => 7,
        ObjectKind::MediaPost => 8,
    });
    encoder.string(&receipt.object_id, 256)?;
    encoder.finish()
}

fn decode_receipt(bytes: &[u8]) -> Result<MutationReceipt, ServiceError> {
    let mut decoder = JournalDecoder::new(bytes, RECEIPT_MAGIC)?;
    let operation_id = decoder.string(MAX_OPERATION_ID_BYTES)?;
    validate_operation(&operation_id)?;
    let input = decoder.input()?;
    let kind = match decoder.u8()? {
        0 => ObjectKind::Profile,
        1 => ObjectKind::Post,
        2 => ObjectKind::Follow,
        3 => ObjectKind::Reaction,
        4 => ObjectKind::Comment,
        5 => ObjectKind::Membership,
        6 => ObjectKind::Community,
        7 => ObjectKind::Wall,
        8 => ObjectKind::MediaPost,
        _ => {
            return Err(ServiceError::new(
                ErrorCode::Storage,
                "invalid receipt object kind",
            ))
        }
    };
    if input.kind() != kind {
        return Err(ServiceError::new(
            ErrorCode::Storage,
            "receipt mutation kind does not match its input",
        ));
    }
    let object_id = decoder.string(256)?;
    decoder.finish()?;
    Ok(MutationReceipt {
        operation_id,
        input,
        kind,
        object_id,
    })
}

fn decode_sequence(bytes: &[u8]) -> Result<u64, ServiceError> {
    if bytes.len() != SEQUENCE_MAGIC.len() + 1 + 8
        || &bytes[..SEQUENCE_MAGIC.len()] != SEQUENCE_MAGIC
        || bytes[SEQUENCE_MAGIC.len()] != JOURNAL_VERSION
    {
        return Err(ServiceError::new(
            ErrorCode::Storage,
            "invalid application sequence state",
        ));
    }
    let start = SEQUENCE_MAGIC.len() + 1;
    let value: [u8; 8] = bytes[start..]
        .try_into()
        .expect("validated sequence state length");
    Ok(u64::from_be_bytes(value))
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, ServiceError> {
    match fs::read(path) {
        Ok(bytes) => {
            if bytes.len() > MAX_JOURNAL_RECORD_BYTES {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "application journal record exceeds its size limit",
                ));
            }
            Ok(Some(bytes))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ServiceError::new(ErrorCode::Io, error.to_string())),
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, ServiceError> {
    read_optional(path)?.ok_or_else(|| {
        ServiceError::new(
            ErrorCode::Storage,
            "pending journal entry disappeared during recovery",
        )
    })
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[usize::from(byte >> 4)] as char);
        out.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    out
}

#[derive(Debug)]
struct JournalEncoder {
    bytes: Vec<u8>,
}

impl JournalEncoder {
    fn new(magic: &[u8; 8]) -> Self {
        let mut bytes = Vec::with_capacity(256);
        bytes.extend_from_slice(magic);
        bytes.push(JOURNAL_VERSION);
        Self { bytes }
    }

    fn finish(self) -> Result<Vec<u8>, ServiceError> {
        if self.bytes.len() > MAX_JOURNAL_RECORD_BYTES {
            return Err(ServiceError::new(
                ErrorCode::Storage,
                "application journal record exceeds its size limit",
            ));
        }
        Ok(self.bytes)
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn string(&mut self, value: &str, max: usize) -> Result<(), ServiceError> {
        if value.len() > max {
            return Err(ServiceError::new(
                ErrorCode::Storage,
                "journal string exceeds its field limit",
            ));
        }
        self.u32(u32::try_from(value.len()).map_err(|_| {
            ServiceError::new(ErrorCode::Storage, "journal string length overflow")
        })?);
        self.bytes.extend_from_slice(value.as_bytes());
        Ok(())
    }

    fn bytes(&mut self, value: &[u8], max: usize) -> Result<(), ServiceError> {
        if value.len() > max {
            return Err(ServiceError::new(
                ErrorCode::Storage,
                "signed object exceeds the journal limit",
            ));
        }
        self.u32(u32::try_from(value.len()).map_err(|_| {
            ServiceError::new(ErrorCode::Storage, "journal object length overflow")
        })?);
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    fn details(&mut self, draft: &PublicProfileDetails) -> Result<(), ServiceError> {
        draft.validate().map_err(storage_or_social)?;
        self.string(&draft.display_name, MAX_PROFILE_NAME_BYTES)?;
        self.string(&draft.bio, MAX_PROFILE_BIO_BYTES)?;
        self.u8(u8::from(draft.avatar.is_some()));
        if let Some(value) = &draft.avatar {
            self.string(value, MAX_OBJECT_ID_BYTES)?;
        }
        self.u8(u8::from(draft.location.is_some()));
        if let Some(value) = &draft.location {
            self.string(value, mini_app_protocol::MAX_LOCATION_BYTES)?;
        }
        self.u8(u8::from(draft.age.is_some()));
        if let Some(value) = draft.age {
            self.u8(value);
        }
        self.u8(draft.fields.len() as u8);
        for field in &draft.fields {
            self.string(
                &field.label,
                mini_app_protocol::MAX_PROFILE_FIELD_LABEL_BYTES,
            )?;
            self.string(
                &field.value,
                mini_app_protocol::MAX_PROFILE_FIELD_VALUE_BYTES,
            )?;
        }
        Ok(())
    }
    fn input(&mut self, input: &MutationInput) -> Result<(), ServiceError> {
        match input {
            MutationInput::MediaPost { media_id, caption } => {
                self.u8(10);
                self.string(media_id, MAX_OBJECT_ID_BYTES)?;
                self.string(caption, MAX_POST_BYTES)?;
            }
            MutationInput::Wall { draft } => {
                draft.validate().map_err(storage_or_social)?;
                self.u8(9);
                self.string(&draft.display_name, mini_app_protocol::MAX_WALL_NAME_BYTES)?;
                self.string(&draft.bio, mini_app_protocol::MAX_WALL_BIO_BYTES)?;
                self.u8(match draft.visibility {
                    mini_app_protocol::WallVisibility::Public => 0,
                    mini_app_protocol::WallVisibility::Unlisted => 1,
                });
                self.u8(draft.public_links.len() as u8);
                for link in &draft.public_links {
                    self.string(link, mini_app_protocol::MAX_WALL_LINK_BYTES)?;
                }
            }
            MutationInput::Profile { display_name, bio } => {
                self.u8(0);
                self.string(display_name, MAX_PROFILE_NAME_BYTES)?;
                self.string(bio, MAX_PROFILE_BIO_BYTES)?;
            }
            MutationInput::ProfileDetails { draft } => {
                self.u8(7);
                self.details(draft)?;
            }
            MutationInput::PhotoProfile {
                draft,
                photo_digest,
                photo_len,
            } => {
                self.u8(8);
                self.details(draft)?;
                self.bytes.extend_from_slice(photo_digest);
                self.u32(*photo_len);
            }
            MutationInput::Community { name, charter } => {
                self.u8(6);
                self.string(name, MAX_COMMUNITY_NAME_BYTES)?;
                self.string(charter, MAX_COMMUNITY_CHARTER_BYTES)?;
            }
            MutationInput::Membership {
                community_id,
                joined,
            } => {
                self.u8(5);
                self.string(community_id, MAX_OBJECT_ID_BYTES)?;
                self.u8(u8::from(*joined));
            }
            MutationInput::Comment { parent_id, text } => {
                self.u8(4);
                self.string(parent_id, MAX_OBJECT_ID_BYTES)?;
                self.string(text, MAX_COMMENT_BYTES)?;
            }
            MutationInput::Like { target_id, active } => {
                self.u8(3);
                self.string(target_id, MAX_OBJECT_ID_BYTES)?;
                self.u8(u8::from(*active));
            }
            MutationInput::Follow {
                target_did,
                following,
            } => {
                self.u8(2);
                self.string(target_did, MAX_DID_BYTES)?;
                self.u8(u8::from(*following));
            }
            MutationInput::Post { text } => {
                self.u8(1);
                self.string(text, MAX_POST_BYTES)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
struct JournalDecoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> JournalDecoder<'a> {
    fn new(bytes: &'a [u8], magic: &[u8; 8]) -> Result<Self, ServiceError> {
        if bytes.len() > MAX_JOURNAL_RECORD_BYTES
            || bytes.len() < magic.len() + 1
            || &bytes[..magic.len()] != magic
            || bytes[magic.len()] != JOURNAL_VERSION
        {
            return Err(ServiceError::new(
                ErrorCode::Storage,
                "invalid application journal record",
            ));
        }
        Ok(Self {
            bytes,
            offset: magic.len() + 1,
        })
    }

    fn finish(&self) -> Result<(), ServiceError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(ServiceError::new(
                ErrorCode::Storage,
                "application journal has trailing bytes",
            ))
        }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], ServiceError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| ServiceError::new(ErrorCode::Storage, "journal length overflow"))?;
        let out = self.bytes.get(self.offset..end).ok_or_else(|| {
            ServiceError::new(ErrorCode::Storage, "truncated application journal")
        })?;
        self.offset = end;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, ServiceError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, ServiceError> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("four-byte journal slice");
        Ok(u32::from_be_bytes(bytes))
    }

    fn string(&mut self, max: usize) -> Result<String, ServiceError> {
        let length = self.u32()? as usize;
        if length > max {
            return Err(ServiceError::new(
                ErrorCode::Storage,
                "journal string exceeds its field limit",
            ));
        }
        String::from_utf8(self.take(length)?.to_vec()).map_err(|error| {
            ServiceError::new(ErrorCode::Storage, format!("journal utf-8: {error}"))
        })
    }

    fn bytes(&mut self, max: usize) -> Result<Vec<u8>, ServiceError> {
        let length = self.u32()? as usize;
        if length > max {
            return Err(ServiceError::new(
                ErrorCode::Storage,
                "journal object exceeds its field limit",
            ));
        }
        Ok(self.take(length)?.to_vec())
    }

    fn details(&mut self) -> Result<PublicProfileDetails, ServiceError> {
        let display_name = self.string(MAX_PROFILE_NAME_BYTES)?;
        let bio = self.string(MAX_PROFILE_BIO_BYTES)?;
        let avatar = if self.flag()? {
            Some(self.string(MAX_OBJECT_ID_BYTES)?)
        } else {
            None
        };
        let location = if self.flag()? {
            Some(self.string(mini_app_protocol::MAX_LOCATION_BYTES)?)
        } else {
            None
        };
        let age = if self.flag()? { Some(self.u8()?) } else { None };
        let count = self.u8()? as usize;
        if count > mini_app_protocol::MAX_PROFILE_FIELDS {
            return Err(ServiceError::new(
                ErrorCode::Storage,
                "too many journal profile fields",
            ));
        }
        let mut fields = Vec::with_capacity(count);
        for _ in 0..count {
            fields.push(mini_app_protocol::ProfileField {
                label: self.string(mini_app_protocol::MAX_PROFILE_FIELD_LABEL_BYTES)?,
                value: self.string(mini_app_protocol::MAX_PROFILE_FIELD_VALUE_BYTES)?,
            });
        }
        let draft = PublicProfileDetails {
            display_name,
            bio,
            avatar,
            location,
            age,
            fields,
        };
        draft.validate().map_err(storage_or_social)?;
        Ok(draft)
    }
    fn flag(&mut self) -> Result<bool, ServiceError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ServiceError::new(
                ErrorCode::Storage,
                "invalid journal option flag",
            )),
        }
    }
    fn input(&mut self) -> Result<MutationInput, ServiceError> {
        match self.u8()? {
            10 => {
                let media_id = self.string(MAX_OBJECT_ID_BYTES)?;
                ObjectId::parse(&media_id).map_err(storage_error)?;
                let caption = self.string(MAX_POST_BYTES)?;
                Ok(MutationInput::MediaPost { media_id, caption })
            }
            9 => {
                let display_name = self.string(mini_app_protocol::MAX_WALL_NAME_BYTES)?;
                let bio = self.string(mini_app_protocol::MAX_WALL_BIO_BYTES)?;
                let visibility = match self.u8()? {
                    0 => mini_app_protocol::WallVisibility::Public,
                    1 => mini_app_protocol::WallVisibility::Unlisted,
                    _ => {
                        return Err(ServiceError::new(
                            ErrorCode::Storage,
                            "invalid journal wall visibility",
                        ))
                    }
                };
                let count = self.u8()? as usize;
                if count > mini_app_protocol::MAX_WALL_LINKS {
                    return Err(ServiceError::new(ErrorCode::Storage, "too many wall links"));
                }
                let mut public_links = Vec::with_capacity(count);
                for _ in 0..count {
                    public_links.push(self.string(mini_app_protocol::MAX_WALL_LINK_BYTES)?);
                }
                let draft = PublicWallDraft {
                    display_name,
                    bio,
                    public_links,
                    visibility,
                };
                draft.validate().map_err(storage_or_social)?;
                Ok(MutationInput::Wall { draft })
            }
            0 => Ok(MutationInput::Profile {
                display_name: self.string(MAX_PROFILE_NAME_BYTES)?,
                bio: self.string(MAX_PROFILE_BIO_BYTES)?,
            }),
            7 => Ok(MutationInput::ProfileDetails {
                draft: self.details()?,
            }),
            8 => {
                let draft = self.details()?;
                let photo_digest = self.take(32)?.try_into().map_err(storage_or_social)?;
                let photo_len = self.u32()?;
                if draft.avatar.is_some()
                    || photo_len == 0
                    || photo_len as usize > mini_app_protocol::MAX_PROFILE_PHOTO_BYTES
                {
                    return Err(ServiceError::new(
                        ErrorCode::Storage,
                        "invalid photo draft journal",
                    ));
                }
                Ok(MutationInput::PhotoProfile {
                    draft,
                    photo_digest,
                    photo_len,
                })
            }
            6 => {
                let name = self.string(MAX_COMMUNITY_NAME_BYTES)?;
                let charter = self.string(MAX_COMMUNITY_CHARTER_BYTES)?;
                if name.trim().is_empty() {
                    return Err(ServiceError::new(
                        ErrorCode::Storage,
                        "blank journal community name",
                    ));
                }
                Ok(MutationInput::Community { name, charter })
            }
            5 => {
                let community_id = self.string(MAX_OBJECT_ID_BYTES)?;
                ObjectId::parse(&community_id).map_err(storage_error)?;
                let joined = match self.u8()? {
                    0 => false,
                    1 => true,
                    _ => {
                        return Err(ServiceError::new(
                            ErrorCode::Storage,
                            "invalid journal membership action",
                        ))
                    }
                };
                Ok(MutationInput::Membership {
                    community_id,
                    joined,
                })
            }
            4 => {
                let parent_id = self.string(MAX_OBJECT_ID_BYTES)?;
                ObjectId::parse(&parent_id).map_err(storage_error)?;
                let text = self.string(MAX_COMMENT_BYTES)?;
                if text.trim().is_empty() {
                    return Err(ServiceError::new(
                        ErrorCode::Storage,
                        "empty journal comment",
                    ));
                }
                Ok(MutationInput::Comment { parent_id, text })
            }
            3 => {
                let target_id = self.string(MAX_OBJECT_ID_BYTES)?;
                ObjectId::parse(&target_id).map_err(storage_error)?;
                let active = match self.u8()? {
                    0 => false,
                    1 => true,
                    _ => {
                        return Err(ServiceError::new(
                            ErrorCode::Storage,
                            "invalid journal like action",
                        ))
                    }
                };
                Ok(MutationInput::Like { target_id, active })
            }
            2 => {
                let target_did = self.string(MAX_DID_BYTES)?;
                Did::parse(&target_did).map_err(storage_error)?;
                let following = match self.u8()? {
                    0 => false,
                    1 => true,
                    _ => {
                        return Err(ServiceError::new(
                            ErrorCode::Storage,
                            "invalid journal follow action",
                        ))
                    }
                };
                Ok(MutationInput::Follow {
                    target_did,
                    following,
                })
            }
            1 => Ok(MutationInput::Post {
                text: self.string(MAX_POST_BYTES)?,
            }),
            _ => Err(ServiceError::new(
                ErrorCode::Storage,
                "invalid journal mutation kind",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_app_protocol::{FeedScope, Reply};

    mod beta_wallet_tests;

    #[derive(Debug, Clone)]
    struct MemoryVault {
        root: Option<SeedPair>,
        device: Option<SeedPair>,
    }

    impl MemoryVault {
        fn empty() -> Self {
            Self {
                root: None,
                device: None,
            }
        }

        fn seeded() -> Self {
            Self {
                root: Some(SeedPair {
                    current: [11; 32],
                    next: [12; 32],
                }),
                device: Some(SeedPair {
                    current: [13; 32],
                    next: [14; 32],
                }),
            }
        }
    }

    impl IdentityVault for MemoryVault {
        fn root_exists(&self) -> bool {
            self.root.is_some()
        }

        fn load_root(&mut self) -> Result<SeedPair, ServiceError> {
            self.root.clone().ok_or_else(|| {
                ServiceError::new(ErrorCode::RootMissing, "test root does not exist")
            })
        }

        fn load_or_create_root(&mut self) -> Result<SeedPair, ServiceError> {
            let pair = self.root.get_or_insert(SeedPair {
                current: [11; 32],
                next: [12; 32],
            });
            Ok(pair.clone())
        }

        fn load_or_create_device(&mut self) -> Result<SeedPair, ServiceError> {
            let pair = self.device.get_or_insert(SeedPair {
                current: [13; 32],
                next: [14; 32],
            });
            Ok(pair.clone())
        }
    }

    fn temp_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mini-app-service-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn forwarded_stop_prevents_another_scheduling_turn_until_control_is_processed() {
        let root = temp_root("stop-pending");
        let mut core = Core::open_inner(root.clone(), MemoryVault::seeded(), false).unwrap();
        core.handle(Command::UnlockIdentity).unwrap();
        core.handle(Command::SetDeliveryEnabled { enabled: true })
            .unwrap();
        let interrupt = core.delivery_interrupt_handle();
        let stop = interrupt.stop();
        core.poll_delivery().unwrap();
        assert!(core.delivery_listener.is_none());
        assert!(core.attempt_delivery("any", now_ms()).is_err());
        core.handle(Command::LockIdentity).unwrap();
        drop(stop);
        assert!(!interrupt.is_stopped());
        core.poll_delivery().unwrap();
        assert!(core.identity.is_none() && core.delivery_listener.is_none());
        drop(core);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn offline_delivery_recovers_and_reports_verified_remote_receipt_without_new_signature() {
        let sender_root = temp_root("sender-runtime");
        let receiver_root = temp_root("receiver-runtime");
        let sender_vault = MemoryVault::seeded();
        let receiver_vault = MemoryVault {
            root: Some(SeedPair {
                current: [21; 32],
                next: [22; 32],
            }),
            device: Some(SeedPair {
                current: [23; 32],
                next: [24; 32],
            }),
        };
        let mut sender =
            Core::open_inner(sender_root.clone(), sender_vault.clone(), false).unwrap();
        let mut recipient =
            Core::open_inner(receiver_root.clone(), receiver_vault.clone(), false).unwrap();
        sender.handle(Command::UnlockIdentity).unwrap();
        recipient.handle(Command::UnlockIdentity).unwrap();
        let Reply::DeliveryIdentity(sender_public) =
            sender.handle(Command::ExportDeliveryIdentity).unwrap()
        else {
            panic!("identity reply");
        };
        let Reply::DeliveryIdentity(recipient_public) =
            recipient.handle(Command::ExportDeliveryIdentity).unwrap()
        else {
            panic!("identity reply");
        };
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = probe.local_addr().unwrap();
        drop(probe);
        sender
            .handle(Command::AuthorizeDeliveryPeer {
                peer_id: "recipient".into(),
                endpoint: address.to_string(),
                root_kel: recipient_public.root_kel,
                device_kel: recipient_public.device_kel,
            })
            .unwrap();
        recipient
            .handle(Command::AuthorizeDeliveryPeer {
                peer_id: "sender".into(),
                endpoint: address.to_string(),
                root_kel: sender_public.root_kel,
                device_kel: sender_public.device_kel,
            })
            .unwrap();
        let Reply::Published(post) = sender
            .handle(Command::PublishPost {
                operation_id: "post:once".into(),
                text: "survives an offline recipient".into(),
            })
            .unwrap()
        else {
            panic!("post reply");
        };
        let sequence_after_publication = sender.next_sequence;
        sender
            .handle(Command::QueuePublicDelivery {
                operation_id: "send:once".into(),
                peer_id: "recipient".into(),
                object_id: post.object_id.clone(),
            })
            .unwrap();
        assert!(sender
            .handle(Command::AttemptDelivery {
                operation_id: "send:once".into()
            })
            .is_err());
        assert_eq!(sender.outbox.get("send:once").unwrap().unwrap().attempts, 0);
        sender
            .handle(Command::SetDeliveryEnabled { enabled: true })
            .unwrap();
        let failed = sender.attempt_delivery("send:once", now_ms()).unwrap();
        assert_eq!(failed.state, mini_app_protocol::DeliveryState::RetryWaiting);
        let exact_bytes = sender
            .outbox
            .get("send:once")
            .unwrap()
            .unwrap()
            .object_bytes;
        drop(sender);
        let mut sender = Core::open_inner(sender_root.clone(), sender_vault, false).unwrap();
        assert!(sender.delivery_policy.enabled);
        sender.poll_delivery().unwrap();
        assert_eq!(sender.outbox.get("send:once").unwrap().unwrap().attempts, 1);
        assert!(sender.delivery_listener.is_none());
        sender.handle(Command::UnlockIdentity).unwrap();
        recipient
            .handle(Command::SetDeliveryEnabled { enabled: true })
            .unwrap();
        recipient
            .handle(Command::SetDeliveryListener {
                peer_id: "sender".into(),
                endpoint: address.to_string(),
            })
            .unwrap();
        let listener = std::net::TcpListener::bind(address).unwrap();
        listener.set_nonblocking(true).unwrap();
        recipient.delivery_listener = Some(listener);
        let expected_id = ObjectId::parse(&post.object_id).unwrap();
        let remote_id = expected_id.clone();
        let worker = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while std::time::Instant::now() < deadline {
                recipient.poll_delivery().unwrap();
                if recipient.store.get(&remote_id).is_ok() {
                    return recipient;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            panic!("recipient did not ingest queued post");
        });
        let delivered = sender
            .attempt_delivery("send:once", failed.next_attempt_ms)
            .unwrap();
        assert_eq!(delivered.state, mini_app_protocol::DeliveryState::Delivered);
        assert_eq!(delivered.attempts, 2);
        assert_eq!(sender.next_sequence, sequence_after_publication);
        let recipient = worker.join().unwrap();
        assert_eq!(
            recipient.store.get(&expected_id).unwrap().to_bytes(),
            exact_bytes
        );
        assert_eq!(
            recipient.store.by_type(&ObjectType::POST).unwrap(),
            vec![expected_id.clone()]
        );
        assert_eq!(
            sender.attempt_delivery("send:once", now_ms()).unwrap(),
            delivered
        );
        drop(sender);
        drop(recipient);
        let mut sender =
            Core::open_inner(sender_root.clone(), MemoryVault::seeded(), false).unwrap();
        assert_eq!(
            sender
                .handle(Command::DeliveryStatus {
                    operation_id: "send:once".into()
                })
                .unwrap(),
            Reply::Delivery(delivered)
        );
        let recipient = Core::open_inner(receiver_root.clone(), receiver_vault, false).unwrap();
        assert_eq!(
            recipient.store.get(&expected_id).unwrap().to_bytes(),
            exact_bytes
        );
        drop(sender);
        drop(recipient);
        fs::remove_dir_all(sender_root).unwrap();
        fs::remove_dir_all(receiver_root).unwrap();
    }

    #[test]
    fn revoked_recipient_never_connects_and_locked_identity_stops_listener() {
        let root = temp_root("delivery-permissions");
        let mut core = Core::open_inner(root.clone(), MemoryVault::seeded(), false).unwrap();
        core.handle(Command::UnlockIdentity).unwrap();
        let Reply::DeliveryIdentity(public) = core.handle(Command::ExportDeliveryIdentity).unwrap()
        else {
            panic!("identity reply");
        };
        core.handle(Command::AuthorizeDeliveryPeer {
            peer_id: "peer".into(),
            endpoint: "127.0.0.1:9".into(),
            root_kel: public.root_kel,
            device_kel: public.device_kel,
        })
        .unwrap();
        let Reply::Published(post) = core
            .handle(Command::PublishPost {
                operation_id: "post:1".into(),
                text: "public".into(),
            })
            .unwrap()
        else {
            panic!("post reply");
        };
        core.handle(Command::QueuePublicDelivery {
            operation_id: "send:1".into(),
            peer_id: "peer".into(),
            object_id: post.object_id,
        })
        .unwrap();
        core.handle(Command::SetDeliveryEnabled { enabled: true })
            .unwrap();
        core.handle(Command::RevokeDeliveryPeer {
            peer_id: "peer".into(),
        })
        .unwrap();
        let attempt = core.attempt_delivery("send:1", now_ms()).unwrap();
        assert_eq!(
            attempt.last_failure,
            Some(mini_app_protocol::DeliveryFailure::Rejected)
        );
        assert_eq!(
            attempt.state,
            mini_app_protocol::DeliveryState::RetryWaiting
        );
        core.delivery_listener = Some(std::net::TcpListener::bind("127.0.0.1:0").unwrap());
        core.handle(Command::LockIdentity).unwrap();
        assert!(core.delivery_listener.is_none());
        core.poll_delivery().unwrap();
        assert_eq!(core.outbox.get("send:1").unwrap().unwrap().attempts, 1);
        core.handle(Command::CancelDelivery {
            operation_id: "send:1".into(),
        })
        .unwrap();
        drop(core);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn queued_public_post_survives_locked_restart_and_cancellation_without_resigning() {
        let root = temp_root("durable-outbox");
        let vault = MemoryVault::seeded();
        let mut core = Core::open_inner(root.clone(), vault.clone(), false).unwrap();
        core.handle(Command::UnlockIdentity).unwrap();
        let Reply::Published(post) = core
            .handle(Command::PublishPost {
                operation_id: "publish:1".into(),
                text: "public post".into(),
            })
            .unwrap()
        else {
            panic!("publication reply");
        };
        let identity = core.identity.as_ref().unwrap();
        let root_kel = identity.root.kel().to_bytes();
        let device_kel = identity.device.kel().to_bytes();
        core.handle(Command::AuthorizeDeliveryPeer {
            peer_id: "recipient".into(),
            endpoint: "127.0.0.1:9".into(),
            root_kel,
            device_kel,
        })
        .unwrap();
        let next_sequence = core.next_sequence;
        let queue = Command::QueuePublicDelivery {
            operation_id: "send:1".into(),
            peer_id: "recipient".into(),
            object_id: post.object_id.clone(),
        };
        let queued = core.handle(queue.clone()).unwrap();
        assert_eq!(queued, core.handle(queue).unwrap());
        let bytes = core.outbox.get("send:1").unwrap().unwrap().object_bytes;
        assert_eq!(core.next_sequence, next_sequence);
        drop(core);
        let mut restarted = Core::open_inner(root.clone(), vault, false).unwrap();
        assert!(restarted.identity.is_none());
        assert_eq!(
            restarted
                .handle(Command::DeliveryStatus {
                    operation_id: "send:1".into()
                })
                .unwrap(),
            queued
        );
        assert_eq!(
            restarted
                .outbox
                .get("send:1")
                .unwrap()
                .unwrap()
                .object_bytes,
            bytes
        );
        let Reply::Delivery(cancelled) = restarted
            .handle(Command::CancelDelivery {
                operation_id: "send:1".into(),
            })
            .unwrap()
        else {
            panic!("delivery reply");
        };
        assert_eq!(cancelled.state, mini_app_protocol::DeliveryState::Cancelled);
        assert_eq!(restarted.next_sequence, next_sequence);
        assert!(restarted
            .handle(Command::ListDeliveries { limit: 0 })
            .is_err());
        drop(restarted);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn identity_profile_post_feed_and_restart_idempotency_share_one_authority() {
        let root = temp_root("e2e");
        let vault = MemoryVault::empty();
        let mut core = Core::open_inner(root.clone(), vault, false).unwrap();

        let status = core.handle(Command::CreateRoot).unwrap();
        let Reply::Status(status) = status else {
            panic!("create root returned wrong reply");
        };
        assert!(status.root_created);
        assert!(status.identity_unlocked);

        let first_profile = core
            .handle(Command::PublishProfile {
                operation_id: "profile:create:1".to_string(),
                display_name: "Alice".to_string(),
                bio: "local-first".to_string(),
            })
            .unwrap();
        let Reply::Published(first_profile) = first_profile else {
            panic!("publish profile returned wrong reply");
        };
        assert_eq!(first_profile.kind, ObjectKind::Profile);
        assert!(!first_profile.duplicate);

        let first_post = core
            .handle(Command::PublishPost {
                operation_id: "post:create:1".to_string(),
                text: "hello from the application core".to_string(),
            })
            .unwrap();
        let Reply::Published(first_post) = first_post else {
            panic!("publish post returned wrong reply");
        };
        assert!(!first_post.duplicate);

        let feed = core
            .handle(Command::FeedSnapshot {
                order: FeedOrder::Chronological,
                scope: FeedScope::Following,
                limit: 50,
            })
            .unwrap();
        let Reply::Feed(feed) = feed else {
            panic!("feed returned wrong reply");
        };
        assert_eq!(feed.len(), 1);
        assert_eq!(feed[0].author, "Alice");
        assert_eq!(feed[0].body, "hello from the application core");
        assert!(feed[0].own);

        let saved_vault = core.vault.clone();
        drop(core);

        let mut restarted = Core::open_inner(root.clone(), saved_vault, false).unwrap();
        let duplicate = restarted
            .handle(Command::PublishPost {
                operation_id: "post:create:1".to_string(),
                text: "hello from the application core".to_string(),
            })
            .unwrap();
        let Reply::Published(duplicate) = duplicate else {
            panic!("duplicate post returned wrong reply");
        };
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.object_id, first_post.object_id);

        assert!(restarted
            .handle(Command::PublishPost {
                operation_id: "post:create:1".to_string(),
                text: "different content".to_string(),
            })
            .is_err());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pending_exact_signed_post_is_recovered_without_resigning() {
        let root = temp_root("recovery");
        let vault = MemoryVault::seeded();
        let mut core = Core::open_inner(root.clone(), vault.clone(), false).unwrap();
        core.unlock().unwrap();
        let (human, device) = core.signing_identity().unwrap();
        let object = build_post(human, device, "recover me", 77, 9).unwrap();
        let expected_id = object.id().as_str().to_string();
        let record = PendingMutation {
            operation_id: "post:recover:1".to_string(),
            input: MutationInput::Post {
                text: "recover me".to_string(),
            },
            objects: vec![object.to_bytes()],
        };
        core.journal.write_pending(&record).unwrap();
        drop(core);

        let mut recovered = Core::open_inner(root.clone(), vault, false).unwrap();
        let feed = recovered
            .handle(Command::FeedSnapshot {
                order: FeedOrder::Chronological,
                scope: FeedScope::Following,
                limit: 10,
            })
            .unwrap();
        let Reply::Feed(feed) = feed else {
            panic!("feed returned wrong reply");
        };
        assert_eq!(feed.len(), 1);
        assert_eq!(feed[0].id, expected_id);
        assert_eq!(feed[0].body, "recover me");

        let duplicate = recovered
            .handle(Command::PublishPost {
                operation_id: "post:recover:1".to_string(),
                text: "recover me".to_string(),
            })
            .unwrap();
        let Reply::Published(duplicate) = duplicate else {
            panic!("duplicate returned wrong reply");
        };
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.object_id, expected_id);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn profile_pending_record_is_bound_to_requested_content() {
        let root = temp_root("profile-binding");
        let vault = MemoryVault::seeded();
        let mut core = Core::open_inner(root.clone(), vault, false).unwrap();
        core.unlock().unwrap();
        let (human, device) = core.signing_identity().unwrap();
        let (profile, head) =
            build_profile(human, device, "Alice", "real bio", None, 77, 9).unwrap();
        let record = PendingMutation {
            operation_id: "profile:binding:1".to_string(),
            input: MutationInput::Profile {
                display_name: "Mallory".to_string(),
                bio: "different bio".to_string(),
            },
            objects: vec![profile.to_bytes(), head.to_bytes()],
        };
        core.journal.write_pending(&record).unwrap();
        drop(core);

        let error = Core::open_inner(root.clone(), MemoryVault::seeded(), false).unwrap_err();
        assert_eq!(error.code, ErrorCode::Storage);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn orphaned_atomic_replace_temp_does_not_block_recovery() {
        let root = temp_root("orphan-temp");
        let pending = root.join("app-service").join("pending");
        fs::create_dir_all(&pending).unwrap();
        let orphan = pending.join("deadbeef.op.999-1.tmp");
        fs::write(&orphan, b"partial uncommitted bytes").unwrap();

        let core = Core::open_inner(root.clone(), MemoryVault::empty(), false).unwrap();
        assert!(!orphan.exists());
        drop(core);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn production_lock_refuses_a_second_core_service_instance() {
        let root = temp_root("lock");
        let first = Core::open_inner(root.clone(), MemoryVault::empty(), true).unwrap();
        let error = Core::open_inner(root.clone(), MemoryVault::empty(), true).unwrap_err();
        assert_eq!(error.code, ErrorCode::Busy);
        drop(first);
        let third = Core::open_inner(root.clone(), MemoryVault::empty(), true).unwrap();
        drop(third);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn follow_mutations_drive_feed_and_keep_exact_retry_identity_after_locked_restart() {
        let path = temp_root("follow");
        let vault = MemoryVault::seeded();
        let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
        core.unlock().unwrap();
        let owner = core.human.clone().unwrap();
        let mut target_root = Controller::incept_single_from_seeds(&[41; 32], &[42; 32]).unwrap();
        let target_device =
            Controller::incept_device_single_from_seeds(&target_root.did(), &[43; 32], &[44; 32])
                .unwrap();
        target_root
            .delegate_device(&target_device.did(), Capabilities::primary())
            .unwrap();
        let target = target_root.did();
        let post = build_post(&target, &target_device, "friend post", 9, 1).unwrap();
        core.store.insert(&post).unwrap();
        let feed_command = Command::FeedSnapshot {
            order: FeedOrder::Chronological,
            scope: FeedScope::Following,
            limit: 10,
        };
        assert!(
            matches!(core.handle(feed_command.clone()).unwrap(),Reply::Feed(cards) if cards.is_empty())
        );
        let command = Command::SetFollow {
            operation_id: "follow:1".into(),
            target_did: target.as_str().into(),
            following: true,
        };
        let Reply::Published(published) = core.handle(command.clone()).unwrap() else {
            panic!("wrong reply")
        };
        assert_eq!(published.kind, ObjectKind::Follow);
        assert!(!published.duplicate);
        assert_eq!(
            following(&core.store, &owner).unwrap(),
            vec![target.clone()]
        );
        assert!(
            matches!(core.handle(feed_command.clone()).unwrap(),Reply::Feed(cards) if cards.len()==1 && cards[0].id==post.id().as_str())
        );
        let object_id = ObjectId::parse(&published.object_id).unwrap();
        let original = core.store.get(&object_id).unwrap().to_bytes();
        drop(core);
        let mut core = Core::open_inner(path.clone(), vault, false).unwrap();
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        let Reply::Published(duplicate) = core.handle(command).unwrap() else {
            panic!("wrong reply")
        };
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.object_id, published.object_id);
        assert_eq!(core.store.get(&object_id).unwrap().to_bytes(), original);
        assert!(core
            .handle(Command::SetFollow {
                operation_id: "new:locked".into(),
                target_did: target.as_str().into(),
                following: false
            })
            .is_err());
        assert!(core
            .handle(Command::SetFollow {
                operation_id: "follow:1".into(),
                target_did: target.as_str().into(),
                following: false
            })
            .is_err());
        assert!(core
            .handle(Command::SetFollow {
                operation_id: "bad:did".into(),
                target_did: "not-a-did".into(),
                following: true
            })
            .is_err());
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        assert_eq!(core.next_sequence, next);
        core.unlock().unwrap();
        core.handle(Command::SetFollow {
            operation_id: "unfollow:1".into(),
            target_did: target.as_str().into(),
            following: false,
        })
        .unwrap();
        assert!(following(&core.store, &owner).unwrap().is_empty());
        assert!(
            matches!(core.handle(feed_command).unwrap(),Reply::Feed(cards) if cards.is_empty())
        );
        let Reply::Published(stale_retry) = core
            .handle(Command::SetFollow {
                operation_id: "follow:1".into(),
                target_did: target.as_str().into(),
                following: true,
            })
            .unwrap()
        else {
            panic!("wrong reply")
        };
        assert!(stale_retry.duplicate);
        assert!(following(&core.store, &owner).unwrap().is_empty());
        drop(core);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn exact_pending_follow_recovers_but_tampered_input_never_inserts() {
        for tamper in [false, true] {
            let path = temp_root("follow-recover");
            let vault = MemoryVault::seeded();
            let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
            core.unlock().unwrap();
            let target = Controller::incept_single_from_seeds(&[51; 32], &[52; 32])
                .unwrap()
                .did();
            let (human, device) = core.signing_identity().unwrap();
            let object = build_follow(human, device, &target, true, 77, 9).unwrap();
            let object_id = object.id().clone();
            let record = PendingMutation {
                operation_id: "follow:recover".into(),
                input: MutationInput::Follow {
                    target_did: target.as_str().into(),
                    following: !tamper,
                },
                objects: vec![object.to_bytes()],
            };
            core.journal.write_pending(&record).unwrap();
            drop(core);
            let recovered = Core::open_inner(path.clone(), vault, false);
            if tamper {
                assert!(recovered.is_err());
                let store = Store::new(FsBackend::open(&path).unwrap());
                assert!(store.get(&object_id).is_err());
            } else {
                let mut core = recovered.unwrap();
                assert_eq!(
                    core.store.get(&object_id).unwrap().to_bytes(),
                    object.to_bytes()
                );
                let Reply::Published(reply) = core
                    .handle(Command::SetFollow {
                        operation_id: "follow:recover".into(),
                        target_did: target.as_str().into(),
                        following: true,
                    })
                    .unwrap()
                else {
                    panic!("wrong reply")
                };
                assert!(reply.duplicate);
                assert_eq!(reply.object_id, object_id.as_str());
                drop(core);
            }
            fs::remove_dir_all(path).unwrap();
        }
    }
    fn like_count(core: &Core<MemoryVault>, target: &ObjectId) -> usize {
        reaction_counts(&core.store, target)
            .unwrap()
            .into_iter()
            .find(|(kind, _)| *kind == mini_social::ReactionKind::Like)
            .map(|(_, count)| count)
            .unwrap_or(0)
    }
    #[test]
    fn like_unlike_counts_and_locked_retry_keep_exact_bytes_without_reviving_stale_like() {
        let path = temp_root("likes");
        let vault = MemoryVault::seeded();
        let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
        core.unlock().unwrap();
        let Reply::Published(post) = core
            .handle(Command::PublishPost {
                operation_id: "post:target".into(),
                text: "target".into(),
            })
            .unwrap()
        else {
            panic!("wrong reply")
        };
        let target = ObjectId::parse(&post.object_id).unwrap();
        let command = Command::SetLike {
            operation_id: "like:1".into(),
            target_id: target.as_str().into(),
            active: true,
        };
        let Reply::Published(liked) = core.handle(command.clone()).unwrap() else {
            panic!("wrong reply")
        };
        assert_eq!(liked.kind, ObjectKind::Reaction);
        assert_eq!(like_count(&core, &target), 1);
        let id = ObjectId::parse(&liked.object_id).unwrap();
        let bytes = core.store.get(&id).unwrap().to_bytes();
        drop(core);
        let mut core = Core::open_inner(path.clone(), vault, false).unwrap();
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        let Reply::Published(retry) = core.handle(command.clone()).unwrap() else {
            panic!("wrong reply")
        };
        assert!(retry.duplicate);
        assert_eq!(retry.object_id, liked.object_id);
        assert_eq!(core.store.get(&id).unwrap().to_bytes(), bytes);
        assert!(core
            .handle(Command::SetLike {
                operation_id: "new:locked".into(),
                target_id: target.as_str().into(),
                active: false
            })
            .is_err());
        assert!(core
            .handle(Command::SetLike {
                operation_id: "like:1".into(),
                target_id: target.as_str().into(),
                active: false
            })
            .is_err());
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        core.unlock().unwrap();
        core.handle(Command::SetLike {
            operation_id: "unlike:1".into(),
            target_id: target.as_str().into(),
            active: false,
        })
        .unwrap();
        assert_eq!(like_count(&core, &target), 0);
        let Reply::Published(stale) = core.handle(command).unwrap() else {
            panic!("wrong reply")
        };
        assert!(stale.duplicate);
        assert_eq!(like_count(&core, &target), 0);
        drop(core);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn missing_private_and_noncontent_like_targets_never_reserve_sequence() {
        let path = temp_root("like-targets");
        let mut core = Core::open_inner(path.clone(), MemoryVault::seeded(), false).unwrap();
        core.unlock().unwrap();
        let (human, device) = core.signing_identity().unwrap();
        let private = mini_objects::ObjectBuilder::new(ObjectType::POST)
            .payload(Payload::Encrypted(vec![1]))
            .sign(human, device)
            .unwrap();
        let other = mini_objects::ObjectBuilder::new(ObjectType::PROFILE)
            .payload(Payload::Public(vec![]))
            .sign(human, device)
            .unwrap();
        let missing = build_post(human, device, "missing", 1, 1).unwrap();
        core.store.insert(&private).unwrap();
        core.store.insert(&other).unwrap();
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        for (index, target) in [private.id(), other.id(), missing.id()].iter().enumerate() {
            assert!(core
                .handle(Command::SetLike {
                    operation_id: format!("bad-target:{index}"),
                    target_id: target.as_str().into(),
                    active: true
                })
                .is_err());
        }
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        assert!(core
            .store
            .by_type(&ObjectType::REACTION)
            .unwrap()
            .is_empty());
        drop(core);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn pending_like_recovers_exact_bytes_and_refuses_tampered_action_or_kind() {
        for mode in 0..3 {
            let path = temp_root("like-recover");
            let vault = MemoryVault::seeded();
            let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
            core.unlock().unwrap();
            let (human, device) = core.signing_identity().unwrap();
            let target = build_post(human, device, "target", 1, 1).unwrap();
            let reaction = build_reaction(
                human,
                device,
                target.id(),
                if mode == 2 {
                    mini_social::ReactionKind::Love
                } else {
                    mini_social::ReactionKind::Like
                },
                true,
                77,
                9,
            )
            .unwrap();
            let id = reaction.id().clone();
            core.store.insert(&target).unwrap();
            let record = PendingMutation {
                operation_id: "like:recover".into(),
                input: MutationInput::Like {
                    target_id: target.id().as_str().into(),
                    active: mode != 1,
                },
                objects: vec![reaction.to_bytes()],
            };
            core.journal.write_pending(&record).unwrap();
            drop(core);
            let recovered = Core::open_inner(path.clone(), vault, false);
            if mode != 0 {
                assert!(recovered.is_err());
                let store = Store::new(FsBackend::open(&path).unwrap());
                assert!(store.get(&id).is_err());
            } else {
                let mut core = recovered.unwrap();
                assert_eq!(core.store.get(&id).unwrap().to_bytes(), reaction.to_bytes());
                let Reply::Published(reply) = core
                    .handle(Command::SetLike {
                        operation_id: "like:recover".into(),
                        target_id: target.id().as_str().into(),
                        active: true,
                    })
                    .unwrap()
                else {
                    panic!("wrong reply")
                };
                assert!(reply.duplicate);
                assert_eq!(reply.object_id, id.as_str());
                drop(core);
            }
            fs::remove_dir_all(path).unwrap();
        }
    }
    #[test]
    fn public_comments_thread_and_custom_parents_keep_locked_receipts_after_parent_removal() {
        let path = temp_root("comments");
        let vault = MemoryVault::seeded();
        let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
        core.unlock().unwrap();
        let Reply::Published(post) = core
            .handle(Command::PublishPost {
                operation_id: "comment:parent".into(),
                text: "parent".into(),
            })
            .unwrap()
        else {
            panic!("wrong reply")
        };
        let command = Command::PublishComment {
            operation_id: "comment:1".into(),
            parent_id: post.object_id.clone(),
            text: "first reply".into(),
        };
        let Reply::Published(comment) = core.handle(command.clone()).unwrap() else {
            panic!("wrong reply")
        };
        assert_eq!(comment.kind, ObjectKind::Comment);
        core.handle(Command::PublishComment {
            operation_id: "comment:thread".into(),
            parent_id: comment.object_id.clone(),
            text: "nested reply".into(),
        })
        .unwrap();
        assert_eq!(
            comments(&core.store, &ObjectId::parse(&post.object_id).unwrap()).unwrap()[0].text,
            "first reply"
        );
        assert_eq!(
            comments(&core.store, &ObjectId::parse(&comment.object_id).unwrap()).unwrap()[0].text,
            "nested reply"
        );
        for (index, kind) in [
            ObjectType::COMMUNITY,
            ObjectType::Custom("example/public-topic".into()),
        ]
        .into_iter()
        .enumerate()
        {
            let (human, device) = core.signing_identity().unwrap();
            let parent = mini_objects::ObjectBuilder::new(kind)
                .payload(Payload::Public(vec![1]))
                .sign(human, device)
                .unwrap();
            core.store.insert(&parent).unwrap();
            core.handle(Command::PublishComment {
                operation_id: format!("comment:topic:{index}"),
                parent_id: parent.id().as_str().into(),
                text: "topic reply".into(),
            })
            .unwrap();
            assert_eq!(comments(&core.store, parent.id()).unwrap().len(), 1);
        }
        let id = ObjectId::parse(&comment.object_id).unwrap();
        let bytes = core.store.get(&id).unwrap().to_bytes();
        drop(core);
        let mut core = Core::open_inner(path.clone(), vault, false).unwrap();
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        let parent_path = path
            .join("blobs")
            .join(&post.object_id[..2])
            .join(&post.object_id);
        fs::remove_file(parent_path).unwrap();
        let Reply::Published(retry) = core.handle(command).unwrap() else {
            panic!("wrong reply")
        };
        assert!(retry.duplicate);
        assert_eq!(retry.object_id, comment.object_id);
        assert_eq!(core.store.get(&id).unwrap().to_bytes(), bytes);
        assert!(core
            .handle(Command::PublishComment {
                operation_id: "comment:new-locked".into(),
                parent_id: comment.object_id.clone(),
                text: "locked".into()
            })
            .is_err());
        assert!(core
            .handle(Command::PublishComment {
                operation_id: "comment:1".into(),
                parent_id: post.object_id,
                text: "changed".into()
            })
            .is_err());
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        drop(core);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn invalid_private_missing_or_blank_new_comments_never_reserve_sequence() {
        let path = temp_root("comment-invalid");
        let mut core = Core::open_inner(path.clone(), MemoryVault::seeded(), false).unwrap();
        core.unlock().unwrap();
        let (human, device) = core.signing_identity().unwrap();
        let private = mini_objects::ObjectBuilder::new(ObjectType::POST)
            .payload(Payload::Encrypted(vec![1]))
            .sign(human, device)
            .unwrap();
        let missing = build_post(human, device, "missing", 1, 1).unwrap();
        core.store.insert(&private).unwrap();
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        for (index, (parent_id, text)) in [
            (private.id().as_str().to_string(), "reply".to_string()),
            (missing.id().as_str().into(), "reply".into()),
            ("invalid-id".into(), "reply".into()),
            (private.id().as_str().into(), "  \n\t".into()),
            (
                private.id().as_str().into(),
                "x".repeat(MAX_COMMENT_BYTES + 1),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            assert!(core
                .handle(Command::PublishComment {
                    operation_id: format!("comment:bad:{index}"),
                    parent_id,
                    text
                })
                .is_err());
        }
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        assert!(core.store.by_type(&ObjectType::COMMENT).unwrap().is_empty());
        drop(core);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn pending_comments_recover_exactly_and_reject_text_parent_link_or_type_tampering() {
        for mode in 0..5 {
            let path = temp_root("comment-recover");
            let vault = MemoryVault::seeded();
            let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
            core.unlock().unwrap();
            let (human, device) = core.signing_identity().unwrap();
            let parent = build_post(human, device, "parent", 1, 1).unwrap();
            let other = build_post(human, device, "other", 1, 2).unwrap();
            let base = build_comment(human, device, parent.id(), "exact", 77, 9).unwrap();
            let object = match mode {
                3 => mini_objects::ObjectBuilder::new(ObjectType::COMMENT)
                    .link("re", parent.id().clone())
                    .link("extra", other.id().clone())
                    .payload(base.payload.clone())
                    .sign(human, device)
                    .unwrap(),
                4 => build_post(human, device, "exact", 77, 9).unwrap(),
                _ => base,
            };
            let id = object.id().clone();
            core.store.insert(&parent).unwrap();
            let record = PendingMutation {
                operation_id: "comment:recover".into(),
                input: MutationInput::Comment {
                    parent_id: if mode == 2 {
                        other.id().as_str().into()
                    } else {
                        parent.id().as_str().into()
                    },
                    text: if mode == 1 {
                        "changed".into()
                    } else {
                        "exact".into()
                    },
                },
                objects: vec![object.to_bytes()],
            };
            core.journal.write_pending(&record).unwrap();
            drop(core);
            let recovered = Core::open_inner(path.clone(), vault, false);
            if mode != 0 {
                assert!(recovered.is_err());
                let store = Store::new(FsBackend::open(&path).unwrap());
                assert!(store.get(&id).is_err());
            } else {
                let mut core = recovered.unwrap();
                assert_eq!(core.store.get(&id).unwrap().to_bytes(), object.to_bytes());
                let Reply::Published(reply) = core
                    .handle(Command::PublishComment {
                        operation_id: "comment:recover".into(),
                        parent_id: parent.id().as_str().into(),
                        text: "exact".into(),
                    })
                    .unwrap()
                else {
                    panic!("wrong reply")
                };
                assert!(reply.duplicate);
                assert_eq!(reply.object_id, id.as_str());
                drop(core);
            }
            fs::remove_dir_all(path).unwrap();
        }
    }
    fn membership_test_card(
        core: &mut Core<MemoryVault>,
        mode: mini_social::MembershipMode,
    ) -> Object {
        let mut owner = Controller::incept_single_from_seeds(&[61; 32], &[62; 32]).unwrap();
        let device =
            Controller::incept_device_single_from_seeds(&owner.did(), &[63; 32], &[64; 32])
                .unwrap();
        owner
            .delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        mini_social::publish_community(
            &mut core.store,
            &owner.did(),
            &device,
            "Community",
            "Charter",
            mode,
            1,
            1,
        )
        .unwrap()
    }
    #[test]
    fn membership_open_join_leave_policy_and_locked_deleted_card_retry_are_exact() {
        let path = temp_root("membership");
        let vault = MemoryVault::seeded();
        let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
        core.unlock().unwrap();
        let member = core.human.clone().unwrap();
        let card = membership_test_card(&mut core, mini_social::MembershipMode::Open);
        let approval =
            membership_test_card(&mut core, mini_social::MembershipMode::ApprovalRequired);
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        assert!(core
            .handle(Command::SetMembership {
                operation_id: "membership:approval".into(),
                community_id: approval.id().as_str().into(),
                joined: true
            })
            .is_err());
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        core.handle(Command::SetMembership {
            operation_id: "membership:approval-leave".into(),
            community_id: approval.id().as_str().into(),
            joined: false,
        })
        .unwrap();
        let join = Command::SetMembership {
            operation_id: "membership:join".into(),
            community_id: card.id().as_str().into(),
            joined: true,
        };
        core.handle(join.clone()).unwrap();
        assert_eq!(
            mini_social::community_members(&core.store, card.id()).unwrap(),
            vec![member]
        );
        let leave = Command::SetMembership {
            operation_id: "membership:leave".into(),
            community_id: card.id().as_str().into(),
            joined: false,
        };
        let Reply::Published(published) = core.handle(leave.clone()).unwrap() else {
            panic!("wrong reply")
        };
        assert_eq!(published.kind, ObjectKind::Membership);
        assert!(mini_social::community_members(&core.store, card.id())
            .unwrap()
            .is_empty());
        let Reply::Published(stale) = core.handle(join).unwrap() else {
            panic!("wrong reply")
        };
        assert!(stale.duplicate);
        assert!(mini_social::community_members(&core.store, card.id())
            .unwrap()
            .is_empty());
        let id = ObjectId::parse(&published.object_id).unwrap();
        let bytes = core.store.get(&id).unwrap().to_bytes();
        drop(core);
        let mut core = Core::open_inner(path.clone(), vault, false).unwrap();
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        let parent = card.id().as_str();
        fs::remove_file(path.join("blobs").join(&parent[..2]).join(parent)).unwrap();
        let Reply::Published(retry) = core.handle(leave).unwrap() else {
            panic!("wrong reply")
        };
        assert!(retry.duplicate);
        assert_eq!(retry.object_id, published.object_id);
        assert_eq!(core.store.get(&id).unwrap().to_bytes(), bytes);
        assert!(core
            .handle(Command::SetMembership {
                operation_id: "membership:locked".into(),
                community_id: approval.id().as_str().into(),
                joined: false
            })
            .is_err());
        assert!(core
            .handle(Command::SetMembership {
                operation_id: "membership:leave".into(),
                community_id: card.id().as_str().into(),
                joined: true
            })
            .is_err());
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        drop(core);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn pending_membership_recovers_exactly_and_refuses_action_or_target_tampering() {
        for mode in 0..3 {
            let path = temp_root("membership-recover");
            let vault = MemoryVault::seeded();
            let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
            core.unlock().unwrap();
            let card = membership_test_card(&mut core, mini_social::MembershipMode::Open);
            let other =
                membership_test_card(&mut core, mini_social::MembershipMode::ApprovalRequired);
            let (member, device) = core.signing_identity().unwrap();
            let object = build_membership(member, device, card.id(), true, 77, 9).unwrap();
            let id = object.id().clone();
            let record = PendingMutation {
                operation_id: "membership:recover".into(),
                input: MutationInput::Membership {
                    community_id: if mode == 2 {
                        other.id().as_str().into()
                    } else {
                        card.id().as_str().into()
                    },
                    joined: mode != 1,
                },
                objects: vec![object.to_bytes()],
            };
            core.journal.write_pending(&record).unwrap();
            drop(core);
            let recovered = Core::open_inner(path.clone(), vault, false);
            if mode != 0 {
                assert!(recovered.is_err());
                let store = Store::new(FsBackend::open(&path).unwrap());
                assert!(store.get(&id).is_err());
            } else {
                let mut core = recovered.unwrap();
                assert_eq!(core.store.get(&id).unwrap().to_bytes(), object.to_bytes());
                let Reply::Published(reply) = core
                    .handle(Command::SetMembership {
                        operation_id: "membership:recover".into(),
                        community_id: card.id().as_str().into(),
                        joined: true,
                    })
                    .unwrap()
                else {
                    panic!("wrong reply")
                };
                assert!(reply.duplicate);
                assert_eq!(reply.object_id, id.as_str());
                drop(core);
            }
            fs::remove_dir_all(path).unwrap();
        }
    }
    #[test]
    fn community_creation_is_open_and_exact_locked_receipts_do_not_replace_newer_cards() {
        let path = temp_root("community-create");
        let vault = MemoryVault::seeded();
        let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
        let initial = fs::read(&core.journal.sequence).unwrap();
        assert!(core
            .handle(Command::PublishCommunity {
                operation_id: "community:locked".into(),
                name: "Name".into(),
                charter: String::new()
            })
            .is_err());
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), initial);
        core.unlock().unwrap();
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        for (index, (name, charter)) in [
            (" \n".into(), String::new()),
            ("x".repeat(MAX_COMMUNITY_NAME_BYTES + 1), String::new()),
            ("Name".into(), "x".repeat(MAX_COMMUNITY_CHARTER_BYTES + 1)),
        ]
        .into_iter()
        .enumerate()
        {
            assert!(core
                .handle(Command::PublishCommunity {
                    operation_id: format!("community:bad:{index}"),
                    name,
                    charter
                })
                .is_err());
        }
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        let command = Command::PublishCommunity {
            operation_id: "community:first".into(),
            name: "First".into(),
            charter: "Rules".into(),
        };
        let Reply::Published(first) = core.handle(command.clone()).unwrap() else {
            panic!("wrong reply")
        };
        assert_eq!(first.kind, ObjectKind::Community);
        let id = ObjectId::parse(&first.object_id).unwrap();
        let bytes = core.store.get(&id).unwrap().to_bytes();
        assert_eq!(
            mini_social::resolve_community(&core.store, &id)
                .unwrap()
                .membership,
            mini_social::MembershipMode::Open
        );
        core.handle(Command::PublishCommunity {
            operation_id: "community:second".into(),
            name: "Second".into(),
            charter: String::new(),
        })
        .unwrap();
        let Reply::Published(stale) = core.handle(command.clone()).unwrap() else {
            panic!("wrong reply")
        };
        assert!(stale.duplicate);
        assert_eq!(core.store.by_type(&ObjectType::COMMUNITY).unwrap().len(), 2);
        drop(core);
        let mut core = Core::open_inner(path.clone(), vault, false).unwrap();
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        let Reply::Published(retry) = core.handle(command).unwrap() else {
            panic!("wrong reply")
        };
        assert!(retry.duplicate);
        assert_eq!(retry.object_id, first.object_id);
        assert_eq!(core.store.get(&id).unwrap().to_bytes(), bytes);
        assert!(core
            .handle(Command::PublishCommunity {
                operation_id: "community:first".into(),
                name: "Changed".into(),
                charter: "Rules".into()
            })
            .is_err());
        assert!(core
            .handle(Command::PublishCommunity {
                operation_id: "community:new-locked".into(),
                name: "New".into(),
                charter: String::new()
            })
            .is_err());
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        drop(core);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn pending_community_recovers_exactly_and_refuses_name_charter_mode_link_type_tampering() {
        for mode in 0..6 {
            let path = temp_root("community-recover");
            let vault = MemoryVault::seeded();
            let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
            core.unlock().unwrap();
            let (owner, device) = core.signing_identity().unwrap();
            let base = build_community(
                owner,
                device,
                "Name",
                "Charter",
                if mode == 3 {
                    mini_social::MembershipMode::ApprovalRequired
                } else {
                    mini_social::MembershipMode::Open
                },
                77,
                9,
            )
            .unwrap();
            let object = match mode {
                4 => mini_objects::ObjectBuilder::new(ObjectType::COMMUNITY)
                    .link("extra", base.id().clone())
                    .payload(base.payload.clone())
                    .sign(owner, device)
                    .unwrap(),
                5 => build_post(owner, device, "Name", 77, 9).unwrap(),
                _ => base,
            };
            let id = object.id().clone();
            let record = PendingMutation {
                operation_id: "community:recover".into(),
                input: MutationInput::Community {
                    name: if mode == 1 {
                        "Changed".into()
                    } else {
                        "Name".into()
                    },
                    charter: if mode == 2 {
                        "Changed".into()
                    } else {
                        "Charter".into()
                    },
                },
                objects: vec![object.to_bytes()],
            };
            core.journal.write_pending(&record).unwrap();
            drop(core);
            let recovered = Core::open_inner(path.clone(), vault, false);
            if mode != 0 {
                assert!(recovered.is_err());
                let store = Store::new(FsBackend::open(&path).unwrap());
                assert!(store.get(&id).is_err());
            } else {
                let mut core = recovered.unwrap();
                assert_eq!(core.store.get(&id).unwrap().to_bytes(), object.to_bytes());
                let Reply::Published(reply) = core
                    .handle(Command::PublishCommunity {
                        operation_id: "community:recover".into(),
                        name: "Name".into(),
                        charter: "Charter".into(),
                    })
                    .unwrap()
                else {
                    panic!("wrong reply")
                };
                assert!(reply.duplicate);
                assert_eq!(reply.object_id, id.as_str());
                drop(core);
            }
            fs::remove_dir_all(path).unwrap();
        }
    }
    #[test]
    fn public_delivery_preview_and_queue_require_exact_own_identity_and_expose_only_review() {
        let path = temp_root("social-delivery-preview");
        let mut core = Core::open_inner(path.clone(), MemoryVault::seeded(), false).unwrap();
        core.unlock().unwrap();
        let Reply::Published(post) = core
            .handle(Command::PublishPost {
                operation_id: "preview:parent".into(),
                text: "parent".into(),
            })
            .unwrap()
        else {
            panic!("wrong reply")
        };
        let Reply::Published(comment) = core
            .handle(Command::PublishComment {
                operation_id: "preview:comment".into(),
                parent_id: post.object_id.clone(),
                text: "public reviewed reply".into(),
            })
            .unwrap()
        else {
            panic!("wrong reply")
        };
        let preview = Command::PreviewPublicDelivery {
            object_id: comment.object_id.clone(),
        };
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let Reply::PublicDeliveryObject(view) = core.handle(preview.clone()).unwrap() else {
            panic!("wrong reply")
        };
        assert_eq!(view.kind, ObjectKind::Comment);
        assert_eq!(view.summary, "public reviewed reply");
        assert_eq!(view.targets, vec![post.object_id]);
        assert_eq!(view.object_id, comment.object_id);
        let mut peer_root = Controller::incept_single_from_seeds(&[71; 32], &[72; 32]).unwrap();
        let peer_device =
            Controller::incept_device_single_from_seeds(&peer_root.did(), &[73; 32], &[74; 32])
                .unwrap();
        peer_root
            .delegate_device(&peer_device.did(), Capabilities::primary())
            .unwrap();
        core.peers
            .authorize(
                "friend",
                "127.0.0.1:7400",
                &peer_root.kel().to_bytes(),
                &peer_device.kel().to_bytes(),
            )
            .unwrap();
        let queue = Command::QueuePublicDelivery {
            operation_id: "preview:queue".into(),
            peer_id: "friend".into(),
            object_id: comment.object_id.clone(),
        };
        assert!(matches!(
            core.handle(queue.clone()).unwrap(),
            Reply::Delivery(_)
        ));
        let foreign = mini_social::build_comment(
            &peer_root.did(),
            &peer_device,
            &ObjectId::parse(&comment.object_id).unwrap(),
            "foreign reply",
            1,
            1,
        )
        .unwrap();
        core.store.insert(&foreign).unwrap();
        assert!(core
            .handle(Command::PreviewPublicDelivery {
                object_id: foreign.id().as_str().into()
            })
            .is_err());
        assert!(core
            .handle(Command::QueuePublicDelivery {
                operation_id: "preview:foreign".into(),
                peer_id: "friend".into(),
                object_id: foreign.id().as_str().into()
            })
            .is_err());
        let (human, device) = core.signing_identity().unwrap();
        let private = mini_objects::ObjectBuilder::new(ObjectType::POST)
            .payload(Payload::Encrypted(vec![1]))
            .sign(human, device)
            .unwrap();
        core.store.insert(&private).unwrap();
        assert!(core
            .handle(Command::PreviewPublicDelivery {
                object_id: private.id().as_str().into()
            })
            .is_err());
        core.lock();
        assert!(core.handle(preview).is_err());
        assert!(core.handle(queue).is_err());
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        drop(core);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn maximum_comment_publication_locked_receipt_and_pending_recovery_keep_journal_usable() {
        let path = temp_root("maximum-comment");
        let vault = MemoryVault::seeded();
        let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
        core.unlock().unwrap();
        let Reply::Published(parent) = core
            .handle(Command::PublishPost {
                operation_id: "max-comment:parent".into(),
                text: "parent".into(),
            })
            .unwrap()
        else {
            panic!("wrong reply")
        };
        let maximum = "x".repeat(MAX_COMMENT_BYTES);
        let command = Command::PublishComment {
            operation_id: "max-comment:published".into(),
            parent_id: parent.object_id.clone(),
            text: maximum.clone(),
        };
        let Reply::Published(published) = core.handle(command.clone()).unwrap() else {
            panic!("wrong reply")
        };
        assert_eq!(fs::read_dir(&core.journal.pending).unwrap().count(), 0);
        let published_id = ObjectId::parse(&published.object_id).unwrap();
        let published_bytes = core.store.get(&published_id).unwrap().to_bytes();
        let utf8 = "🦀".repeat(MAX_COMMENT_BYTES / 4);
        let (human, device) = core.signing_identity().unwrap();
        let pending = build_comment(
            human,
            device,
            &ObjectId::parse(&parent.object_id).unwrap(),
            &utf8,
            77,
            99,
        )
        .unwrap();
        let pending_id = pending.id().clone();
        let record = PendingMutation {
            operation_id: "max-comment:recover".into(),
            input: MutationInput::Comment {
                parent_id: parent.object_id.clone(),
                text: utf8.clone(),
            },
            objects: vec![pending.to_bytes()],
        };
        core.journal.write_pending(&record).unwrap();
        drop(core);
        let mut core = Core::open_inner(path.clone(), vault, false).unwrap();
        for request in [
            command,
            Command::PublishComment {
                operation_id: "max-comment:recover".into(),
                parent_id: parent.object_id.clone(),
                text: utf8,
            },
        ] {
            let Reply::Published(receipt) = core.handle(request).unwrap() else {
                panic!("wrong reply")
            };
            assert!(receipt.duplicate);
        }
        assert_eq!(
            core.store.get(&published_id).unwrap().to_bytes(),
            published_bytes
        );
        assert_eq!(
            core.store.get(&pending_id).unwrap().to_bytes(),
            pending.to_bytes()
        );
        assert_eq!(
            comments(&core.store, &ObjectId::parse(&parent.object_id).unwrap())
                .unwrap()
                .len(),
            2
        );
        assert_eq!(fs::read_dir(&core.journal.pending).unwrap().count(), 0);
        drop(core);
        fs::remove_dir_all(path).unwrap();
    }
    fn details_test_draft() -> PublicProfileDetails {
        PublicProfileDetails {
            display_name: "Alice".into(),
            bio: "About".into(),
            avatar: None,
            location: Some("Chosen city".into()),
            age: Some(25),
            fields: vec![
                mini_app_protocol::ProfileField {
                    label: "One".into(),
                    value: "First".into(),
                },
                mini_app_protocol::ProfileField {
                    label: "Two".into(),
                    value: "Second".into(),
                },
            ],
        }
    }
    #[test]
    fn profile_details_all_fields_and_avatar_keep_exact_locked_receipts_and_invalid_input_no_sequence(
    ) {
        let path = temp_root("profile-details");
        let vault = MemoryVault::seeded();
        let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
        core.unlock().unwrap();
        let mut image_root = Controller::incept_single_from_seeds(&[81; 32], &[82; 32]).unwrap();
        let image_device =
            Controller::incept_device_single_from_seeds(&image_root.did(), &[83; 32], &[84; 32])
                .unwrap();
        image_root
            .delegate_device(&image_device.did(), Capabilities::primary())
            .unwrap();
        let manifest = mini_media::publish_media(
            &mut core.store,
            &image_root.did(),
            &image_device,
            "image/png",
            b"public image fixture",
            1,
            1,
        )
        .unwrap();
        let mut draft = details_test_draft();
        draft.avatar = Some(manifest.id.as_str().into());
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        let missing = build_post(&image_root.did(), &image_device, "missing", 1, 1).unwrap();
        let wrong = build_post(&image_root.did(), &image_device, "not an image", 1, 2).unwrap();
        core.store.insert(&wrong).unwrap();
        let mut payload = Vec::new();
        payload.extend_from_slice(&9u32.to_be_bytes());
        payload.extend_from_slice(b"image/png");
        payload.extend_from_slice(&(8u64 * 1024 * 1024 + 1).to_be_bytes());
        payload.extend_from_slice(&manifest.digest);
        let mut oversized = mini_objects::ObjectBuilder::new(ObjectType::MEDIA_MANIFEST)
            .payload(Payload::Public(payload));
        for _ in 0..9 {
            oversized = oversized.link("chunk", manifest.chunks[0].clone());
        }
        let oversized = oversized.sign(&image_root.did(), &image_device).unwrap();
        assert!(mini_media::read_manifest(&oversized).unwrap().total_len > 8 * 1024 * 1024);
        core.store.insert(&oversized).unwrap();
        for invalid in [
            PublicProfileDetails {
                avatar: Some(oversized.id().as_str().into()),
                ..draft.clone()
            },
            PublicProfileDetails {
                location: Some(String::new()),
                ..draft.clone()
            },
            PublicProfileDetails {
                age: Some(0),
                ..draft.clone()
            },
            PublicProfileDetails {
                avatar: Some(missing.id().as_str().into()),
                ..draft.clone()
            },
            PublicProfileDetails {
                avatar: Some(wrong.id().as_str().into()),
                ..draft.clone()
            },
        ] {
            assert!(core
                .handle(Command::PublishProfileDetails {
                    operation_id: "details:invalid".into(),
                    draft: invalid
                })
                .is_err());
        }
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        let command = Command::PublishProfileDetails {
            operation_id: "details:1".into(),
            draft: draft.clone(),
        };
        let Reply::Published(published) = core.handle(command.clone()).unwrap() else {
            panic!("wrong reply")
        };
        let id = ObjectId::parse(&published.object_id).unwrap();
        let bytes = core.store.get(&id).unwrap().to_bytes();
        let profile = mini_social::decode_profile(&core.store.get(&id).unwrap()).unwrap();
        assert_eq!(profile.display_name, draft.display_name);
        assert_eq!(profile.bio, draft.bio);
        assert_eq!(profile.location, draft.location);
        assert_eq!(profile.age, draft.age);
        assert_eq!(
            profile.avatar.as_ref().map(|id| id.as_str()),
            draft.avatar.as_deref()
        );
        assert_eq!(profile.fields[0].label, "One");
        assert_eq!(profile.fields[1].label, "Two");
        drop(core);
        let mut core = Core::open_inner(path.clone(), vault, false).unwrap();
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        let avatar = manifest.id.as_str();
        fs::remove_file(path.join("blobs").join(&avatar[..2]).join(avatar)).unwrap();
        let Reply::Published(retry) = core.handle(command).unwrap() else {
            panic!("wrong reply")
        };
        assert!(retry.duplicate);
        assert_eq!(retry.object_id, published.object_id);
        assert_eq!(core.store.get(&id).unwrap().to_bytes(), bytes);
        assert!(core
            .handle(Command::PublishProfileDetails {
                operation_id: "details:1".into(),
                draft: PublicProfileDetails {
                    bio: "changed".into(),
                    ..draft.clone()
                }
            })
            .is_err());
        assert!(core
            .handle(Command::PublishProfileDetails {
                operation_id: "details:new-locked".into(),
                draft
            })
            .is_err());
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        drop(core);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn pending_profile_details_validate_all_objects_before_any_insert() {
        for mode in 0..6 {
            let path = temp_root("details-recover");
            let vault = MemoryVault::seeded();
            let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
            core.unlock().unwrap();
            let mut draft = details_test_draft();
            let (avatar, fields) = profile_details_values(&draft).unwrap();
            let public = mini_social::PublicProfileDraft {
                display_name: &draft.display_name,
                bio: &draft.bio,
                avatar: avatar.as_ref(),
                location: draft.location.as_deref(),
                age: draft.age,
                fields: &fields,
            };
            let (human, device) = core.signing_identity().unwrap();
            let (profile, normal_head) =
                build_profile_details(human, device, &public, 77, 9).unwrap();
            let head = if mode >= 3 {
                mini_objects::ObjectBuilder::new(ObjectType::HEAD)
                    .timestamp_ms(if mode == 3 { 78 } else { 77 })
                    .sequence(if mode == 4 { 10 } else { 9 })
                    .link("target", profile.id().clone())
                    .payload(Payload::Public(if mode == 5 {
                        b"wrong".to_vec()
                    } else {
                        b"profile".to_vec()
                    }))
                    .sign(human, device)
                    .unwrap()
            } else {
                normal_head
            };
            if mode == 1 {
                draft.age = Some(26);
            }
            if mode == 2 {
                draft.fields.reverse();
            }
            let record = PendingMutation {
                operation_id: "details:recover".into(),
                input: MutationInput::ProfileDetails {
                    draft: draft.clone(),
                },
                objects: vec![profile.to_bytes(), head.to_bytes()],
            };
            core.journal.write_pending(&record).unwrap();
            drop(core);
            let recovered = Core::open_inner(path.clone(), vault, false);
            if mode != 0 {
                assert!(recovered.is_err());
                let store = Store::new(FsBackend::open(&path).unwrap());
                assert!(store.get(profile.id()).is_err());
                assert!(store.get(head.id()).is_err());
            } else {
                let mut core = recovered.unwrap();
                assert_eq!(
                    core.store.get(profile.id()).unwrap().to_bytes(),
                    profile.to_bytes()
                );
                let Reply::Published(reply) = core
                    .handle(Command::PublishProfileDetails {
                        operation_id: "details:recover".into(),
                        draft,
                    })
                    .unwrap()
                else {
                    panic!("wrong reply")
                };
                assert!(reply.duplicate);
                assert_eq!(reply.object_id, profile.id().as_str());
                drop(core);
            }
            fs::remove_dir_all(path).unwrap();
        }
    }
    #[test]
    fn profile_photo_publishes_exact_four_objects_and_locked_receipt_while_invalid_input_does_not_reserve(
    ) {
        let path = temp_root("profile-photo");
        let vault = MemoryVault::seeded();
        let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
        core.unlock().unwrap();
        let draft = details_test_draft();
        let photo = profile_photo::test_jpeg(2, 2);
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        for (details, bytes) in [
            (draft.clone(), b"not jpeg".to_vec()),
            (draft.clone(), profile_photo::test_jpeg(257, 1)),
            (
                draft.clone(),
                vec![0; mini_app_protocol::MAX_PROFILE_PHOTO_BYTES + 1],
            ),
            (
                PublicProfileDetails {
                    avatar: Some("existing".into()),
                    ..draft.clone()
                },
                photo.clone(),
            ),
        ] {
            assert!(core
                .handle(Command::PublishProfileWithPhoto {
                    operation_id: "photo:invalid".into(),
                    draft: details,
                    photo: bytes
                })
                .is_err());
        }
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        let command = Command::PublishProfileWithPhoto {
            operation_id: "photo:1".into(),
            draft: draft.clone(),
            photo: photo.clone(),
        };
        let Reply::Published(published) = core.handle(command.clone()).unwrap() else {
            panic!("wrong reply")
        };
        let id = ObjectId::parse(&published.object_id).unwrap();
        let object = core.store.get(&id).unwrap();
        let profile = mini_social::decode_profile(&object).unwrap();
        assert_eq!(profile.location, draft.location);
        assert_eq!(profile.age, draft.age);
        assert_eq!(profile.fields[0].label, draft.fields[0].label);
        let avatar = profile.avatar.unwrap();
        let media = mini_media::read_manifest(&core.store.get(&avatar).unwrap()).unwrap();
        assert_eq!(mini_media::assemble(&core.store, &media).unwrap(), photo);
        assert_eq!(core.next_sequence, next + 2);
        assert_eq!(core.store.all_ids().unwrap().len(), 4);
        let receipt_bytes = fs::read(core.journal.receipt_path("photo:1")).unwrap();
        assert!(receipt_bytes.len() < 4096);
        drop(core);
        let mut core = Core::open_inner(path.clone(), vault, false).unwrap();
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        let media_id = avatar.as_str();
        fs::remove_file(path.join("blobs").join(&media_id[..2]).join(media_id)).unwrap();
        let Reply::Published(retry) = core.handle(command).unwrap() else {
            panic!("wrong reply")
        };
        assert!(retry.duplicate);
        assert_eq!(retry.object_id, published.object_id);
        assert_eq!(core.store.get(&id).unwrap().to_bytes(), object.to_bytes());
        assert!(core
            .handle(Command::PublishProfileWithPhoto {
                operation_id: "photo:1".into(),
                draft,
                photo: profile_photo::test_jpeg(3, 3)
            })
            .is_err());
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        drop(core);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn photo_profile_pending_recovery_validates_every_object_before_inserting() {
        for mode in 0..4 {
            let path = temp_root("photo-recover");
            let vault = MemoryVault::seeded();
            let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
            core.unlock().unwrap();
            let draft = details_test_draft();
            let photo = profile_photo::test_jpeg(2, 2);
            let (human, device) = core.signing_identity().unwrap();
            let mut objects = profile_photo::build(human, device, &draft, &photo, 10, 77).unwrap();
            if mode == 1 {
                objects[0] = mini_objects::ObjectBuilder::new(ObjectType::Custom(
                    mini_media::CHUNK_TYPE.into(),
                ))
                .sequence(10)
                .timestamp_ms(77)
                .payload(Payload::Public(profile_photo::test_jpeg(3, 3)))
                .sign(human, device)
                .unwrap()
                .to_bytes();
            }
            if mode == 2 {
                let original = Object::from_bytes(&objects[1]).unwrap();
                objects[1] = mini_objects::ObjectBuilder::new(ObjectType::MEDIA_MANIFEST)
                    .sequence(11)
                    .timestamp_ms(77)
                    .link(
                        "chunk",
                        Object::from_bytes(&objects[0]).unwrap().id().clone(),
                    )
                    .payload(original.payload)
                    .sign(human, device)
                    .unwrap()
                    .to_bytes();
            }
            if mode == 3 {
                objects[3] = mini_objects::ObjectBuilder::new(ObjectType::HEAD)
                    .sequence(11)
                    .timestamp_ms(78)
                    .link(
                        "target",
                        Object::from_bytes(&objects[2]).unwrap().id().clone(),
                    )
                    .payload(Payload::Public(b"profile".to_vec()))
                    .sign(human, device)
                    .unwrap()
                    .to_bytes();
            }
            let ids = objects
                .iter()
                .map(|bytes| Object::from_bytes(bytes).unwrap().id().clone())
                .collect::<Vec<_>>();
            let record = PendingMutation {
                operation_id: "photo:recover".into(),
                input: MutationInput::PhotoProfile {
                    draft: draft.clone(),
                    photo_digest: profile_photo::digest(&photo),
                    photo_len: photo.len() as u32,
                },
                objects,
            };
            core.journal.write_pending(&record).unwrap();
            drop(core);
            let recovered = Core::open_inner(path.clone(), vault, false);
            if mode != 0 {
                assert!(recovered.is_err());
                let store = Store::new(FsBackend::open(&path).unwrap());
                for id in ids {
                    assert!(store.get(&id).is_err());
                }
            } else {
                let mut core = recovered.unwrap();
                for (id, bytes) in ids.iter().zip(&record.objects) {
                    assert_eq!(core.store.get(id).unwrap().to_bytes(), *bytes);
                }
                let Reply::Published(reply) = core
                    .handle(Command::PublishProfileWithPhoto {
                        operation_id: "photo:recover".into(),
                        draft,
                        photo,
                    })
                    .unwrap()
                else {
                    panic!("wrong reply")
                };
                assert!(reply.duplicate);
                drop(core);
            }
            fs::remove_dir_all(path).unwrap();
        }
    }
    #[test]
    fn photo_sequence_range_overflow_never_partially_reserves() {
        let path = temp_root("photo-overflow");
        let mut core = Core::open_inner(path.clone(), MemoryVault::seeded(), false).unwrap();
        core.unlock().unwrap();
        core.next_sequence = u64::MAX - 1;
        let before = fs::read(&core.journal.sequence).unwrap();
        assert!(core.reserve_sequences(2).is_err());
        assert_eq!(core.next_sequence, u64::MAX - 1);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), before);
        drop(core);
        fs::remove_dir_all(path).unwrap();
    }
    fn wall_test_draft() -> PublicWallDraft {
        PublicWallDraft {
            display_name: "Wall".into(),
            bio: "Bio".into(),
            public_links: vec!["https://example.test".into()],
            visibility: mini_app_protocol::WallVisibility::Public,
        }
    }
    #[test]
    fn wall_public_and_unlisted_heads_keep_locked_exact_receipts_and_no_profile_confusion() {
        let path = temp_root("wall");
        let vault = MemoryVault::seeded();
        let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
        let initial = fs::read(&core.journal.sequence).unwrap();
        assert!(core
            .handle(Command::PublishWall {
                operation_id: "wall:locked".into(),
                draft: wall_test_draft()
            })
            .is_err());
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), initial);
        core.unlock().unwrap();
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        assert!(core
            .handle(Command::PublishWall {
                operation_id: "wall:invalid".into(),
                draft: PublicWallDraft {
                    display_name: String::new(),
                    ..wall_test_draft()
                }
            })
            .is_err());
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        let command = Command::PublishWall {
            operation_id: "wall:public".into(),
            draft: wall_test_draft(),
        };
        let Reply::Published(first) = core.handle(command.clone()).unwrap() else {
            panic!("wrong reply")
        };
        assert_eq!(first.kind, ObjectKind::Wall);
        let id = ObjectId::parse(&first.object_id).unwrap();
        let bytes = core.store.get(&id).unwrap().to_bytes();
        let owner = core.human.clone().unwrap();
        assert_eq!(
            mini_social::resolve_wall(&core.store, &owner)
                .unwrap()
                .unwrap()
                .visibility,
            mini_social::VisibilityPolicy::Public
        );
        core.handle(Command::PublishWall {
            operation_id: "wall:unlisted".into(),
            draft: PublicWallDraft {
                visibility: mini_app_protocol::WallVisibility::Unlisted,
                ..wall_test_draft()
            },
        })
        .unwrap();
        let current = mini_social::resolve_wall(&core.store, &owner)
            .unwrap()
            .unwrap();
        assert_eq!(current.visibility, mini_social::VisibilityPolicy::Unlisted);
        assert!(mini_social::resolve_profile(&core.store, &owner)
            .unwrap()
            .is_none());
        let Reply::Published(stale) = core.handle(command.clone()).unwrap() else {
            panic!("wrong reply")
        };
        assert!(stale.duplicate);
        assert_eq!(
            mini_social::resolve_wall(&core.store, &owner)
                .unwrap()
                .unwrap()
                .wall_id,
            current.wall_id
        );
        assert!(core
            .handle(Command::PreviewPublicDelivery {
                object_id: first.object_id.clone()
            })
            .is_err());
        drop(core);
        let mut core = Core::open_inner(path.clone(), vault, false).unwrap();
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        let Reply::Published(retry) = core.handle(command).unwrap() else {
            panic!("wrong reply")
        };
        assert!(retry.duplicate);
        assert_eq!(retry.object_id, first.object_id);
        assert_eq!(core.store.get(&id).unwrap().to_bytes(), bytes);
        assert!(core
            .handle(Command::PublishWall {
                operation_id: "wall:public".into(),
                draft: PublicWallDraft {
                    bio: "changed".into(),
                    ..wall_test_draft()
                }
            })
            .is_err());
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        drop(core);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn pending_wall_checks_visibility_header_links_head_label_and_time_before_any_insert() {
        for mode in 0..5 {
            let path = temp_root("wall-recover");
            let vault = MemoryVault::seeded();
            let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
            core.unlock().unwrap();
            let mut draft = wall_test_draft();
            let (owner, device) = core.signing_identity().unwrap();
            let (base, normal_head) = mini_social::build_wall(
                owner,
                device,
                &draft.display_name,
                &draft.bio,
                None,
                &["https://example.test"],
                &[],
                mini_social::VisibilityPolicy::Public,
                77,
                9,
            )
            .unwrap();
            let wall = if mode == 2 {
                mini_objects::ObjectBuilder::new(ObjectType::WALL)
                    .sequence(9)
                    .timestamp_ms(77)
                    .link("extra", base.id().clone())
                    .payload(base.payload.clone())
                    .sign(owner, device)
                    .unwrap()
            } else {
                base
            };
            let head = if mode >= 3 || mode == 2 {
                mini_objects::ObjectBuilder::new(ObjectType::HEAD)
                    .sequence(9)
                    .timestamp_ms(if mode == 4 { 78 } else { 77 })
                    .link("target", wall.id().clone())
                    .payload(Payload::Public(if mode == 3 {
                        b"profile".to_vec()
                    } else {
                        b"wall".to_vec()
                    }))
                    .sign(owner, device)
                    .unwrap()
            } else {
                normal_head
            };
            if mode == 1 {
                draft.visibility = mini_app_protocol::WallVisibility::Unlisted;
            }
            let record = PendingMutation {
                operation_id: "wall:recover".into(),
                input: MutationInput::Wall {
                    draft: draft.clone(),
                },
                objects: vec![wall.to_bytes(), head.to_bytes()],
            };
            core.journal.write_pending(&record).unwrap();
            drop(core);
            let recovered = Core::open_inner(path.clone(), vault, false);
            if mode != 0 {
                assert!(recovered.is_err());
                let store = Store::new(FsBackend::open(&path).unwrap());
                assert!(store.get(wall.id()).is_err());
                assert!(store.get(head.id()).is_err());
            } else {
                let mut core = recovered.unwrap();
                assert_eq!(
                    core.store.get(wall.id()).unwrap().to_bytes(),
                    wall.to_bytes()
                );
                let Reply::Published(reply) = core
                    .handle(Command::PublishWall {
                        operation_id: "wall:recover".into(),
                        draft,
                    })
                    .unwrap()
                else {
                    panic!("wrong reply")
                };
                assert!(reply.duplicate);
                drop(core);
            }
            fs::remove_dir_all(path).unwrap();
        }
    }
    fn media_share_test_headers(human: &Did, device: &Controller) -> Vec<Object> {
        let missing = build_post(human, device, "descendant absent by design", 1, 1).unwrap();
        let mut payload = Vec::new();
        payload.extend_from_slice(&24u32.to_be_bytes());
        payload.extend_from_slice(b"application/octet-stream");
        payload.extend_from_slice(&1u64.to_be_bytes());
        payload.extend_from_slice(&[0; 32]);
        let manifest = mini_objects::ObjectBuilder::new(ObjectType::MEDIA_MANIFEST)
            .link("chunk", missing.id().clone())
            .payload(Payload::Public(payload.clone()))
            .sign(human, device)
            .unwrap();
        let superblock = mini_objects::ObjectBuilder::new(ObjectType::Custom(
            mini_media::SUPERBLOCK_TYPE.into(),
        ))
        .link("part", missing.id().clone())
        .payload(Payload::Public(payload))
        .sign(human, device)
        .unwrap();
        let mut payload = vec![1];
        for value in ["file.bin", "application/octet-stream"] {
            payload.extend_from_slice(&(value.len() as u32).to_be_bytes());
            payload.extend_from_slice(value.as_bytes());
        }
        payload.extend_from_slice(&1u64.to_be_bytes());
        let collection = mini_objects::ObjectBuilder::new(ObjectType::Custom(
            mini_media::COLLECTION_TYPE.into(),
        ))
        .link("part", missing.id().clone())
        .payload(Payload::Public(payload))
        .sign(human, device)
        .unwrap();
        vec![manifest, superblock, collection]
    }
    #[test]
    fn media_share_accepts_three_public_headers_without_fetch_and_keeps_locked_exact_receipts() {
        let path = temp_root("media-share");
        let vault = MemoryVault::seeded();
        let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
        core.unlock().unwrap();
        let (human, device) = core.signing_identity().unwrap();
        let headers = media_share_test_headers(human, device);
        let mut published = Vec::new();
        for (index, header) in headers.iter().enumerate() {
            core.store.insert(header).unwrap();
            let Reply::Published(post) = core
                .handle(Command::PublishMediaPost {
                    operation_id: format!("media-share:{index}"),
                    media_id: header.id().as_str().into(),
                    caption: String::new(),
                })
                .unwrap()
            else {
                panic!("wrong reply")
            };
            assert_eq!(post.kind, ObjectKind::MediaPost);
            published.push(post);
        }
        let target = headers[0].id().as_str();
        let old = Command::PublishMediaPost {
            operation_id: "media-share:0".into(),
            media_id: target.into(),
            caption: String::new(),
        };
        core.handle(Command::PublishMediaPost {
            operation_id: "media-share:newer".into(),
            media_id: target.into(),
            caption: "new caption".into(),
        })
        .unwrap();
        let before = core.store.by_type(&ObjectType::POST).unwrap().len();
        let Reply::Published(stale) = core.handle(old.clone()).unwrap() else {
            panic!("wrong reply")
        };
        assert!(stale.duplicate);
        assert_eq!(core.store.by_type(&ObjectType::POST).unwrap().len(), before);
        let post_id = ObjectId::parse(&published[0].object_id).unwrap();
        let bytes = core.store.get(&post_id).unwrap().to_bytes();
        assert!(core
            .handle(Command::PreviewPublicDelivery {
                object_id: published[0].object_id.clone()
            })
            .is_err());
        drop(core);
        let mut core = Core::open_inner(path.clone(), vault, false).unwrap();
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        fs::remove_file(path.join("blobs").join(&target[..2]).join(target)).unwrap();
        let Reply::Published(retry) = core.handle(old).unwrap() else {
            panic!("wrong reply")
        };
        assert!(retry.duplicate);
        assert_eq!(retry.object_id, published[0].object_id);
        assert_eq!(core.store.get(&post_id).unwrap().to_bytes(), bytes);
        assert!(core
            .handle(Command::PublishMediaPost {
                operation_id: "media-share:0".into(),
                media_id: target.into(),
                caption: "changed".into()
            })
            .is_err());
        assert!(core
            .handle(Command::PublishMediaPost {
                operation_id: "media-share:locked".into(),
                media_id: headers[1].id().as_str().into(),
                caption: String::new()
            })
            .is_err());
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        drop(core);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn missing_private_unrelated_and_noncanonical_media_headers_never_reserve_sequence() {
        let path = temp_root("media-share-invalid");
        let mut core = Core::open_inner(path.clone(), MemoryVault::seeded(), false).unwrap();
        core.unlock().unwrap();
        let (human, device) = core.signing_identity().unwrap();
        let headers = media_share_test_headers(human, device);
        let private = mini_objects::ObjectBuilder::new(ObjectType::MEDIA_MANIFEST)
            .payload(Payload::Encrypted(vec![1]))
            .sign(human, device)
            .unwrap();
        let unrelated = build_post(human, device, "not media", 1, 1).unwrap();
        let extra = mini_objects::ObjectBuilder::new(ObjectType::MEDIA_MANIFEST)
            .link("chunk", headers[0].links[0].target.clone())
            .link("extra", unrelated.id().clone())
            .payload(headers[0].payload.clone())
            .sign(human, device)
            .unwrap();
        for object in [&private, &unrelated, &extra] {
            core.store.insert(object).unwrap();
        }
        let sequence = fs::read(&core.journal.sequence).unwrap();
        let next = core.next_sequence;
        for (index, id) in [headers[0].id(), private.id(), unrelated.id(), extra.id()]
            .into_iter()
            .enumerate()
        {
            assert!(core
                .handle(Command::PublishMediaPost {
                    operation_id: format!("bad-media:{index}"),
                    media_id: id.as_str().into(),
                    caption: String::new()
                })
                .is_err());
        }
        assert_eq!(core.next_sequence, next);
        assert_eq!(fs::read(&core.journal.sequence).unwrap(), sequence);
        drop(core);
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn pending_media_share_recovery_binds_exact_caption_media_and_no_extra_or_zero_links() {
        for mode in 0..5 {
            let path = temp_root("media-share-recover");
            let vault = MemoryVault::seeded();
            let mut core = Core::open_inner(path.clone(), vault.clone(), false).unwrap();
            core.unlock().unwrap();
            let (human, device) = core.signing_identity().unwrap();
            let headers = media_share_test_headers(human, device);
            let base = mini_social::build_media_post(
                human,
                device,
                headers[0].id().clone(),
                "caption",
                77,
                9,
            )
            .unwrap();
            let object = match mode {
                3 => mini_objects::ObjectBuilder::new(ObjectType::POST)
                    .link("media", headers[0].id().clone())
                    .link("extra", headers[1].id().clone())
                    .payload(base.payload.clone())
                    .sign(human, device)
                    .unwrap(),
                4 => build_post(human, device, "caption", 77, 9).unwrap(),
                _ => base,
            };
            let id = object.id().clone();
            let record = PendingMutation {
                operation_id: "media-share:recover".into(),
                input: MutationInput::MediaPost {
                    media_id: headers[if mode == 2 { 1 } else { 0 }].id().as_str().into(),
                    caption: if mode == 1 {
                        "changed".into()
                    } else {
                        "caption".into()
                    },
                },
                objects: vec![object.to_bytes()],
            };
            core.journal.write_pending(&record).unwrap();
            drop(core);
            let recovered = Core::open_inner(path.clone(), vault, false);
            if mode != 0 {
                assert!(recovered.is_err());
                let store = Store::new(FsBackend::open(&path).unwrap());
                assert!(store.get(&id).is_err());
            } else {
                let mut core = recovered.unwrap();
                assert_eq!(core.store.get(&id).unwrap().to_bytes(), object.to_bytes());
                let Reply::Published(reply) = core
                    .handle(Command::PublishMediaPost {
                        operation_id: "media-share:recover".into(),
                        media_id: headers[0].id().as_str().into(),
                        caption: "caption".into(),
                    })
                    .unwrap()
                else {
                    panic!("wrong reply")
                };
                assert!(reply.duplicate);
                assert_eq!(reply.kind, ObjectKind::MediaPost);
                drop(core);
            }
            fs::remove_dir_all(path).unwrap();
        }
    }
}
