//! Gate #72 remediation: production FROST threshold **signing** (as
//! opposed to [`crate::session`]'s DKG ceremony) over `frost_ristretto255`'s
//! own round1/round2/aggregate API -- not a second hand-rolled
//! implementation.
//!
//! An anonymous external audit report ("Mininet External Privacy & Value
//! Layer Audit Report", Gate #72, 2026-09-11) found `mini_treasury`'s
//! `frost_sign` module re-derives the entire two-round FROST signing
//! protocol (binding factors, Lagrange interpolation, the Schnorr
//! challenge) from raw `curve25519-dalek` scalar/point arithmetic, mirroring
//! the same "bespoke re-implementation of an already-solved, already-
//! audited problem" pattern the Gate #93 audit found in that crate's DKG
//! (`frost_dkg`, remediated by D-0507/D-0508 and this crate's [`session`]
//! module). The fix here is the same shape: this module wraps
//! `frost_ristretto255::round1`/`round2`/`aggregate` directly rather than
//! hand-deriving the binding-factor/challenge/Lagrange math a second time.
//!
//! ## What this module adds on top of `frost_ristretto255`
//!
//! The library itself is stateless per call: `round1::commit` takes a
//! signing share and an RNG and returns nonces/commitments; `round2::sign`
//! takes a signing package and those nonces and returns a share; it has no
//! opinion on what happens between those two calls. The catastrophic FROST
//! failure mode -- reusing a round-1 nonce pair across two different
//! round-2 signs, which leaks the signer's secret share the same way nonce
//! reuse leaks an ECDSA/Schnorr key -- is exactly the gap
//! `mini_treasury::frost_sign::DurableFrostSigner` closed for the old
//! implementation with a crash-safe on-disk journal. [`DurableCustodySigner`]
//! is that same protection carried over to `frost_ristretto255`'s types:
//! every reserved commitment is durably recorded *before* it is returned to
//! the caller, and a signer instance that reopens a journal after a crash
//! treats every not-yet-burned reservation as unusable (its in-memory
//! secret nonce is gone with the old process; the only sound thing to do is
//! refuse to complete it, never resurrect or reuse it).
//!
//! Only the public round-1 *commitment* (`D_i`, `E_i`) is ever written to
//! disk -- never the secret nonce scalars themselves, which live only in
//! [`DurableSigningNonces`] for the lifetime of the in-memory value
//! returned by [`DurableCustodySigner::commit`]. A crash between `commit`
//! and `sign` loses that secret with the process; the journal's job is
//! only to make sure the *commitment* can never be silently reused by a
//! later call once that happens.

use std::collections::BTreeMap;
use std::path::PathBuf;

use frost_ristretto255::keys::{KeyPackage, PublicKeyPackage};
use frost_ristretto255::round1::{self, SigningCommitments, SigningNonces};
use frost_ristretto255::round2::{self, SignatureShare};
use frost_ristretto255::{Identifier, Signature, SigningPackage};
use rand_core::{CryptoRng, RngCore};

use crate::error::{CustodyError, Result};
use crate::wire::push_bytes;

fn frost_err(e: frost_ristretto255::Error) -> CustodyError {
    CustodyError::Frost(e.to_string())
}

fn journal_error(msg: impl std::fmt::Display) -> CustodyError {
    CustodyError::SigningJournal(msg.to_string())
}

/// Round 1: generate this signer's fresh, single-use nonce pair and its
/// public commitment. Prefer [`DurableCustodySigner::commit`] for any
/// caller whose process could crash between round 1 and round 2 --  a bare
/// [`SigningNonces`] value has no crash-safety of its own beyond the
/// library's own zeroize-on-drop.
pub fn round1_commit<R>(key: &KeyPackage, rng: &mut R) -> (SigningNonces, SigningCommitments)
where
    R: RngCore + CryptoRng,
{
    round1::commit(key.signing_share(), rng)
}

/// Bundle the coordinator-collected round-1 commitments with the message
/// into the package every round-2 signer signs over.
pub fn build_signing_package(
    commitments: BTreeMap<Identifier, SigningCommitments>,
    message: &[u8],
) -> SigningPackage {
    SigningPackage::new(commitments, message)
}

/// Round 2: this signer's share of the final signature. Consumes `nonces`
/// by value so the caller's move checker -- not just convention -- refuses
/// a second call over the same nonce pair.
pub fn round2_sign(
    package: &SigningPackage,
    nonces: SigningNonces,
    key: &KeyPackage,
) -> Result<SignatureShare> {
    round2::sign(package, &nonces, key).map_err(frost_err)
}

/// Combine every participating signer's round-2 share into the final,
/// ordinary Schnorr signature. Internally re-verifies each share before
/// aggregating, per `frost_ristretto255::aggregate`'s own contract.
pub fn aggregate_signature(
    package: &SigningPackage,
    shares: &BTreeMap<Identifier, SignatureShare>,
    pubkeys: &PublicKeyPackage,
) -> Result<Signature> {
    frost_ristretto255::aggregate(package, shares, pubkeys).map_err(frost_err)
}

const SIGNER_DOMAIN: &[u8] = b"mininet/custody/durable-signer/v1";
const NONCE_DOMAIN: &[u8] = b"mininet/custody/nonce-record/v1";
/// Retention is conservative: never delete commitments to regain capacity.
/// Exhaustion requires an explicitly reviewed signer/journal migration --
/// the same policy `mini_treasury::frost_sign`'s equivalent constant uses.
pub const MAX_DURABLE_NONCE_RECORDS: usize = 100_000;

#[derive(Debug)]
struct NonceRecord {
    signer: [u8; 32],
    instance: [u8; 32],
    commitment: Vec<u8>,
    burned: bool,
    transcript: [u8; 32],
}

impl NonceRecord {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = NONCE_DOMAIN.to_vec();
        bytes.extend_from_slice(&self.signer);
        bytes.extend_from_slice(&self.instance);
        push_bytes(&mut bytes, &self.commitment);
        bytes.push(u8::from(self.burned));
        bytes.extend_from_slice(&self.transcript);
        let checksum = blake3::hash(&bytes);
        bytes.extend_from_slice(checksum.as_bytes());
        bytes
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < NONCE_DOMAIN.len() + 32 || !bytes.starts_with(NONCE_DOMAIN) {
            return Err(journal_error("invalid nonce journal record"));
        }
        let (content, checksum) = bytes.split_at(bytes.len() - 32);
        if blake3::hash(content).as_bytes() != checksum {
            return Err(journal_error("nonce journal checksum mismatch"));
        }
        let mut cursor = &content[NONCE_DOMAIN.len()..];
        let take = |cursor: &mut &[u8], n: usize| -> Result<Vec<u8>> {
            if cursor.len() < n {
                return Err(journal_error("truncated nonce journal record"));
            }
            let (field, rest) = cursor.split_at(n);
            *cursor = rest;
            Ok(field.to_vec())
        };
        let signer: [u8; 32] = take(&mut cursor, 32)?.try_into().unwrap();
        let instance: [u8; 32] = take(&mut cursor, 32)?.try_into().unwrap();
        if cursor.len() < 2 {
            return Err(journal_error("truncated nonce commitment length"));
        }
        let commitment_len = u16::from_be_bytes([cursor[0], cursor[1]]) as usize;
        cursor = &cursor[2..];
        let commitment = take(&mut cursor, commitment_len)?;
        if cursor.is_empty() {
            return Err(journal_error("truncated nonce burned flag"));
        }
        let burned = match cursor[0] {
            0 => false,
            1 => true,
            _ => return Err(journal_error("invalid nonce record status")),
        };
        cursor = &cursor[1..];
        let transcript: [u8; 32] = take(&mut cursor, 32)?.try_into().unwrap();
        if !cursor.is_empty() {
            return Err(journal_error("trailing bytes in nonce journal record"));
        }
        if !burned && transcript != [0; 32] {
            return Err(journal_error("invalid nonce record fields"));
        }
        if SigningCommitments::deserialize(&commitment).is_err() {
            return Err(journal_error("malformed persisted nonce commitment"));
        }
        Ok(Self {
            signer,
            instance,
            commitment,
            burned,
            transcript,
        })
    }

    fn name(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"mininet/custody/nonce-name/v1");
        hasher.update(&self.signer);
        hasher.update(&self.commitment);
        format!("{}.nonce", hasher.finalize().to_hex())
    }
}

fn read_journal_file(path: &std::path::Path, limit: usize) -> Result<Vec<u8>> {
    use std::io::Read;
    let metadata = std::fs::symlink_metadata(path).map_err(journal_error)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > limit as u64 {
        return Err(journal_error("journal must be a bounded regular file"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(journal_error)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(journal_error)?;
    if bytes.len() > limit {
        return Err(journal_error("journal exceeds read bound"));
    }
    Ok(bytes)
}

/// A crash-safe FROST signer for one [`KeyPackage`], backed by an
/// exclusively-locked on-disk directory. See the module docs for what
/// durability property this actually provides (reservation-reuse
/// detection across a crash, not secret-nonce recovery).
pub struct DurableCustodySigner {
    directory: PathBuf,
    key: KeyPackage,
    binding: [u8; 32],
    instance: [u8; 32],
    record_count: usize,
    /// Reservations that were consumed by a real `sign` (non-zero transcript),
    /// as opposed to burned unused on reopen.
    signed_count: usize,
    poisoned: bool,
    _lock: std::fs::File,
}

impl core::fmt::Debug for DurableCustodySigner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DurableCustodySigner")
            .field("identifier", self.key.identifier())
            .field("record_count", &self.record_count)
            .field("poisoned", &self.poisoned)
            .finish_non_exhaustive()
    }
}

/// Private nonce material reserved by one durable signer. Consuming it (by
/// value) into [`DurableCustodySigner::sign`] burns the on-disk reservation
/// before the secret nonces are ever touched, so a crash after the burn but
/// before the response is released can never be retried with the same
/// nonces.
#[derive(Debug)]
pub struct DurableSigningNonces {
    nonces: SigningNonces,
    record: NonceRecord,
}

impl DurableCustodySigner {
    /// Open (creating if absent) a durable signer journal in `directory`
    /// for `key`. Takes an exclusive lock for the process lifetime -- a
    /// second concurrent `open` on the same directory fails rather than
    /// silently sharing state.
    pub fn open(directory: impl Into<PathBuf>, key: KeyPackage) -> Result<Self> {
        let directory = directory.into();
        mini_durable::create_dir_all(&directory).map_err(journal_error)?;
        let lock = mini_durable::try_lock_exclusive(&directory.join("signer.lock"))
            .map_err(journal_error)?;

        let verifying_share = key
            .verifying_share()
            .serialize()
            .map_err(|_| journal_error("key package has an invalid verifying share"))?;
        let verifying_key = key
            .verifying_key()
            .serialize()
            .map_err(|_| journal_error("key package has an invalid verifying key"))?;
        let mut manifest = SIGNER_DOMAIN.to_vec();
        push_bytes(&mut manifest, &key.identifier().serialize());
        push_bytes(&mut manifest, &verifying_share);
        push_bytes(&mut manifest, &verifying_key);
        let binding = *blake3::hash(&manifest).as_bytes();

        let manifest_path = directory.join("signer.binding");
        let mut record_count = 0usize;
        let mut records = Vec::new();
        let mut directory_entries = 0usize;
        for entry in std::fs::read_dir(&directory).map_err(journal_error)? {
            let entry = entry.map_err(journal_error)?;
            directory_entries += 1;
            if directory_entries > MAX_DURABLE_NONCE_RECORDS + 2 {
                return Err(journal_error("signer journal capacity exceeded"));
            }
            let path = entry.path();
            if path == manifest_path || path == directory.join("signer.lock") {
                continue;
            }
            if path.extension().is_some_and(|extension| extension == "tmp") {
                continue;
            }
            if path
                .extension()
                .is_none_or(|extension| extension != "nonce")
            {
                return Err(journal_error("unexpected signer journal entry"));
            }
            let record = NonceRecord::decode(&read_journal_file(&path, 4096)?)?;
            if record.signer != binding
                || path.file_name().and_then(|name| name.to_str()) != Some(record.name().as_str())
            {
                return Err(journal_error(
                    "nonce record does not bind this signer/commitment",
                ));
            }
            records.push(record);
            record_count += 1;
        }
        match read_journal_file(&manifest_path, manifest.len()) {
            Ok(bytes) if bytes == manifest => {}
            Ok(_) => {
                return Err(journal_error(
                    "signer identity/share does not match journal",
                ))
            }
            Err(_) if !manifest_path.try_exists().map_err(journal_error)? => {
                if directory_entries != 1 {
                    return Err(journal_error("missing binding in nonempty signer journal"));
                }
                mini_durable::atomic_replace(&manifest_path, &manifest).map_err(journal_error)?;
            }
            Err(error) => return Err(error),
        }
        // Re-establish barriers in case a previous manifest write returned
        // an uncertain error. Runs before any new commitment is released.
        mini_durable::atomic_replace(&manifest_path, &manifest).map_err(journal_error)?;
        let records_signed = records
            .iter()
            .filter(|record| record.transcript != [0; 32])
            .count();
        for mut record in records {
            if !record.burned {
                record.burned = true;
                mini_durable::atomic_replace(&directory.join(record.name()), &record.encode())
                    .map_err(journal_error)?;
            }
        }

        let signed_count = records_signed;
        let instance = mini_crypto::random_32().map_err(|_| CustodyError::Entropy)?;
        Ok(Self {
            directory,
            key,
            binding,
            instance,
            record_count,
            signed_count,
            poisoned: false,
            _lock: lock,
        })
    }

    fn ready(&mut self) -> Result<()> {
        if self.poisoned {
            return Err(journal_error(
                "storage outcome uncertain; signer must reopen",
            ));
        }
        match read_journal_file(
            &self.directory.join("signer.binding"),
            SIGNER_DOMAIN.len() + 512,
        ) {
            Ok(bytes) if *blake3::hash(&bytes).as_bytes() == self.binding => Ok(()),
            _ => {
                self.poisoned = true;
                Err(journal_error(
                    "signer manifest disappeared or changed; refusing publication",
                ))
            }
        }
    }

    fn persist(&mut self, record: &NonceRecord) -> Result<()> {
        if let Err(error) =
            mini_durable::atomic_replace(&self.directory.join(record.name()), &record.encode())
        {
            self.poisoned = true;
            return Err(journal_error(error));
        }
        Ok(())
    }

    /// Round 1: reserve a fresh nonce pair, durably recording its public
    /// commitment before returning it.
    pub fn commit<R>(&mut self, rng: &mut R) -> Result<(DurableSigningNonces, SigningCommitments)>
    where
        R: RngCore + CryptoRng,
    {
        self.ready()?;
        if self.record_count >= MAX_DURABLE_NONCE_RECORDS {
            return Err(journal_error("signer journal capacity exceeded"));
        }
        let (nonces, commitment) = round1_commit(&self.key, rng);
        let commitment_bytes = commitment
            .serialize()
            .map_err(|_| journal_error("failed to serialize a freshly generated commitment"))?;
        let record = NonceRecord {
            signer: self.binding,
            instance: self.instance,
            commitment: commitment_bytes,
            burned: false,
            transcript: [0; 32],
        };
        if self
            .directory
            .join(record.name())
            .try_exists()
            .map_err(journal_error)?
        {
            return Err(journal_error("nonce commitment already reserved or burned"));
        }
        self.persist(&record)?;
        self.record_count += 1;
        Ok((DurableSigningNonces { nonces, record }, commitment))
    }

    /// Round 2: burn the reservation, then compute this signer's share.
    /// Burns even if `package` turns out invalid -- a crash after the
    /// burn-barrier but before a response is released must never permit a
    /// retry over the same nonces.
    pub fn sign(
        &mut self,
        nonces: DurableSigningNonces,
        package: &SigningPackage,
    ) -> Result<SignatureShare> {
        self.ready()?;
        if nonces.record.signer != self.binding || nonces.record.instance != self.instance {
            return Err(journal_error(
                "nonce belongs to a different signer instance",
            ));
        }
        let path = self.directory.join(nonces.record.name());
        match read_journal_file(&path, 4096) {
            Ok(bytes) if bytes == nonces.record.encode() => {}
            _ => {
                self.poisoned = true;
                return Err(journal_error(
                    "nonce reservation is corrupt, missing, or burned",
                ));
            }
        }
        let mut record = nonces.record;
        record.burned = true;
        record.transcript = *blake3::hash(package.message()).as_bytes();
        self.persist(&record)?;
        self.signed_count += 1;
        round2_sign(package, nonces.nonces, &self.key)
    }

    /// Total `commit` + `sign` operations this journal has recorded. The
    /// external anchor ([`crate::anchor`]) counts the same operations, which is
    /// what lets a restored older copy of this directory be told apart.
    pub fn operation_count(&self) -> u64 {
        (self.record_count + self.signed_count) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frost_ristretto255::keys::{IdentifierList, PublicKeyPackage};
    use rand_core::OsRng;
    use std::collections::BTreeMap;

    fn keys(
        min_signers: u16,
        max_signers: u16,
    ) -> (BTreeMap<Identifier, KeyPackage>, PublicKeyPackage) {
        let (shares, pubkeys) = frost_ristretto255::keys::generate_with_dealer(
            max_signers,
            min_signers,
            IdentifierList::Default,
            OsRng,
        )
        .unwrap();
        let key_packages: BTreeMap<_, _> = shares
            .into_iter()
            .map(|(id, share)| (id, KeyPackage::try_from(share).unwrap()))
            .collect();
        (key_packages, pubkeys)
    }

    fn sign_with(
        key_packages: &BTreeMap<Identifier, KeyPackage>,
        pubkeys: &PublicKeyPackage,
        signers: &[Identifier],
        message: &[u8],
    ) -> Signature {
        let mut nonces_map = BTreeMap::new();
        let mut commitments = BTreeMap::new();
        for &id in signers {
            let (nonces, commitment) = round1_commit(&key_packages[&id], &mut OsRng);
            nonces_map.insert(id, nonces);
            commitments.insert(id, commitment);
        }
        let package = build_signing_package(commitments, message);
        let mut shares = BTreeMap::new();
        for &id in signers {
            let nonces = nonces_map.remove(&id).unwrap();
            let share = round2_sign(&package, nonces, &key_packages[&id]).unwrap();
            shares.insert(id, share);
        }
        aggregate_signature(&package, &shares, pubkeys).unwrap()
    }

    #[test]
    fn a_full_signing_round_produces_a_signature_the_group_key_verifies() {
        let (key_packages, pubkeys) = keys(2, 3);
        let signers: Vec<Identifier> = key_packages.keys().take(2).copied().collect();
        let message = b"gate-72 signing smoke test";
        let signature = sign_with(&key_packages, &pubkeys, &signers, message);
        assert!(pubkeys.verifying_key().verify(message, &signature).is_ok());
    }

    fn journal_dir(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mini-custody-signing-journal-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn durable_signer(dir: &std::path::Path, key: KeyPackage) -> DurableCustodySigner {
        DurableCustodySigner::open(dir, key).unwrap()
    }

    #[test]
    fn a_durable_signer_produces_a_verifiable_signature() {
        let (key_packages, pubkeys) = keys(2, 2);
        let ids: Vec<Identifier> = key_packages.keys().copied().collect();
        let dir_a = journal_dir("a");
        let dir_b = journal_dir("b");
        let mut a = durable_signer(&dir_a, key_packages[&ids[0]].clone());
        let mut b = durable_signer(&dir_b, key_packages[&ids[1]].clone());

        let (nonces_a, commitment_a) = a.commit(&mut OsRng).unwrap();
        let (nonces_b, commitment_b) = b.commit(&mut OsRng).unwrap();
        let message = b"durable custody signer smoke test";
        let package = build_signing_package(
            BTreeMap::from([(ids[0], commitment_a), (ids[1], commitment_b)]),
            message,
        );
        let share_a = a.sign(nonces_a, &package).unwrap();
        let share_b = b.sign(nonces_b, &package).unwrap();
        let signature = aggregate_signature(
            &package,
            &BTreeMap::from([(ids[0], share_a), (ids[1], share_b)]),
            &pubkeys,
        )
        .unwrap();
        assert!(pubkeys.verifying_key().verify(message, &signature).is_ok());
    }

    #[test]
    fn reopening_a_durable_signer_burns_any_uncompleted_reservation() {
        let (key_packages, pubkeys) = keys(2, 2);
        let mut ids = key_packages.keys().copied();
        let durable_id = ids.next().unwrap();
        let other_id = ids.next().unwrap();
        let key = key_packages[&durable_id].clone();
        let dir = journal_dir("reopen");
        {
            let mut signer = durable_signer(&dir, key.clone());
            let _ = signer.commit(&mut OsRng).unwrap();
            // Process "crashes" here: signer, and its in-memory secret
            // nonces, are dropped without ever calling `sign`.
        }
        let mut reopened = durable_signer(&dir, key.clone());
        // The leftover reservation was force-burned on reopen; a brand new
        // commit must still work (fresh capacity, fresh nonces). A second,
        // ordinary (non-durable) signer makes up the 2-of-2 threshold so
        // the resulting share can be aggregated into a real signature.
        let (nonces, commitment) = reopened.commit(&mut OsRng).unwrap();
        let (other_nonces, other_commitment) = round1_commit(&key_packages[&other_id], &mut OsRng);
        let package = build_signing_package(
            BTreeMap::from([(durable_id, commitment), (other_id, other_commitment)]),
            b"post-reopen",
        );
        let share = reopened.sign(nonces, &package).unwrap();
        let other_share = round2_sign(&package, other_nonces, &key_packages[&other_id]).unwrap();
        let signature = aggregate_signature(
            &package,
            &BTreeMap::from([(durable_id, share), (other_id, other_share)]),
            &pubkeys,
        )
        .unwrap();
        assert!(pubkeys
            .verifying_key()
            .verify(b"post-reopen", &signature)
            .is_ok());
    }

    #[test]
    fn a_durable_signer_rejects_a_journal_opened_for_a_different_key() {
        let (key_packages, _pubkeys) = keys(2, 3);
        let mut ids = key_packages.keys().copied();
        let first = ids.next().unwrap();
        let second = ids.next().unwrap();
        let dir = journal_dir("mismatched-key");
        drop(durable_signer(&dir, key_packages[&first].clone()));
        assert!(DurableCustodySigner::open(&dir, key_packages[&second].clone()).is_err());
    }
}
