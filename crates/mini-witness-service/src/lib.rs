//! Durable witness observations and transition certifications (F-05/F-06).
//!
//! Every released receipt is stored verbatim with the verified KEL and exact
//! observation epoch. Recovery replays the same state machine and requires its
//! output to equal the stored receipt. The same signing key must be supplied
//! after restart; key migration is an explicit recovery operation.
//!
//! A lifetime OS lock excludes other processes. Writes stage in memory, flush
//! file contents, rename, and flush the parent directory before publication.
//! Any write error poisons the instance until reopen because a failed directory
//! barrier can leave a committed rename with an uncertain durability outcome.
//!
//! Legacy v1 records omitted certifications and original receipts, so they are
//! refused for explicit reconciliation rather than silently resumed. Records
//! have per-identity and aggregate replay limits. No automatic history pruning,
//! remote witness service, external rollback anchor, or hardware fault proof is
//! implied. See docs/audits/pr332-durability.md for recovery requirements.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use did_mini::{Did, IdentityError, Kel, WitnessId, WitnessIdentityState};
use did_mini::{WitnessJournal, WitnessObservation, WitnessReceipt};
use mini_crypto::{HashAlgorithm, SigningKey, VerifyingKey};

/// Maximum distinct identities, checked both on startup and admission.
pub const MAX_TRACKED_IDENTITIES: usize = 100_000;
/// Bound each eager read and the total replay allocation.
pub const MAX_RECORD_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_TOTAL_RECORD_BYTES: u64 = 512 * 1024 * 1024;
const RECORD_DOMAIN: &[u8] = b"mini-witness-service/record/v2";

#[derive(Debug)]
#[non_exhaustive]
pub enum WitnessServiceError {
    Io(std::io::Error),
    Identity(IdentityError),
    CorruptRecord,
    TooManyIdentities,
    TooLarge,
    MigrationRequired,
    RecoveryRequired,
    WrongWitnessKey,
}
impl core::fmt::Display for WitnessServiceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "witness service storage: {e}"),
            Self::Identity(e) => write!(f, "witness service: {e}"),
            Self::CorruptRecord => write!(f, "witness service: persisted record is corrupt"),
            Self::TooManyIdentities => write!(f, "witness service: identity capacity exceeded"),
            Self::TooLarge => write!(f, "witness service: replay capacity exceeded"),
            Self::MigrationRequired => write!(
                f,
                "witness service: legacy state requires reconciliation before signing"
            ),
            Self::RecoveryRequired => write!(
                f,
                "witness service: storage outcome uncertain; reopen required"
            ),
            Self::WrongWitnessKey => write!(
                f,
                "witness service: signer differs from retained receipt key"
            ),
        }
    }
}
impl std::error::Error for WitnessServiceError {}
impl From<std::io::Error> for WitnessServiceError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<IdentityError> for WitnessServiceError {
    fn from(e: IdentityError) -> Self {
        Self::Identity(e)
    }
}
pub type Result<T> = core::result::Result<T, WitnessServiceError>;

#[derive(Debug, Clone)]
struct RecordEvent {
    transition: bool,
    kel: Vec<u8>,
    receipt: WitnessReceipt,
}

#[derive(Debug)]
pub struct PersistentWitnessJournal {
    root: PathBuf,
    _lock: File,
    journal: WitnessJournal,
    records: HashMap<Did, Vec<RecordEvent>>,
    total_bytes: u64,
    max_identities: usize,
    witness_id: WitnessId,
    witness_key: VerifyingKey,
    poisoned: bool,
}
// Release the owner-held lock explicitly before closing its handle. A process
// spawned concurrently by another thread can briefly inherit a duplicate until
// exec; closing our handle alone would let that duplicate retain the lock.
impl Drop for PersistentWitnessJournal {
    fn drop(&mut self) {
        let _ = self._lock.unlock();
    }
}

impl PersistentWitnessJournal {
    pub fn open(
        root: impl Into<PathBuf>,
        witness_id: WitnessId,
        witness_key: &SigningKey,
    ) -> Result<Self> {
        Self::open_with_capacity(root, witness_id, witness_key, MAX_TRACKED_IDENTITIES)
    }
    pub fn open_with_capacity(
        root: impl Into<PathBuf>,
        witness_id: WitnessId,
        witness_key: &SigningKey,
        max_identities: usize,
    ) -> Result<Self> {
        let root = root.into();
        mini_durable::create_dir_all(&root)?;
        let lock = mini_durable::try_lock_exclusive(&root.join("journal.lock"))?;
        let mut journal = WitnessJournal::new();
        let mut records = HashMap::new();
        let mut total_bytes = 0u64;
        for entry in fs::read_dir(&root)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("state") {
                continue;
            }
            if records.len() >= max_identities {
                return Err(WitnessServiceError::TooManyIdentities);
            }
            let metadata = fs::symlink_metadata(&path)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(WitnessServiceError::CorruptRecord);
            }
            total_bytes = total_bytes
                .checked_add(metadata.len())
                .ok_or(WitnessServiceError::TooLarge)?;
            if metadata.len() > MAX_RECORD_BYTES as u64 || total_bytes > MAX_TOTAL_RECORD_BYTES {
                return Err(WitnessServiceError::TooLarge);
            }
            let mut bytes = Vec::new();
            File::open(&path)?
                .take(MAX_RECORD_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > MAX_RECORD_BYTES {
                return Err(WitnessServiceError::TooLarge);
            }
            let events = decode_record(&bytes)?;
            let mut identity = None;
            for event in &events {
                let kel = Kel::from_bytes(&event.kel)?;
                let did = kel.did();
                if identity.as_ref().is_some_and(|previous| previous != &did) {
                    return Err(WitnessServiceError::CorruptRecord);
                }
                identity = Some(did);
                if event.receipt.statement.witness_id != witness_id
                    || event.receipt.verify(&witness_key.verifying_key()).is_err()
                {
                    return Err(WitnessServiceError::WrongWitnessKey);
                }
                let epoch = event.receipt.statement.observed_epoch;
                let replayed = if event.transition {
                    journal.certify_policy_transition(
                        &kel,
                        witness_id.clone(),
                        witness_key,
                        epoch,
                    )?
                } else {
                    let WitnessObservation::Accepted(receipt) =
                        journal.observe_declared(&kel, witness_id.clone(), witness_key, epoch)?
                    else {
                        return Err(WitnessServiceError::CorruptRecord);
                    };
                    receipt
                };
                if replayed != event.receipt {
                    return Err(WitnessServiceError::CorruptRecord);
                }
            }
            let identity = identity.ok_or(WitnessServiceError::CorruptRecord)?;
            if record_path(&root, &identity) != path || records.insert(identity, events).is_some() {
                return Err(WitnessServiceError::CorruptRecord);
            }
        }
        Ok(Self {
            root,
            _lock: lock,
            journal,
            records,
            total_bytes,
            max_identities,
            witness_id,
            witness_key: witness_key.verifying_key(),
            poisoned: false,
        })
    }

    fn check_ready(&self, witness_id: &WitnessId, key: &SigningKey) -> Result<()> {
        if self.poisoned {
            return Err(WitnessServiceError::RecoveryRequired);
        }
        if witness_id != &self.witness_id || key.verifying_key() != self.witness_key {
            return Err(WitnessServiceError::WrongWitnessKey);
        }
        Ok(())
    }

    pub fn observe_declared(
        &mut self,
        kel: &Kel,
        witness_id: WitnessId,
        witness_key: &SigningKey,
        observed_epoch: u64,
    ) -> Result<WitnessObservation> {
        self.check_ready(&witness_id, witness_key)?;
        let identity = kel.did();
        if !self.records.contains_key(&identity) && self.records.len() >= self.max_identities {
            return Err(WitnessServiceError::TooManyIdentities);
        }
        let mut staged = self.journal.clone();
        let outcome = staged.observe_declared(kel, witness_id, witness_key, observed_epoch)?;
        if let WitnessObservation::Accepted(receipt) = &outcome {
            self.persist(
                &identity,
                RecordEvent {
                    transition: false,
                    kel: kel.to_bytes(),
                    receipt: receipt.clone(),
                },
            )?;
        }
        self.journal = staged;
        Ok(outcome)
    }

    /// Commit certification before exposing the signature. Identical retries
    /// return the original receipt including its original observation epoch.
    pub fn certify_policy_transition(
        &mut self,
        kel: &Kel,
        witness_id: WitnessId,
        witness_key: &SigningKey,
        observed_epoch: u64,
    ) -> Result<WitnessReceipt> {
        self.check_ready(&witness_id, witness_key)?;
        let identity = kel.did();
        let bytes = kel.to_bytes();
        if let Some(previous) = self.records.get(&identity).and_then(|events| {
            events
                .iter()
                .find(|event| event.transition && event.kel == bytes)
        }) {
            return Ok(previous.receipt.clone());
        }
        let mut staged = self.journal.clone();
        let receipt =
            staged.certify_policy_transition(kel, witness_id, witness_key, observed_epoch)?;
        self.persist(
            &identity,
            RecordEvent {
                transition: true,
                kel: bytes,
                receipt: receipt.clone(),
            },
        )?;
        self.journal = staged;
        Ok(receipt)
    }

    pub fn state_for(&self, identity: &Did) -> Option<&WitnessIdentityState> {
        if self.poisoned {
            None
        } else {
            self.journal.state_for(identity)
        }
    }
    pub fn tracked_identity_count(&self) -> usize {
        self.records.len()
    }

    fn persist(&mut self, identity: &Did, event: RecordEvent) -> Result<()> {
        let mut events = self.records.get(identity).cloned().unwrap_or_default();
        let previous_bytes = if events.is_empty() {
            0
        } else {
            encode_record(&events).len() as u64
        };
        events.push(event);
        let bytes = encode_record(&events);
        let total = self.total_bytes - previous_bytes + bytes.len() as u64;
        if bytes.len() > MAX_RECORD_BYTES || total > MAX_TOTAL_RECORD_BYTES {
            return Err(WitnessServiceError::TooLarge);
        }
        if let Err(error) = mini_durable::atomic_replace(&record_path(&self.root, identity), &bytes)
        {
            self.poisoned = true;
            return Err(error.into());
        }
        self.records.insert(identity.clone(), events);
        self.total_bytes = total;
        Ok(())
    }
}

fn record_path(root: &Path, identity: &Did) -> PathBuf {
    let mut input = b"mini-witness-service/identity/v2\0".to_vec();
    input.extend_from_slice(identity.as_str().as_bytes());
    let hash = HashAlgorithm::Blake3.digest(&input);
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    root.join(format!("{hex}.state"))
}
fn encode_record(events: &[RecordEvent]) -> Vec<u8> {
    let mut bytes = RECORD_DOMAIN.to_vec();
    bytes.extend_from_slice(&(events.len() as u32).to_be_bytes());
    for event in events {
        bytes.push(u8::from(event.transition));
        for field in [&event.kel, &event.receipt.encode()] {
            bytes.extend_from_slice(&(field.len() as u32).to_be_bytes());
            bytes.extend_from_slice(field);
        }
    }
    let checksum = HashAlgorithm::Blake3.digest(&bytes);
    bytes.extend_from_slice(&checksum);
    bytes
}
fn take<'a>(bytes: &mut &'a [u8], len: usize) -> Result<&'a [u8]> {
    if bytes.len() < len {
        return Err(WitnessServiceError::CorruptRecord);
    }
    let (head, tail) = bytes.split_at(len);
    *bytes = tail;
    Ok(head)
}
fn number(bytes: &mut &[u8]) -> Result<usize> {
    Ok(u32::from_be_bytes(
        take(bytes, 4)?
            .try_into()
            .map_err(|_| WitnessServiceError::CorruptRecord)?,
    ) as usize)
}
fn decode_record(bytes: &[u8]) -> Result<Vec<RecordEvent>> {
    if bytes.starts_with(b"mini-witness-service/record/v1") {
        return Err(WitnessServiceError::MigrationRequired);
    }
    if !bytes.starts_with(RECORD_DOMAIN) || bytes.len() < RECORD_DOMAIN.len() + 4 + 32 {
        return Err(WitnessServiceError::CorruptRecord);
    }
    let (data, checksum) = bytes.split_at(bytes.len() - 32);
    if HashAlgorithm::Blake3.digest(data).as_slice() != checksum {
        return Err(WitnessServiceError::CorruptRecord);
    }
    let mut body = &data[RECORD_DOMAIN.len()..];
    let count = number(&mut body)?;
    if count == 0 || count > body.len() / 9 {
        return Err(WitnessServiceError::CorruptRecord);
    }
    let mut events = Vec::new();
    for _ in 0..count {
        let transition = match take(&mut body, 1)?[0] {
            0 => false,
            1 => true,
            _ => return Err(WitnessServiceError::CorruptRecord),
        };
        let size = number(&mut body)?;
        let kel = take(&mut body, size)?.to_vec();
        let size = number(&mut body)?;
        let receipt = WitnessReceipt::decode(take(&mut body, size)?)?;
        events.push(RecordEvent {
            transition,
            kel,
            receipt,
        });
    }
    if !body.is_empty() {
        return Err(WitnessServiceError::CorruptRecord);
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;

    fn temp_root(tag: &str) -> PathBuf {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "mini-witness-service-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        root
    }

    fn a_witness() -> (WitnessId, SigningKey) {
        let root = Controller::incept_single().unwrap();
        (WitnessId(root.did()), SigningKey::generate().unwrap())
    }

    fn appointed_identity() -> Controller {
        let mut owner = Controller::incept_single().unwrap();
        let (witness_id, _) = a_witness();
        owner.appoint_witnesses(vec![witness_id.0], 1).unwrap();
        owner
    }

    #[test]
    fn an_accepted_receipt_survives_a_restart_byte_for_byte() {
        let root = temp_root("restart");
        let (witness_id, witness_key) = a_witness();
        let owner = appointed_identity();
        // Re-appoint the same witness the journal will actually authenticate as.
        let mut owner = owner;
        owner
            .appoint_witnesses(vec![witness_id.0.clone()], 1)
            .unwrap();

        let before = {
            let mut store =
                PersistentWitnessJournal::open(&root, witness_id.clone(), &witness_key).unwrap();
            let outcome = store
                .observe_declared(&owner.kel(), witness_id.clone(), &witness_key, 100)
                .unwrap();
            let WitnessObservation::Accepted(receipt) = outcome else {
                panic!("expected Accepted");
            };
            receipt
        };
        // Drop `store` here -- the equivalent of the process exiting.

        let after = {
            let store =
                PersistentWitnessJournal::open(&root, witness_id.clone(), &witness_key).unwrap();
            store
                .state_for(&owner.did())
                .expect("state survives a restart")
                .issued_receipt()
                .clone()
        };

        assert_eq!(before, after, "the exact same receipt, not a re-signed one");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_second_open_answers_a_resubmission_with_already_accepted() {
        let root = temp_root("already-accepted");
        let (witness_id, witness_key) = a_witness();
        let mut owner = appointed_identity();
        owner
            .appoint_witnesses(vec![witness_id.0.clone()], 1)
            .unwrap();

        {
            let mut store =
                PersistentWitnessJournal::open(&root, witness_id.clone(), &witness_key).unwrap();
            store
                .observe_declared(&owner.kel(), witness_id.clone(), &witness_key, 100)
                .unwrap();
        }

        let mut reopened =
            PersistentWitnessJournal::open(&root, witness_id.clone(), &witness_key).unwrap();
        let outcome = reopened
            .observe_declared(&owner.kel(), witness_id, &witness_key, 200)
            .unwrap();
        assert!(matches!(outcome, WitnessObservation::AlreadyAccepted(_)));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_stray_tmp_file_from_an_interrupted_write_is_ignored_on_open() {
        let root = temp_root("stray-tmp");
        let (witness_id, witness_key) = a_witness();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("deadbeef.tmp"), b"not a real record").unwrap();

        // Must open cleanly: the stray .tmp is never mistaken for a record.
        let store = PersistentWitnessJournal::open(&root, witness_id, &witness_key).unwrap();
        assert_eq!(store.tracked_identity_count(), 0);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_corrupt_state_file_is_rejected_not_silently_skipped() {
        let root = temp_root("corrupt");
        let (witness_id, witness_key) = a_witness();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("0000.state"), b"garbage, not a valid record").unwrap();

        let result = PersistentWitnessJournal::open(&root, witness_id, &witness_key);
        assert!(
            result.is_err(),
            "corrupt durable state must not be silently ignored"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_full_journal_refuses_a_new_identity_but_not_an_existing_one() {
        let root = temp_root("capacity");
        let (witness_id, witness_key) = a_witness();
        let mut first = appointed_identity();
        first
            .appoint_witnesses(vec![witness_id.0.clone()], 1)
            .unwrap();
        let mut second = appointed_identity();
        second
            .appoint_witnesses(vec![witness_id.0.clone()], 1)
            .unwrap();

        let mut store = PersistentWitnessJournal::open_with_capacity(
            &root,
            witness_id.clone(),
            &witness_key,
            1,
        )
        .unwrap();
        store
            .observe_declared(&first.kel(), witness_id.clone(), &witness_key, 1)
            .unwrap();

        // A second call for the *same* (already-tracked) identity must
        // still be allowed even though the journal is "full."
        let repeat = store
            .observe_declared(&first.kel(), witness_id.clone(), &witness_key, 2)
            .unwrap();
        assert!(matches!(repeat, WitnessObservation::AlreadyAccepted(_)));

        // A genuinely new identity is refused once at capacity.
        let refused = store.observe_declared(&second.kel(), witness_id, &witness_key, 3);
        assert!(matches!(
            refused,
            Err(WitnessServiceError::TooManyIdentities)
        ));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_identity_without_a_declared_policy_is_rejected_and_nothing_is_persisted() {
        let root = temp_root("no-policy");
        let (witness_id, witness_key) = a_witness();
        let owner = Controller::incept_single().unwrap(); // never appoints witnesses

        let mut store =
            PersistentWitnessJournal::open(&root, witness_id.clone(), &witness_key).unwrap();
        let result = store.observe_declared(&owner.kel(), witness_id, &witness_key, 1);
        assert!(result.is_err());
        assert_eq!(store.tracked_identity_count(), 0);
        assert_eq!(
            std::fs::read_dir(&root)
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|e| e == "state"))
                .count(),
            0
        );
        std::fs::remove_dir_all(&root).ok();
    }

    // -----------------------------------------------------------------
    // F-05: a persistence failure must not leave the in-memory journal
    // believing an unwritten observation was accepted
    // -----------------------------------------------------------------

    #[test]
    fn a_persist_failure_never_leaves_the_in_memory_journal_ahead_of_disk() {
        // Simulated the way F-05's own concrete example describes ("make
        // the state directory unwritable") without relying on POSIX
        // permission bits, which root (this test's runtime user in some
        // environments) simply ignores: removing the directory `persist`
        // expects to write into forces the same real I/O error
        // (`fs::write` into a nonexistent directory) regardless of
        // privilege level, and is itself a realistic failure (the mount
        // went away, a parallel cleanup raced it, disk pressure evicted
        // it) that any real deployment could hit.
        let root = temp_root("persist-failure");
        let (witness_id, witness_key) = a_witness();
        let mut owner = appointed_identity();
        owner
            .appoint_witnesses(vec![witness_id.0.clone()], 1)
            .unwrap();

        let mut store =
            PersistentWitnessJournal::open(&root, witness_id.clone(), &witness_key).unwrap();

        std::fs::remove_dir_all(&root).unwrap();

        let first_attempt =
            store.observe_declared(&owner.kel(), witness_id.clone(), &witness_key, 100);
        assert!(
            first_attempt.is_err(),
            "persistence must fail while the directory is gone"
        );
        // The in-memory journal must not have advanced: nothing was
        // durably recorded, so state_for must still see nothing.
        assert!(
            store.state_for(&owner.did()).is_none(),
            "a failed persist must not leave the identity looking accepted in memory"
        );
        assert_eq!(store.tracked_identity_count(), 0);

        // Recreate the directory and retry -- must behave as a fresh,
        // genuine attempt (Accepted), not AlreadyAccepted from a phantom
        // in-memory acceptance, and must actually land on disk this time.
        std::fs::create_dir_all(&root).unwrap();
        assert!(matches!(
            store.observe_declared(&owner.kel(), witness_id.clone(), &witness_key, 101),
            Err(WitnessServiceError::RecoveryRequired)
        ));
        drop(store);
        let mut store =
            PersistentWitnessJournal::open(&root, witness_id.clone(), &witness_key).unwrap();
        let retry = store
            .observe_declared(&owner.kel(), witness_id.clone(), &witness_key, 101)
            .unwrap();
        assert!(
            matches!(retry, WitnessObservation::Accepted(_)),
            "the retry must be a genuine acceptance, not a phantom AlreadyAccepted"
        );
        assert_eq!(store.tracked_identity_count(), 1);

        // Confirm the record actually reached disk by reopening.
        drop(store);
        let reopened = PersistentWitnessJournal::open(&root, witness_id, &witness_key).unwrap();
        assert!(reopened.state_for(&owner.did()).is_some());

        std::fs::remove_dir_all(&root).ok();
    }

    fn forks(witness: &WitnessId) -> (Controller, Controller, Controller) {
        let mut owner = Controller::incept_single().unwrap();
        owner.appoint_witnesses(vec![witness.0.clone()], 1).unwrap();
        let (current, next) = owner.export_current_and_next_keys_for_storage();
        let mut a = Controller::restore(&owner.kel(), current, next).unwrap();
        let (current, next) = owner.export_current_and_next_keys_for_storage();
        let mut b = Controller::restore(&owner.kel(), current, next).unwrap();
        a.appoint_witnesses(vec![witness.0.clone(), a_witness().0 .0], 1)
            .unwrap();
        b.appoint_witnesses(vec![witness.0.clone(), a_witness().0 .0], 1)
            .unwrap();
        (owner, a, b)
    }

    #[test]
    fn transition_commitments_survive_restart_and_refuse_both_conflict_orders() {
        for reverse in [false, true] {
            let root = temp_root("transition-restart");
            let (witness, key) = a_witness();
            let (owner, mut a, mut b) = forks(&witness);
            if reverse {
                std::mem::swap(&mut a, &mut b);
            }
            let receipt = {
                let mut store =
                    PersistentWitnessJournal::open(&root, witness.clone(), &key).unwrap();
                store
                    .observe_declared(&owner.kel(), witness.clone(), &key, 100)
                    .unwrap();
                store
                    .certify_policy_transition(&a.kel(), witness.clone(), &key, 101)
                    .unwrap()
            };
            let mut store = PersistentWitnessJournal::open(&root, witness.clone(), &key).unwrap();
            assert_eq!(
                store
                    .certify_policy_transition(&a.kel(), witness.clone(), &key, 999)
                    .unwrap(),
                receipt
            );
            assert!(matches!(
                store.certify_policy_transition(&b.kel(), witness.clone(), &key, 102),
                Err(WitnessServiceError::Identity(
                    IdentityError::ConflictingPolicyTransitionCertification
                ))
            ));
            assert!(matches!(
                store.observe_declared(&b.kel(), witness.clone(), &key, 103),
                Err(WitnessServiceError::Identity(
                    IdentityError::ConflictingPolicyTransitionCertification
                ))
            ));
            // Accepting the certified head does not erase its certification.
            store
                .observe_declared(&a.kel(), witness.clone(), &key, 104)
                .unwrap();
            drop(store);
            let mut store = PersistentWitnessJournal::open(&root, witness.clone(), &key).unwrap();
            assert_eq!(
                store
                    .certify_policy_transition(&a.kel(), witness.clone(), &key, 1000)
                    .unwrap(),
                receipt
            );
            drop(store);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn uncertain_certification_does_not_release_a_signature_or_allow_retry() {
        let root = temp_root("certification-failure");
        let (witness, key) = a_witness();
        let (owner, a, b) = forks(&witness);
        let mut store = PersistentWitnessJournal::open(&root, witness.clone(), &key).unwrap();
        store
            .observe_declared(&owner.kel(), witness.clone(), &key, 1)
            .unwrap();
        // Make rename fail without removing previously committed evidence.
        let destination = record_path(&root, &owner.did());
        let saved = fs::read(&destination).unwrap();
        fs::remove_file(&destination).unwrap();
        fs::create_dir(&destination).unwrap();
        assert!(store
            .certify_policy_transition(&a.kel(), witness.clone(), &key, 2)
            .is_err());
        assert!(matches!(
            store.certify_policy_transition(&b.kel(), witness.clone(), &key, 3),
            Err(WitnessServiceError::RecoveryRequired)
        ));
        fs::remove_dir(&destination).unwrap();
        fs::write(&destination, saved).unwrap();
        drop(store);
        let mut store = PersistentWitnessJournal::open(&root, witness.clone(), &key).unwrap();
        store
            .certify_policy_transition(&b.kel(), witness, &key, 4)
            .unwrap();
    }

    #[test]
    fn a_rotated_key_never_resigns_old_receipts() {
        let root = temp_root("wrong-key");
        let (witness, key) = a_witness();
        let (owner, _, _) = forks(&witness);
        let mut store = PersistentWitnessJournal::open(&root, witness.clone(), &key).unwrap();
        store
            .observe_declared(&owner.kel(), witness.clone(), &key, 1)
            .unwrap();
        drop(store);
        assert!(matches!(
            PersistentWitnessJournal::open(&root, witness, &SigningKey::generate().unwrap()),
            Err(WitnessServiceError::WrongWitnessKey)
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_enforces_record_size_identity_capacity_and_legacy_migration() {
        let root = temp_root("load-limits");
        let (witness, key) = a_witness();
        let (owner, _, _) = forks(&witness);
        let mut store = PersistentWitnessJournal::open(&root, witness.clone(), &key).unwrap();
        store
            .observe_declared(&owner.kel(), witness.clone(), &key, 1)
            .unwrap();
        drop(store);
        assert!(matches!(
            PersistentWitnessJournal::open_with_capacity(&root, witness.clone(), &key, 0),
            Err(WitnessServiceError::TooManyIdentities)
        ));
        let path = record_path(&root, &owner.did());
        File::create(&path)
            .unwrap()
            .set_len(MAX_RECORD_BYTES as u64 + 1)
            .unwrap();
        assert!(matches!(
            PersistentWitnessJournal::open(&root, witness.clone(), &key),
            Err(WitnessServiceError::TooLarge)
        ));
        fs::write(&path, b"mini-witness-service/record/v1").unwrap();
        assert!(matches!(
            PersistentWitnessJournal::open(&root, witness, &key),
            Err(WitnessServiceError::MigrationRequired)
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dropping_the_journal_releases_a_duplicated_lock_handle() {
        let root = temp_root("duplicated-lock");
        let (witness, key) = a_witness();
        let store = PersistentWitnessJournal::open(&root, witness.clone(), &key).unwrap();
        // Deterministically model the descriptor inherited between fork and
        // exec by an unrelated, concurrently spawned test process.
        let inherited = store._lock.try_clone().unwrap();
        drop(store);
        let reopened = PersistentWitnessJournal::open(&root, witness, &key).unwrap();
        drop(inherited);
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn another_process_cannot_open_a_live_journal() {
        const ENV: &str = "MINI_WITNESS_LOCK_CHILD";
        if let Some(root) = std::env::var_os(ENV) {
            let (witness, key) = a_witness();
            assert!(PersistentWitnessJournal::open(PathBuf::from(root), witness, &key).is_err());
            return;
        }
        let root = temp_root("process-lock");
        let (witness, key) = a_witness();
        let store = PersistentWitnessJournal::open(&root, witness, &key).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tests::another_process_cannot_open_a_live_journal",
            ])
            .env(ENV, &root)
            .status()
            .unwrap();
        assert!(status.success());
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
}
