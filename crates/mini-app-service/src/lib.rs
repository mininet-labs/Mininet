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

use did_mini::{Capabilities, Controller, Did};
use mini_app_protocol::{
    AccountStatus, Command, ErrorCode, FeedCard, FeedOrder, FeedReason, FeedScope, ObjectKind,
    ProfileView, PublishedObject, Reply, ServiceError, ServiceEvent, ServiceEventKind,
    MAX_OPERATION_ID_BYTES, MAX_POST_BYTES, MAX_PROFILE_BIO_BYTES, MAX_PROFILE_NAME_BYTES,
};
use mini_objects::{Object, ObjectId, ObjectType};
use mini_social::{
    build_post, build_profile, comments, feed, following, reaction_counts, resolve_post,
    resolve_profile, FeedFilter, FeedReason as SocialFeedReason, PostKind,
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
const MAX_SIGNED_OBJECT_BYTES: usize = 128 * 1024;
const PENDING_MAGIC: &[u8; 8] = b"MINIOP1P";
const RECEIPT_MAGIC: &[u8; 8] = b"MINIOP1R";
const SEQUENCE_MAGIC: &[u8; 8] = b"MINISEQ1";

#[derive(Debug)]
struct CoreIdentity {
    root: Controller,
    device: Controller,
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
        mini_windows_vault::load_or_create(&self.root.join("device.dpapi"))
            .map_err(Self::map_vault)
    }
}

/// Long-lived application authority. The production specialization holds an
/// exclusive process lock for its lifetime.
#[derive(Debug)]
pub struct Core<V: IdentityVault> {
    store: Store<FsBackend>,
    vault: V,
    identity: Option<CoreIdentity>,
    human: Option<Did>,
    next_sequence: u64,
    journal: MutationJournal,
    events: VecDeque<ServiceEvent>,
    next_event_id: u64,
    _process_lock: Option<File>,
}

impl Core<WindowsIdentityVault> {
    /// Open the production core and refuse a second live writer.
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
            events: VecDeque::new(),
            next_event_id: 1,
            _process_lock: process_lock,
        })
    }

    pub fn handle(&mut self, command: Command) -> Result<Reply, ServiceError> {
        match command {
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
            Command::PublishPost { operation_id, text } => self
                .publish_post(&operation_id, &text)
                .map(Reply::Published),
            Command::DrainEvents { limit } => {
                Ok(Reply::Events(self.drain_events(usize::from(limit))))
            }
            Command::Shutdown => Ok(Reply::Ack),
        }
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

        let (human, device) = self.signing_identity()?;
        let sequence = self.reserve_sequence()?;
        let object = build_post(human, device, text, now_ms(), sequence)
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

        let (human, device) = self.signing_identity()?;
        let sequence = self.reserve_sequence()?;
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
                let mut published =
                    self.journal.commit_and_complete(&mut self.store, &record)?;
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

    fn signing_identity(&self) -> Result<(&Did, &Controller), ServiceError> {
        let identity = self.identity.as_ref().ok_or_else(|| {
            ServiceError::new(ErrorCode::IdentityLocked, "identity is locked")
        })?;
        let human = self.human.as_ref().ok_or_else(|| {
            ServiceError::new(ErrorCode::RootMissing, "create a Mininet root first")
        })?;
        Ok((human, &identity.device))
    }

    fn reserve_sequence(&mut self) -> Result<u64, ServiceError> {
        if let Some(human) = self.human.as_ref() {
            let store_next = next_object_sequence(&self.store, Some(human))?;
            if store_next > self.next_sequence {
                self.next_sequence = self.journal.ensure_sequence_floor(store_next)?;
            }
        }
        let sequence = self.next_sequence;
        let next = sequence.checked_add(1).ok_or_else(|| {
            ServiceError::new(ErrorCode::Storage, "local object sequence space is exhausted")
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
                    let profile = resolve_profile(&self.store, &seed.author)
                        .map_err(storage_or_social)?;
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
                        PostKind::Plain | PostKind::Intake { .. } => None,
                    },
                    own: post.author == *human,
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

fn root_from_seeds(seeds: &SeedPair) -> Result<Controller, ServiceError> {
    Controller::incept_single_from_seeds(&seeds.current, &seeds.next)
        .map_err(|error| ServiceError::new(ErrorCode::Identity, error.to_string()))
}

fn identity_from_seeds(
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

fn next_object_sequence<B: Backend>(
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
        ServiceError::new(ErrorCode::Storage, "local object sequence space is exhausted")
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn validate_operation(operation_id: &str) -> Result<(), ServiceError> {
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
    Profile { display_name: String, bio: String },
    Post { text: String },
}

impl MutationInput {
    fn kind(&self) -> ObjectKind {
        match self {
            Self::Profile { .. } => ObjectKind::Profile,
            Self::Post { .. } => ObjectKind::Post,
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
    root: PathBuf,
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
            root,
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
            count = count.saturating_add(1);
            if count > MAX_PENDING_OPERATIONS {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "too many pending application mutations; refusing unbounded recovery",
                ));
            }
            let path = entry.path();
            let record = read_bounded(&path).and_then(|bytes| decode_pending(&bytes))?;
            let expected_path = self.pending_path(&record.operation_id);
            if path != expected_path {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "pending mutation filename does not match its operation id",
                ));
            }
            if self.read_receipt(&record.operation_id)?.is_some() {
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

    fn read_pending(
        &self,
        operation_id: &str,
    ) -> Result<Option<PendingMutation>, ServiceError> {
        read_optional(&self.pending_path(operation_id))?
            .map(|bytes| decode_pending(&bytes))
            .transpose()
    }

    fn read_receipt(
        &self,
        operation_id: &str,
    ) -> Result<Option<MutationReceipt>, ServiceError> {
        read_optional(&self.receipt_path(operation_id))?
            .map(|bytes| decode_receipt(&bytes))
            .transpose()
    }

    fn remove_pending(&self, operation_id: &str) -> Result<(), ServiceError> {
        let path = self.pending_path(operation_id);
        match fs::remove_file(&path) {
            Ok(()) => mini_durable::sync_parent(&path)
                .map_err(|error| ServiceError::new(ErrorCode::Io, error.to_string())),
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

fn commit_record(
    store: &mut Store<FsBackend>,
    record: &PendingMutation,
) -> Result<PublishedObject, ServiceError> {
    match &record.input {
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
        MutationInput::Profile { .. } => {
            if record.objects.len() != 2 {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "profile journal record has the wrong object count",
                ));
            }
            let profile = Object::from_bytes(&record.objects[0]).map_err(storage_error)?;
            let head = Object::from_bytes(&record.objects[1]).map_err(storage_error)?;
            if profile.object_type != ObjectType::PROFILE
                || head.object_type != ObjectType::HEAD
                || profile.author_human != head.author_human
                || profile.sequence != head.sequence
            {
                return Err(ServiceError::new(
                    ErrorCode::Storage,
                    "profile journal contains an invalid profile/head pair",
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
    if record.objects.len() > 2 {
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
    if count > 2 {
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

fn hex_encode(bytes: &[u8]) -> String {
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

    fn input(&mut self, input: &MutationInput) -> Result<(), ServiceError> {
        match input {
            MutationInput::Profile { display_name, bio } => {
                self.u8(0);
                self.string(display_name, MAX_PROFILE_NAME_BYTES)?;
                self.string(bio, MAX_PROFILE_BIO_BYTES)?;
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
        let end = self.offset.checked_add(count).ok_or_else(|| {
            ServiceError::new(ErrorCode::Storage, "journal length overflow")
        })?;
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
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .expect("four-byte journal slice");
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

    fn input(&mut self) -> Result<MutationInput, ServiceError> {
        match self.u8()? {
            0 => Ok(MutationInput::Profile {
                display_name: self.string(MAX_PROFILE_NAME_BYTES)?,
                bio: self.string(MAX_PROFILE_BIO_BYTES)?,
            }),
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
    fn production_lock_refuses_a_second_live_writer() {
        let root = temp_root("lock");
        let first = Core::open_inner(root.clone(), MemoryVault::empty(), true).unwrap();
        let error = Core::open_inner(root.clone(), MemoryVault::empty(), true).unwrap_err();
        assert_eq!(error.code, ErrorCode::Busy);
        drop(first);
        Core::open_inner(root.clone(), MemoryVault::empty(), true).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
