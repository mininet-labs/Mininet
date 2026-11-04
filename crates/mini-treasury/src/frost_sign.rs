//! FROST (Flexible Round-Optimized Schnorr Threshold signatures, Komlo &
//! Goldberg) signing: two rounds that let any `threshold`-sized subset of
//! [`crate::frost_keygen`]'s participants jointly produce one ordinary
//! Schnorr signature under the group public key — without ever
//! reconstructing the group secret key at any single point, on any single
//! device, at any time.
//!
//! ## Why two rounds, and why a *binding factor*
//!
//! Round 1: every participant who might sign publishes a pair of nonce
//! commitments `(D_i, E_i) = (d_i*G, e_i*G)` for fresh random `d_i, e_i` —
//! before anyone knows which message will be signed. Round 2: once the
//! message and the final signing set are fixed, each participant computes
//! their response using both nonces, weighted by a *binding factor*
//! `rho_i = H(i, message, all commitments)`. The binding factor is what
//! stops a subtle attack on naive two-round Schnorr aggregation (Drijvers
//! et al.): without it, a coalition of signers can adaptively choose their
//! own nonces after seeing everyone else's, and forge a signature over a
//! different message than any honest signer agreed to. Binding every
//! signer's contribution to the *entire* commitment list and the message
//! closes that gap.
//!
//! ## The two identities this module's correctness rests on
//!
//! Both were hand-derived and checked term-by-term before writing this
//! code, the same discipline `mini_value::bp_range` used for Bulletproofs.
//!
//! **Individual share verification** — for signer `i` with Lagrange
//! coefficient `lambda_i` (see [`lagrange_coefficient`]) and per-signer
//! group-commitment contribution `R_i = D_i + rho_i*E_i`:
//!
//! ```text
//! z_i = d_i + e_i*rho_i + lambda_i*s_i*c
//! z_i*G = d_i*G + rho_i*e_i*G + lambda_i*c*s_i*G
//!       = D_i + rho_i*E_i + c*lambda_i*Y_i
//!       = R_i + c*lambda_i*Y_i
//! ```
//!
//! **Aggregate signature validity** — summing every signer's `z_i` and
//! `R_i`, and using Shamir reconstruction-in-the-exponent
//! (`sum_i lambda_i*s_i = f(0) = s`, the same identity
//! `frost_keygen`'s tests check directly):
//!
//! ```text
//! z = sum_i z_i = sum_i d_i + sum_i(e_i*rho_i) + c * sum_i(lambda_i*s_i)
//!   = sum_i d_i + sum_i(e_i*rho_i) + c*s
//! R = sum_i R_i = sum_i D_i + sum_i(rho_i*E_i) = (sum_i d_i + sum_i e_i*rho_i)*G
//! z*G = R + c*s*G = R + c*Y
//! ```
//!
//! — exactly the ordinary single-key Schnorr verification equation
//! (`z*G == R + c*Y`), which is why the *output* of FROST is an entirely
//! ordinary Schnorr signature: anyone verifying it later needs no idea
//! FROST, or a threshold scheme, or multiple signers, were ever involved.

use std::collections::BTreeMap;

use curve25519_dalek::traits::Identity;
use zeroize::Zeroize;

use crate::curve::{
    basepoint, hash_to_scalar, random_scalar, CompressedRistretto, RistrettoPoint, Scalar,
};
use crate::error::{Result, TreasuryError};
use crate::frost_keygen::{KeyPackage, PublicKeyPackage};

/// A participant's private round-1 nonces (`d_i`, `e_i`). Held only by that
/// participant, between round 1 and round 2 — never transmitted, never
/// reused across a second signature (reusing them leaks the secret share,
/// the same catastrophic failure mode as nonce reuse in plain Schnorr/
/// ECDSA). Deliberately **not** `Copy`/`Clone` — a self-zeroizing secret
/// that could be silently duplicated would leave un-zeroized copies behind,
/// defeating the point (issue #93); every call site holds exactly one
/// instance, by move or by reference, never by copy. [`Drop`] scrubs both
/// scalars; [`core::fmt::Debug`] is hand-written to redact them the same
/// way `mini_crypto::SigningKey` redacts its secret half.
pub struct SigningNonces {
    hiding: Scalar,
    binding: Scalar,
}

impl core::fmt::Debug for SigningNonces {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SigningNonces")
            .field("hiding", &"[redacted]")
            .field("binding", &"[redacted]")
            .finish()
    }
}

impl Drop for SigningNonces {
    fn drop(&mut self) {
        self.hiding.zeroize();
        self.binding.zeroize();
    }
}

/// A participant's public round-1 commitment `(D_i, E_i)`, safe to publish.
#[derive(Debug, Clone, Copy)]
pub struct NonceCommitment {
    /// Which participant this commitment belongs to.
    pub index: u16,
    hiding: RistrettoPoint,
    binding: RistrettoPoint,
}

/// Round 1: generate a fresh nonce pair and its public commitment for
/// `index`. Must be called again for every new signature — see
/// [`SigningNonces`]'s honest limit on reuse.
pub fn round1_commit(index: u16) -> Result<(SigningNonces, NonceCommitment)> {
    let hiding = random_scalar()?;
    let binding = random_scalar()?;
    let commitment = NonceCommitment {
        index,
        hiding: basepoint() * hiding,
        binding: basepoint() * binding,
    };
    Ok((SigningNonces { hiding, binding }, commitment))
}

/// A process-locked nonce journal for the existing FROST prototype.
/// Commitments are reserved before publication and burned durably before any
/// response is computed. Secret nonces are never serialized; restart burns
/// unfinished rounds. Handles from an earlier service instance are rejected.
///
/// Keep the journal with the signer. Full memory/disk snapshots, rollback of
/// every journal copy, and hardware that ignores flushes require independent
/// controls. This service does not authorize custody or replace external audit.
pub struct DurableFrostSigner {
    directory: std::path::PathBuf,
    key: KeyPackage,
    binding: [u8; 32],
    instance: [u8; 32],
    record_count: usize,
    poisoned: bool,
    _lock: std::fs::File,
    #[cfg(test)]
    fail_after_burn: bool,
}

impl core::fmt::Debug for DurableFrostSigner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DurableFrostSigner")
            .field("index", &self.key.index)
            .field("record_count", &self.record_count)
            .field("poisoned", &self.poisoned)
            .finish_non_exhaustive()
    }
}

/// Private nonce material reserved by one durable signer. It is neither
/// cloneable nor deserializable and cannot enter the raw signing path.
///
/// ```compile_fail
/// use mini_treasury::DurableSigningNonces;
/// fn duplicate(nonces: &DurableSigningNonces) -> DurableSigningNonces {
///     nonces.clone()
/// }
/// ```
///
/// ```compile_fail
/// use mini_treasury::{DurableFrostSigner, DurableSigningNonces, SigningPackage};
/// fn reuse(signer: &mut DurableFrostSigner, nonces: DurableSigningNonces, package: &SigningPackage) {
///     let _ = signer.sign(nonces, package);
///     let _ = signer.sign(nonces, package);
/// }
/// ```
///
/// ```compile_fail
/// use mini_treasury::{round2_sign, DurableSigningNonces, KeyPackage, SigningPackage};
/// fn bypass(key: &KeyPackage, nonces: DurableSigningNonces, package: &SigningPackage) {
///     let _ = round2_sign(key, nonces, package);
/// }
/// ```
#[derive(Debug)]
pub struct DurableSigningNonces {
    nonces: SigningNonces,
    record: NonceRecord,
}

const SIGNER_DOMAIN: &[u8] = b"mini-treasury/durable-signer/v2\0";
const NONCE_DOMAIN: &[u8] = b"mini-treasury/nonce-record/v2\0";
const NONCE_RECORD_BYTES: usize = NONCE_DOMAIN.len() + 6 * 32 + 1;
/// Retention is conservative: never delete commitments to regain capacity.
/// Exhaustion requires an explicitly reviewed signer/journal migration.
pub const MAX_DURABLE_NONCE_RECORDS: usize = 100_000;

#[derive(Debug)]
struct NonceRecord {
    signer: [u8; 32],
    instance: [u8; 32],
    hiding: [u8; 32],
    binding: [u8; 32],
    burned: bool,
    transcript: [u8; 32],
}
impl NonceRecord {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = NONCE_DOMAIN.to_vec();
        bytes.extend_from_slice(&self.signer);
        bytes.extend_from_slice(&self.instance);
        bytes.extend_from_slice(&self.hiding);
        bytes.extend_from_slice(&self.binding);
        bytes.push(u8::from(self.burned));
        bytes.extend_from_slice(&self.transcript);
        let checksum = blake3::hash(&bytes);
        bytes.extend_from_slice(checksum.as_bytes());
        bytes
    }
    fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != NONCE_RECORD_BYTES || !bytes.starts_with(NONCE_DOMAIN) {
            return Err(journal_error("invalid nonce journal record length/version"));
        }
        let (content, checksum) = bytes.split_at(bytes.len() - 32);
        if blake3::hash(content).as_bytes() != checksum {
            return Err(journal_error("nonce journal checksum mismatch"));
        }
        let mut cursor = NONCE_DOMAIN.len();
        let mut field = || {
            let mut value = [0; 32];
            value.copy_from_slice(&bytes[cursor..cursor + 32]);
            cursor += 32;
            value
        };
        let signer = field();
        let instance = field();
        let hiding = field();
        let binding = field();
        let burned = match bytes[cursor] {
            0 => false,
            1 => true,
            _ => return Err(journal_error("invalid nonce record status")),
        };
        cursor += 1;
        let mut transcript = [0; 32];
        transcript.copy_from_slice(&bytes[cursor..cursor + 32]);
        if (!burned && transcript != [0; 32])
            || CompressedRistretto(hiding).decompress().is_none()
            || CompressedRistretto(binding).decompress().is_none()
        {
            return Err(journal_error("invalid nonce record fields"));
        }
        Ok(Self {
            signer,
            instance,
            hiding,
            binding,
            burned,
            transcript,
        })
    }
    fn name(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"mini-treasury/nonce-name/v2\0");
        hasher.update(&self.signer);
        hasher.update(&self.hiding);
        hasher.update(&self.binding);
        format!("{}.nonce", hasher.finalize().to_hex())
    }
}
fn journal_error(error: impl std::fmt::Display) -> TreasuryError {
    TreasuryError::SigningJournal(error.to_string())
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

impl DurableFrostSigner {
    pub fn open(directory: impl Into<std::path::PathBuf>, key: KeyPackage) -> Result<Self> {
        let directory = directory.into();
        if key.index == 0
            || key.index > crate::frost_keygen::MAX_PARTICIPANTS
            || key.group_public_key == RistrettoPoint::identity()
            || key.secret_share == Scalar::ZERO
        {
            return Err(TreasuryError::InvalidFrostParticipant);
        }
        mini_durable::create_dir_all(&directory).map_err(journal_error)?;
        let lock = mini_durable::try_lock_exclusive(&directory.join("signer.lock"))
            .map_err(journal_error)?;
        // Include the verification share: resharing can preserve the group key
        // and participant index while replacing this participant's secret.
        let mut manifest = SIGNER_DOMAIN.to_vec();
        manifest.extend_from_slice(&key.index.to_be_bytes());
        manifest.extend_from_slice(key.group_public_key.compress().as_bytes());
        manifest.extend_from_slice((basepoint() * key.secret_share).compress().as_bytes());
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
            let record = NonceRecord::decode(&read_journal_file(&path, NONCE_RECORD_BYTES)?)?;
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
        // Re-establish barriers in case a previous manifest rename returned an
        // uncertain error. This runs before any new commitment is released.
        mini_durable::atomic_replace(&manifest_path, &manifest).map_err(journal_error)?;
        for mut record in records {
            if !record.burned {
                record.burned = true;
                mini_durable::atomic_replace(&directory.join(record.name()), &record.encode())
                    .map_err(journal_error)?;
            }
        }
        let instance = mini_crypto::random_32().map_err(|_| TreasuryError::Entropy)?;
        Ok(Self {
            directory,
            key,
            binding,
            instance,
            record_count,
            poisoned: false,
            _lock: lock,
            #[cfg(test)]
            fail_after_burn: false,
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
            SIGNER_DOMAIN.len() + 66,
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
    pub fn commit(&mut self) -> Result<(DurableSigningNonces, NonceCommitment)> {
        self.ready()?;
        if self.record_count >= MAX_DURABLE_NONCE_RECORDS {
            return Err(journal_error("signer journal capacity exceeded"));
        }
        let (nonces, commitment) = round1_commit(self.key.index)?;
        let record = NonceRecord {
            signer: self.binding,
            instance: self.instance,
            hiding: commitment.hiding.compress().to_bytes(),
            binding: commitment.binding.compress().to_bytes(),
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

    /// Burns even on an invalid package. A crash after the barrier may lose
    /// the response; neither it nor an application retry permits nonce reuse.
    pub fn sign(
        &mut self,
        nonces: DurableSigningNonces,
        package: &SigningPackage,
    ) -> Result<Scalar> {
        self.ready()?;
        if nonces.record.signer != self.binding || nonces.record.instance != self.instance {
            return Err(journal_error(
                "nonce belongs to a different signer instance",
            ));
        }
        let path = self.directory.join(nonces.record.name());
        match read_journal_file(&path, NONCE_RECORD_BYTES) {
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
        record.transcript = package.transcript_digest();
        self.persist(&record)?;
        #[cfg(test)]
        if self.fail_after_burn {
            self.poisoned = true;
            return Err(journal_error(
                "injected interruption after burn, before response",
            ));
        }
        round2_sign(&self.key, nonces.nonces, package)
    }
}

/// The coordinator-assembled bundle every round-2 signer needs: the
/// message being signed, and every participating signer's round-1
/// commitment. Constructing one enforces that at least `threshold`
/// distinct signers are present — signing with fewer is rejected here,
/// not discovered later as an unverifiable aggregate signature.
#[derive(Debug, Clone)]
pub struct SigningPackage {
    message: Vec<u8>,
    commitments: BTreeMap<u16, NonceCommitment>,
}

impl SigningPackage {
    /// Bundle `message` with `commitments` (one round-1 commitment per
    /// participating signer). Rejects duplicate indices and a signing set
    /// smaller than `threshold`.
    pub fn new(
        threshold: u16,
        message: Vec<u8>,
        commitments: Vec<NonceCommitment>,
    ) -> Result<Self> {
        if commitments.len() < threshold as usize {
            return Err(TreasuryError::NotEnoughSigners);
        }
        let mut map = BTreeMap::new();
        for commitment in commitments {
            if map.insert(commitment.index, commitment).is_some() {
                return Err(TreasuryError::InvalidFrostParticipant);
            }
        }
        Ok(SigningPackage {
            message,
            commitments: map,
        })
    }

    fn transcript_digest(&self) -> [u8; 32] {
        let mut hash = blake3::Hasher::new();
        hash.update(b"mini-treasury/signing-transcript/v1\0");
        hash.update(&(self.message.len() as u64).to_be_bytes());
        hash.update(&self.message);
        for (index, commitment) in &self.commitments {
            hash.update(&index.to_be_bytes());
            hash.update(commitment.hiding.compress().as_bytes());
            hash.update(commitment.binding.compress().as_bytes());
        }
        *hash.finalize().as_bytes()
    }

    fn indices(&self) -> Vec<Scalar> {
        self.commitments.keys().map(|&i| index_scalar(i)).collect()
    }

    /// Every binding factor `rho_j = H(j, message, all commitments)`, one
    /// per participating signer, keyed by index.
    fn binding_factors(&self) -> BTreeMap<u16, Scalar> {
        // Bind to the whole sorted commitment list so no signer can change
        // their own or anyone else's contribution after the fact.
        let mut transcript = Vec::new();
        for commitment in self.commitments.values() {
            transcript.extend_from_slice(&commitment.index.to_be_bytes());
            transcript.extend_from_slice(commitment.hiding.compress().as_bytes());
            transcript.extend_from_slice(commitment.binding.compress().as_bytes());
        }

        self.commitments
            .keys()
            .map(|&j| {
                let rho_j = hash_to_scalar(&[
                    b"mini-treasury/frost/binding-factor",
                    &j.to_be_bytes(),
                    &self.message,
                    &transcript,
                ]);
                (j, rho_j)
            })
            .collect()
    }

    /// The group commitment `R = sum_i (D_i + rho_i*E_i)`.
    fn group_commitment(&self, binding_factors: &BTreeMap<u16, Scalar>) -> RistrettoPoint {
        let mut r = RistrettoPoint::identity();
        for commitment in self.commitments.values() {
            let rho = binding_factors[&commitment.index];
            r += commitment.hiding + commitment.binding * rho;
        }
        r
    }

    /// Signer `index`'s own contribution `R_i = D_i + rho_i*E_i` to the
    /// group commitment, or `None` if `index` never published a round-1
    /// commitment into this signing round (F-02: `index` may still be a
    /// real, verifying-share-holding member of the group as a whole — group
    /// membership and *this round's* participation are different facts, and
    /// conflating them by indexing this map directly used to panic instead
    /// of reporting an unknown participant).
    fn per_signer_commitment(
        &self,
        index: u16,
        binding_factors: &BTreeMap<u16, Scalar>,
    ) -> Option<RistrettoPoint> {
        let commitment = self.commitments.get(&index)?;
        let rho = binding_factors.get(&index)?;
        Some(commitment.hiding + commitment.binding * rho)
    }
}

/// The Schnorr challenge `c = H(R, Y, message)`.
fn challenge(
    group_commitment: RistrettoPoint,
    group_public_key: RistrettoPoint,
    message: &[u8],
) -> Scalar {
    hash_to_scalar(&[
        b"mini-treasury/frost/challenge",
        group_commitment.compress().as_bytes(),
        group_public_key.compress().as_bytes(),
        message,
    ])
}

/// This signer's Shamir/Lagrange coefficient for reconstruction at `x=0`,
/// given the full set of participating indices: `lambda_i = prod_{j != i}
/// x_j / (x_j - x_i)`. Every participant in a signing round computes the
/// *same* value for the *same* signing set — it depends only on which
/// indices are signing, not on any secret.
pub(crate) fn lagrange_coefficient(index: Scalar, all_indices: &[Scalar]) -> Scalar {
    let mut numerator = Scalar::ONE;
    let mut denominator = Scalar::ONE;
    for &j in all_indices {
        if j == index {
            continue;
        }
        numerator *= j;
        denominator *= j - index;
    }
    numerator * denominator.invert()
}

fn index_scalar(index: u16) -> Scalar {
    Scalar::from(index as u64)
}

/// Round 2: compute this signer's response `z_i` to `signing_package`,
/// consuming the nonces generated for it in round 1.
///
/// Takes `nonces` **by value**, not by reference (F-01): a `&SigningNonces`
/// could be handed to this function twice — once per signing package — and
/// nothing in the type system or the old signature stopped a caller from
/// doing exactly that. Two responses over the same `(d_i, e_i)` pair under
/// three independent transcripts can provide three linear equations in
/// the two nonce unknowns and secret share, recovering all three — the
/// catastrophic failure is the same as nonce
/// reuse in plain Schnorr/ECDSA. Consuming `nonces` means Rust's move
/// checker refuses a second call at compile time, and [`SigningNonces`]'s
/// own [`Drop`] zeroizes both scalars the moment this function returns on
/// *any* path (success or error). Durable service callers additionally use
/// [`DurableFrostSigner`] to burn a journal record before releasing a response.
///
/// Also verifies (F-01's second half) that `nonces` actually derives the
/// `(D_i, E_i)` commitment `signing_package` claims for this signer's
/// index, rejecting a stale, foreign, or mismatched `SigningNonces` value
/// before it can contribute to a response at all.
pub fn round2_sign(
    key_package: &KeyPackage,
    nonces: SigningNonces,
    signing_package: &SigningPackage,
) -> Result<Scalar> {
    let Some(commitment) = signing_package.commitments.get(&key_package.index) else {
        return Err(TreasuryError::InvalidFrostParticipant);
    };
    if (basepoint() * nonces.hiding).compress() != commitment.hiding.compress()
        || (basepoint() * nonces.binding).compress() != commitment.binding.compress()
    {
        return Err(TreasuryError::NonceCommitmentMismatch);
    }
    let indices = signing_package.indices();
    let binding_factors = signing_package.binding_factors();
    let r = signing_package.group_commitment(&binding_factors);
    let c = challenge(r, key_package.group_public_key, &signing_package.message);
    let rho_i = binding_factors[&key_package.index];
    let lambda_i = lagrange_coefficient(index_scalar(key_package.index), &indices);

    Ok(nonces.hiding + nonces.binding * rho_i + lambda_i * key_package.secret_share * c)
}

/// Verify signer `index`'s share `z_i` against their public verification
/// share, *before* aggregating — catches a faulty or malicious signer
/// immediately, with attribution, instead of only learning the final
/// aggregate signature doesn't verify.
///
/// `index` must be **both** a real group member (checked against
/// `public_key_package.verifying_shares`) **and** an actual participant in
/// this specific signing round (checked against `signing_package`'s own
/// commitments) — group membership and round participation are different
/// facts (F-02). A real group member absent from this round's commitments
/// now returns [`TreasuryError::InvalidFrostParticipant`] instead of
/// panicking on a missing map key.
pub fn verify_signature_share(
    index: u16,
    z_i: Scalar,
    signing_package: &SigningPackage,
    public_key_package: &PublicKeyPackage,
) -> Result<bool> {
    let Some(&y_i) = public_key_package.verifying_shares.get(&index) else {
        return Err(TreasuryError::InvalidFrostParticipant);
    };
    let indices = signing_package.indices();
    let binding_factors = signing_package.binding_factors();
    let r = signing_package.group_commitment(&binding_factors);
    let c = challenge(
        r,
        public_key_package.group_public_key,
        &signing_package.message,
    );
    let lambda_i = lagrange_coefficient(index_scalar(index), &indices);
    let Some(r_i) = signing_package.per_signer_commitment(index, &binding_factors) else {
        return Err(TreasuryError::InvalidFrostParticipant);
    };

    Ok((basepoint() * z_i).compress() == (r_i + (c * lambda_i) * y_i).compress())
}

/// An ordinary Schnorr signature `(R, z)` — the output of FROST looks
/// exactly like a signature from a single key, by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signature {
    r: CompressedRistretto,
    z: Scalar,
}

impl Signature {
    /// Serialize to the 64-byte wire format (`R || z`, both 32 bytes).
    pub fn to_bytes(self) -> [u8; 64] {
        let mut out = [0u8; 64];
        out[..32].copy_from_slice(self.r.as_bytes());
        out[32..].copy_from_slice(self.z.as_bytes());
        out
    }

    /// Deserialize from the 64-byte wire format. `None` if malformed (wrong
    /// length, the first 32 bytes are not a valid compressed Ristretto
    /// point, or the last 32 bytes are not `z`'s canonical little-endian
    /// encoding).
    ///
    /// The scalar half is **canonically** decoded (F-03), not reduced mod
    /// the group order: `Scalar::from_bytes_mod_order` maps every byte
    /// string in `[0, 2^256)` onto the same `[0, ell)` range a canonical
    /// encoding already covers, so two different 32-byte strings (e.g. `z`
    /// and `z + ell`) can decode to the same signature. That decoder
    /// aliasing conflicts with this workspace's byte-addressed identity and
    /// deduplication expectations for a serialized signature — unlike a
    /// hash's wide-reduction, which legitimately maps a larger input space
    /// down, a signature's `z` is meant to be a unique 32-byte value with
    /// one accepted encoding.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 64 {
            return None;
        }
        let r_bytes: [u8; 32] = bytes[..32].try_into().ok()?;
        let z_bytes: [u8; 32] = bytes[32..64].try_into().ok()?;
        // Confirm it decompresses to a real point now, so a malformed
        // signature is rejected here rather than surfacing later as a
        // confusing verification failure.
        CompressedRistretto(r_bytes).decompress()?;
        let z: Option<Scalar> = Scalar::from_canonical_bytes(z_bytes).into();
        let z = z?;
        Some(Signature {
            r: CompressedRistretto(r_bytes),
            z,
        })
    }
}

/// Combine per-signer shares into the final signature. Every share is
/// verified individually first (see [`verify_signature_share`]) so a bad
/// share is caught and attributed rather than silently producing an
/// aggregate that fails to verify.
///
/// Requires `shares` to name **exactly** the same participant indices as
/// `signing_package`'s own commitments — not merely the same *count*
/// (F-02). Equal cardinality alone lets a caller substitute one real group
/// member's index for another who never actually took part in this round
/// (e.g. commitments for `{1,2}`, shares keyed `{1,3}`): the substituted
/// index is a genuine member of the group as a whole, so it would
/// previously reach `verify_signature_share`'s internal map lookups with
/// nothing having rejected it first.
pub fn aggregate(
    signing_package: &SigningPackage,
    shares: &BTreeMap<u16, Scalar>,
    public_key_package: &PublicKeyPackage,
) -> Result<Signature> {
    let committed_indices: Vec<u16> = signing_package.commitments.keys().copied().collect();
    let share_indices: Vec<u16> = shares.keys().copied().collect();
    if share_indices != committed_indices {
        return Err(TreasuryError::InvalidFrostParticipant);
    }
    let mut z = Scalar::ZERO;
    for (&index, &z_i) in shares {
        if !verify_signature_share(index, z_i, signing_package, public_key_package)? {
            return Err(TreasuryError::InvalidFrostSignatureShare);
        }
        z += z_i;
    }

    let binding_factors = signing_package.binding_factors();
    let r = signing_package.group_commitment(&binding_factors);
    Ok(Signature { r: r.compress(), z })
}

/// Verify a completed FROST signature exactly as any ordinary Schnorr
/// verifier would, with no knowledge that a threshold scheme was involved:
/// `z*G == R + c*Y`.
pub fn verify(signature: &Signature, message: &[u8], group_public_key: RistrettoPoint) -> bool {
    let Some(r) = signature.r.decompress() else {
        return false;
    };
    let c = challenge(r, group_public_key, message);
    (basepoint() * signature.z).compress() == (r + c * group_public_key).compress()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frost_keygen::{trusted_dealer_keygen, AcknowledgedPrototypeOnly};

    fn ack() -> AcknowledgedPrototypeOnly {
        AcknowledgedPrototypeOnly::insecure_trusted_dealer_keygen_is_not_production_ready()
    }

    #[test]
    fn debug_output_redacts_both_secret_scalars() {
        let (nonces, _commitment) = round1_commit(1).unwrap();
        let debug_string = format!("{:?}", nonces);
        assert!(debug_string.contains("[redacted]"));
        // The redacted string must not contain either scalar's actual
        // encoding -- spot-check by confirming the hiding/binding field
        // values never appear as their own hex/byte representation.
        assert!(!debug_string.contains(&format!("{:?}", nonces_hiding_bytes(&nonces))));
    }

    /// Test-only accessor: reaches into the private field so the redaction
    /// test above can prove the real bytes are absent from `Debug` output,
    /// without this crate's real API ever exposing the nonce scalar itself.
    fn nonces_hiding_bytes(nonces: &SigningNonces) -> [u8; 32] {
        *nonces.hiding.as_bytes()
    }

    fn sign_with(
        signer_indices: &[u16],
        shares: &[KeyPackage],
        public: &PublicKeyPackage,
        threshold: u16,
        message: &[u8],
    ) -> Signature {
        let mut nonces_by_index = BTreeMap::new();
        let mut commitments = Vec::new();
        for &i in signer_indices {
            let (nonces, commitment) = round1_commit(i).unwrap();
            nonces_by_index.insert(i, nonces);
            commitments.push(commitment);
        }
        let signing_package =
            SigningPackage::new(threshold, message.to_vec(), commitments).unwrap();

        let mut z_shares = BTreeMap::new();
        for &i in signer_indices {
            let key_package = shares.iter().find(|s| s.index == i).unwrap();
            let nonces = nonces_by_index.remove(&i).unwrap();
            let z_i = round2_sign(key_package, nonces, &signing_package).unwrap();
            z_shares.insert(i, z_i);
        }

        aggregate(&signing_package, &z_shares, public).unwrap()
    }

    #[test]
    fn a_threshold_sized_subset_produces_a_valid_signature() {
        let (shares, public) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        let message = b"send 10 BTC-equivalent MINI to treasury payout #42";
        let signature = sign_with(&[1, 2, 3], &shares, &public, 3, message);
        assert!(verify(&signature, message, public.group_public_key));
    }

    #[test]
    fn a_different_threshold_sized_subset_also_produces_a_valid_signature() {
        let (shares, public) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        let message = b"treasury payout #43";
        let signature = sign_with(&[2, 4, 5], &shares, &public, 3, message);
        assert!(verify(&signature, message, public.group_public_key));
    }

    #[test]
    fn fewer_than_threshold_signers_are_rejected_at_package_construction() {
        let (_, _public) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        let (_, c1) = round1_commit(1).unwrap();
        let (_, c2) = round1_commit(2).unwrap();
        let err = SigningPackage::new(3, b"msg".to_vec(), vec![c1, c2]).unwrap_err();
        assert_eq!(err, TreasuryError::NotEnoughSigners);
    }

    #[test]
    fn duplicate_signer_index_is_rejected() {
        let (_, c1) = round1_commit(1).unwrap();
        let (_, c2) = round1_commit(1).unwrap();
        let err = SigningPackage::new(2, b"msg".to_vec(), vec![c1, c2]).unwrap_err();
        assert_eq!(err, TreasuryError::InvalidFrostParticipant);
    }

    #[test]
    fn a_tampered_signature_share_is_caught_before_aggregation() {
        let (shares, public) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        let message = b"treasury payout #44";
        let signer_indices = [1, 2, 3];

        let mut nonces_by_index = BTreeMap::new();
        let mut commitments = Vec::new();
        for &i in &signer_indices {
            let (nonces, commitment) = round1_commit(i).unwrap();
            nonces_by_index.insert(i, nonces);
            commitments.push(commitment);
        }
        let signing_package = SigningPackage::new(3, message.to_vec(), commitments).unwrap();

        let mut z_shares = BTreeMap::new();
        for &i in &signer_indices {
            let key_package = shares.iter().find(|s| s.index == i).unwrap();
            let nonces = nonces_by_index.remove(&i).unwrap();
            let z_i = round2_sign(key_package, nonces, &signing_package).unwrap();
            z_shares.insert(i, z_i);
        }
        // Tamper with one signer's share.
        *z_shares.get_mut(&2).unwrap() += Scalar::ONE;

        let err = aggregate(&signing_package, &z_shares, &public).unwrap_err();
        assert_eq!(err, TreasuryError::InvalidFrostSignatureShare);
    }

    #[test]
    fn signature_fails_verification_under_a_different_message() {
        let (shares, public) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        let message = b"treasury payout #45";
        let signature = sign_with(&[1, 2, 3], &shares, &public, 3, message);
        assert!(!verify(
            &signature,
            b"treasury payout #46 (attacker-modified)",
            public.group_public_key
        ));
    }

    #[test]
    fn signature_fails_verification_under_a_different_group_key() {
        let (shares, public_a) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        let (_, public_b) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        let message = b"treasury payout #47";
        let signature = sign_with(&[1, 2, 3], &shares, &public_a, 3, message);
        assert!(!verify(&signature, message, public_b.group_public_key));
    }

    #[test]
    fn wrong_length_signature_bytes_are_rejected_without_panicking() {
        assert!(Signature::from_bytes(&[0u8; 10]).is_none());
        assert!(Signature::from_bytes(&[0u8; 63]).is_none());
        assert!(Signature::from_bytes(&[0u8; 65]).is_none());
    }

    #[test]
    fn an_invalid_curve_point_in_signature_bytes_is_rejected_without_panicking() {
        // 0xFF repeated is not a valid compressed Ristretto encoding (it is
        // not the canonical little-endian encoding of any coset
        // representative), so decompression must fail rather than the
        // decoder silently accepting garbage as a point.
        let bytes = [0xFFu8; 64];
        assert!(Signature::from_bytes(&bytes).is_none());
    }

    #[test]
    fn signature_round_trips_through_bytes() {
        let (shares, public) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        let message = b"treasury payout #48";
        let signature = sign_with(&[1, 2, 3], &shares, &public, 3, message);
        let bytes = signature.to_bytes();
        let decoded = Signature::from_bytes(&bytes).unwrap();
        assert!(verify(&decoded, message, public.group_public_key));
    }

    // -------------------------------------------------------------------
    // F-03: Signature::from_bytes canonically decodes z, not mod-order
    // -------------------------------------------------------------------

    /// A real, valid `R` (compressed Ristretto point bytes) to pair with
    /// hand-built `z` values below -- these tests are about the scalar
    /// half's decoding, not the point half.
    fn a_valid_r() -> [u8; 32] {
        let (shares, public) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        let signature = sign_with(&[1, 2, 3], &shares, &public, 3, b"treasury payout #55");
        signature.to_bytes()[..32].try_into().unwrap()
    }

    fn signature_bytes(r: [u8; 32], z: [u8; 32]) -> [u8; 64] {
        let mut out = [0u8; 64];
        out[..32].copy_from_slice(&r);
        out[32..].copy_from_slice(&z);
        out
    }

    /// The Ristretto/Ed25519 group order `ell = 2^252 +
    /// 27742317777372353535851937790883648493`, little-endian -- the exact
    /// published constant `curve25519-dalek` itself uses internally
    /// (`constants::BASEPOINT_ORDER_PRIVATE`), copied here because the
    /// public alias for it was deprecated in 4.1.1 with no replacement.
    /// Not a scalar in canonical range: a valid scalar is `< ell`.
    const GROUP_ORDER_BYTES: [u8; 32] = [
        0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde,
        0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x10,
    ];

    #[test]
    fn canonical_zero_and_order_minus_one_are_accepted() {
        let r = a_valid_r();
        assert!(Signature::from_bytes(&signature_bytes(r, [0u8; 32])).is_some());

        // ell - 1, the largest canonical scalar, little-endian.
        let mut order_minus_one = GROUP_ORDER_BYTES;
        order_minus_one[0] -= 1;
        assert!(Signature::from_bytes(&signature_bytes(r, order_minus_one)).is_some());
    }

    #[test]
    fn the_group_order_itself_is_rejected_not_reduced_to_zero() {
        // ell reduces to 0 under from_bytes_mod_order -- the exact aliasing
        // this decoder must no longer perform.
        let r = a_valid_r();
        assert!(Signature::from_bytes(&signature_bytes(r, GROUP_ORDER_BYTES)).is_none());
    }

    #[test]
    fn all_ones_bytes_are_rejected() {
        let r = a_valid_r();
        assert!(Signature::from_bytes(&signature_bytes(r, [0xFFu8; 32])).is_none());
    }

    #[test]
    fn distinct_byte_strings_no_longer_alias_to_the_same_signature() {
        // The exact hole this closes: z=0 and z=ell used to decode
        // identically under mod-order reduction. They must now decode to
        // either two different signatures or one rejected input, never the
        // same accepted signature from two different wire encodings.
        let r = a_valid_r();
        let zero_sig = Signature::from_bytes(&signature_bytes(r, [0u8; 32]));
        let order_sig = Signature::from_bytes(&signature_bytes(r, GROUP_ORDER_BYTES));
        assert!(zero_sig.is_some());
        assert!(order_sig.is_none());
    }

    #[test]
    fn honest_signatures_still_round_trip_after_canonical_decoding() {
        // Reconfirms signature_round_trips_through_bytes under a different
        // signer set/message, guarding against the canonical check being
        // too strict for real signer output.
        let (shares, public) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        let message = b"treasury payout #56";
        let signature = sign_with(&[2, 3, 4], &shares, &public, 3, message);
        let decoded = Signature::from_bytes(&signature.to_bytes()).unwrap();
        assert!(verify(&decoded, message, public.group_public_key));
    }

    // -------------------------------------------------------------------
    // F-01: round2_sign consumes SigningNonces and checks the commitment
    // -------------------------------------------------------------------
    //
    // A *second call* with the same `SigningNonces` value is not tested
    // here at runtime because it cannot happen at runtime: `round2_sign`
    // now takes `nonces: SigningNonces` by value, so Rust's move checker
    // refuses a second use at compile time -- a strictly stronger
    // guarantee than any test could demonstrate. What a runtime test can
    // and does check is the other half of F-01's fix: that the nonces
    // actually correspond to the published commitment for this signer.

    #[test]
    fn round2_sign_rejects_nonces_that_do_not_derive_the_published_commitment() {
        let (shares, _public) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        // Two independent round-1 runs for the same index: the commitment
        // published in the signing package comes from the first, but the
        // signer (by bug, stale cache, or malice) supplies the nonces from
        // the second.
        let (_stale_nonces, published_commitment) = round1_commit(1).unwrap();
        let (fresh_nonces, _unpublished_commitment) = round1_commit(1).unwrap();
        let (_, c2) = round1_commit(2).unwrap();
        let (_, c3) = round1_commit(3).unwrap();
        let signing_package = SigningPackage::new(
            3,
            b"treasury payout #49".to_vec(),
            vec![published_commitment, c2, c3],
        )
        .unwrap();
        let key_package = shares.iter().find(|s| s.index == 1).unwrap();

        let err = round2_sign(key_package, fresh_nonces, &signing_package).unwrap_err();
        assert_eq!(err, TreasuryError::NonceCommitmentMismatch);
    }

    #[test]
    fn round2_sign_accepts_the_matching_nonces_for_the_same_published_commitment() {
        // Sanity check alongside the mismatch test above: the honest path
        // (nonces paired with their own commitment) must still succeed.
        let (shares, _public) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        let (nonces, commitment) = round1_commit(1).unwrap();
        let (_, c2) = round1_commit(2).unwrap();
        let (_, c3) = round1_commit(3).unwrap();
        let signing_package =
            SigningPackage::new(3, b"treasury payout #50".to_vec(), vec![commitment, c2, c3])
                .unwrap();
        let key_package = shares.iter().find(|s| s.index == 1).unwrap();

        assert!(round2_sign(key_package, nonces, &signing_package).is_ok());
    }

    // -------------------------------------------------------------------
    // F-02: a real group member absent from this signing round is a typed
    // error, not a panic
    // -------------------------------------------------------------------

    #[test]
    fn verify_signature_share_rejects_a_real_group_member_absent_from_this_round() {
        let (_, public) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        let (_, c1) = round1_commit(1).unwrap();
        let (_, c2) = round1_commit(2).unwrap();
        // Round only ever committed indices {1, 2}; index 4 is a real
        // member of the 5-participant group but never took part here.
        let signing_package =
            SigningPackage::new(2, b"treasury payout #51".to_vec(), vec![c1, c2]).unwrap();

        let err = verify_signature_share(4, Scalar::ZERO, &signing_package, &public).unwrap_err();
        assert_eq!(err, TreasuryError::InvalidFrostParticipant);
    }

    #[test]
    fn aggregate_rejects_an_equal_sized_substituted_participant_set() {
        // The exact attack F-02 names: commitments for {1,2}, shares keyed
        // {1,3}. Same cardinality, different membership -- must not reach
        // the internal per-signer lookups at all.
        let (shares, public) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        let (nonces1, c1) = round1_commit(1).unwrap();
        let (_, c2) = round1_commit(2).unwrap();
        let signing_package =
            SigningPackage::new(2, b"treasury payout #52".to_vec(), vec![c1, c2]).unwrap();
        let key_package1 = shares.iter().find(|s| s.index == 1).unwrap();
        let z1 = round2_sign(key_package1, nonces1, &signing_package).unwrap();

        let mut substituted = BTreeMap::new();
        substituted.insert(1u16, z1);
        // Index 3 is a real group member (threshold 3-of-5) but never
        // published a commitment into this round.
        substituted.insert(3u16, Scalar::ZERO);

        let err = aggregate(&signing_package, &substituted, &public).unwrap_err();
        assert_eq!(err, TreasuryError::InvalidFrostParticipant);
    }

    #[test]
    fn aggregate_rejects_missing_and_extra_participants() {
        let (shares, public) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        let (nonces1, c1) = round1_commit(1).unwrap();
        let (nonces2, c2) = round1_commit(2).unwrap();
        let signing_package =
            SigningPackage::new(2, b"treasury payout #53".to_vec(), vec![c1, c2]).unwrap();
        let key_package1 = shares.iter().find(|s| s.index == 1).unwrap();
        let key_package2 = shares.iter().find(|s| s.index == 2).unwrap();
        let z1 = round2_sign(key_package1, nonces1, &signing_package).unwrap();
        let z2 = round2_sign(key_package2, nonces2, &signing_package).unwrap();

        // Missing: only one of the two committed shares supplied.
        let mut missing = BTreeMap::new();
        missing.insert(1u16, z1);
        assert_eq!(
            aggregate(&signing_package, &missing, &public).unwrap_err(),
            TreasuryError::InvalidFrostParticipant
        );

        // Extra: both committed shares plus an uncommitted third.
        let mut extra = BTreeMap::new();
        extra.insert(1u16, z1);
        extra.insert(2u16, z2);
        extra.insert(3u16, Scalar::ZERO);
        assert_eq!(
            aggregate(&signing_package, &extra, &public).unwrap_err(),
            TreasuryError::InvalidFrostParticipant
        );
    }

    #[test]
    fn verify_signature_share_rejects_an_unknown_participant_id() {
        let (_, public) = trusted_dealer_keygen(5, 3, ack()).unwrap();
        let (_, c1) = round1_commit(1).unwrap();
        let (_, c2) = round1_commit(2).unwrap();
        let signing_package =
            SigningPackage::new(2, b"treasury payout #54".to_vec(), vec![c1, c2]).unwrap();

        // Index 999 is not a member of the group at all.
        let err = verify_signature_share(999, Scalar::ZERO, &signing_package, &public).unwrap_err();
        assert_eq!(err, TreasuryError::InvalidFrostParticipant);
    }

    fn journal_dir(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mini-frost-journal-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    /// Reopen a journal whose previous signer was just dropped. A sibling test
    /// that spawns a child process can, for the instant between fork and exec,
    /// hold a duplicate of the lock file's descriptor, so the lock may read as
    /// busy for a few milliseconds after the drop. Retry only that case.
    fn reopen_when_free(root: &std::path::Path, key: KeyPackage) -> DurableFrostSigner {
        for _ in 0..200 {
            match DurableFrostSigner::open(root, key.clone()) {
                Err(TreasuryError::SigningJournal(message)) if message.contains("would block") => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                other => return other.unwrap(),
            }
        }
        DurableFrostSigner::open(root, key).unwrap()
    }
    fn disk_record(root: &std::path::Path, name: &str) -> NonceRecord {
        NonceRecord::decode(&std::fs::read(root.join(name)).unwrap()).unwrap()
    }

    #[test]
    fn durable_signers_burn_bound_transcripts_before_returning_valid_shares() {
        let (keys, public) = trusted_dealer_keygen(3, 2, ack()).unwrap();
        let roots = [journal_dir("round-a"), journal_dir("round-b")];
        let mut a = reopen_when_free(&roots[0], keys[0].clone());
        let mut b = reopen_when_free(&roots[1], keys[1].clone());
        let (na, ca) = a.commit().unwrap();
        let (nb, cb) = b.commit().unwrap();
        let names = [na.record.name(), nb.record.name()];
        assert!(!disk_record(&roots[0], &names[0]).burned);
        let package =
            SigningPackage::new(2, b"durable signing transcript".to_vec(), vec![ca, cb]).unwrap();
        let mut responses = BTreeMap::new();
        responses.insert(keys[0].index, a.sign(na, &package).unwrap());
        responses.insert(keys[1].index, b.sign(nb, &package).unwrap());
        let signature = aggregate(&package, &responses, &public).unwrap();
        assert!(verify(
            &signature,
            &package.message,
            public.group_public_key
        ));
        for (root, name) in roots.iter().zip(&names) {
            let record = disk_record(root, name);
            assert!(record.burned);
            assert_eq!(record.transcript, package.transcript_digest());
        }
        drop(a);
        drop(b);
        for (root, key) in roots.iter().zip(&keys) {
            drop(reopen_when_free(root, key.clone()));
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn reopen_burns_abandoned_round_and_refuses_prior_instance_handle() {
        let (keys, _) = trusted_dealer_keygen(1, 1, ack()).unwrap();
        let root = journal_dir("restart");
        let mut signer = reopen_when_free(&root, keys[0].clone());
        let (nonces, commitment) = signer.commit().unwrap();
        let name = nonces.record.name();
        drop(signer);
        let mut reopened = reopen_when_free(&root, keys[0].clone());
        assert!(disk_record(&root, &name).burned);
        let package =
            SigningPackage::new(1, b"attempt after restart".to_vec(), vec![commitment]).unwrap();
        assert!(reopened.sign(nonces, &package).is_err());
        let (next, _) = reopened.commit().unwrap();
        assert_ne!(next.record.name(), name);
        drop(next);
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_package_and_interruption_after_burn_never_leave_reusable_reservation() {
        let (keys, _) = trusted_dealer_keygen(1, 1, ack()).unwrap();
        for after_burn in [false, true] {
            let root = journal_dir("burn-errors");
            let mut signer = reopen_when_free(&root, keys[0].clone());
            let (nonces, commitment) = signer.commit().unwrap();
            let name = nonces.record.name();
            let (_, foreign_commitment) = round1_commit(1).unwrap();
            let package = SigningPackage::new(
                1,
                b"burn despite failure".to_vec(),
                vec![if after_burn {
                    commitment
                } else {
                    foreign_commitment
                }],
            )
            .unwrap();
            signer.fail_after_burn = after_burn;
            assert!(signer.sign(nonces, &package).is_err());
            assert!(disk_record(&root, &name).burned);
            if after_burn {
                assert!(signer.commit().is_err());
            }
            drop(signer);
            drop(reopen_when_free(&root, keys[0].clone()));
            assert!(disk_record(&root, &name).burned);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn failed_reservation_or_burn_poison_the_service_until_recovery() {
        let (keys, _) = trusted_dealer_keygen(1, 1, ack()).unwrap();
        let root = journal_dir("failed-write");
        let mut signer = reopen_when_free(&root, keys[0].clone());
        let (nonces, commitment) = signer.commit().unwrap();
        let path = root.join(nonces.record.name());
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let package =
            SigningPackage::new(1, b"storage failure".to_vec(), vec![commitment]).unwrap();
        assert!(signer.sign(nonces, &package).is_err());
        assert!(signer.commit().is_err());
        drop(signer);
        assert!(DurableFrostSigner::open(&root, keys[0].clone()).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_binds_verification_share_and_all_reads_are_bounded() {
        let (keys, _) = trusted_dealer_keygen(1, 1, ack()).unwrap();
        let root = journal_dir("manifest-binding");
        drop(reopen_when_free(&root, keys[0].clone()));
        let mut changed = keys[0].clone();
        changed.secret_share += Scalar::ONE;
        assert!(DurableFrostSigner::open(&root, changed).is_err());
        std::fs::File::create(root.join("signer.binding"))
            .unwrap()
            .set_len(1024 * 1024)
            .unwrap();
        assert!(DurableFrostSigner::open(&root, keys[0].clone()).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn modified_complete_record_and_runtime_deleted_manifest_fail_closed() {
        let (keys, _) = trusted_dealer_keygen(1, 1, ack()).unwrap();
        let root = journal_dir("corruption");
        let mut signer = reopen_when_free(&root, keys[0].clone());
        let (nonces, _) = signer.commit().unwrap();
        let path = root.join(nonces.record.name());
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[NONCE_DOMAIN.len() + 32] ^= 1;
        std::fs::write(&path, bytes).unwrap();
        drop(nonces);
        drop(signer);
        assert!(DurableFrostSigner::open(&root, keys[0].clone()).is_err());
        std::fs::remove_dir_all(&root).unwrap();
        let mut signer = reopen_when_free(&root, keys[0].clone());
        std::fs::remove_file(root.join("signer.binding")).unwrap();
        assert!(signer.commit().is_err());
        drop(signer);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn signer_and_key_debug_never_print_the_secret_share() {
        let (keys, _) = trusted_dealer_keygen(1, 1, ack()).unwrap();
        let secret = format!("{:?}", keys[0].secret_share);
        assert!(!format!("{:?}", keys[0]).contains(&secret));
        let root = journal_dir("debug");
        let signer = reopen_when_free(&root, keys[0].clone());
        assert!(!format!("{signer:?}").contains(&secret));
        drop(signer);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn separate_process_cannot_open_an_active_signer() {
        const ENV: &str = "MINI_FROST_LOCK_CHILD";
        if let Some(root) = std::env::var_os(ENV) {
            let (keys, _) = trusted_dealer_keygen(1, 1, ack()).unwrap();
            let error = DurableFrostSigner::open(std::path::PathBuf::from(root), keys[0].clone())
                .unwrap_err();
            // A key mismatch would mean the child got past the exclusive lock.
            assert!(!error.to_string().contains("identity/share"));
            return;
        }
        let root = journal_dir("process-lock");
        let (keys, _) = trusted_dealer_keygen(1, 1, ack()).unwrap();
        let signer = reopen_when_free(&root, keys[0].clone());
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "frost_sign::tests::separate_process_cannot_open_an_active_signer",
            ])
            .env(ENV, &root)
            .status()
            .unwrap();
        assert!(status.success());
        drop(signer);
        std::fs::remove_dir_all(root).unwrap();
    }
}
