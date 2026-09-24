//! Encrypted, crash-consistent sender allowance and retransmission outbox.
//!
//! One owner-controlled directory per permit, one stable lock across processes.
//! The owner supplies a dedicated wallet storage key; it is never written here.
//! Restoring a directory backup or copying it to another device is NOT prevented.
//! Canonical reconciliation is still required. A missing/corrupt existing journal
//! must not be replaced with a new one while the authorization remains usable.
use crate::credit_permit::*;
use mini_crypto::{AeadKey, AeadNonce};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const DOMAIN: &[u8] = b"mininet/credit-journal/v1";
const MAX_COUPONS: usize = 4096;
const MAX_BYTES: u64 = 1024 * 1024;

pub struct FileCreditJournal {
    directory: PathBuf,
    permit: VerifiedCreditPermit,
    key: AeadKey,
}
impl core::fmt::Debug for FileCreditJournal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("FileCreditJournal(<private>)")
    }
}
impl FileCreditJournal {
    /// Only for a newly admitted permit, never recovery. Refuses an existing file.
    pub fn create(
        directory: &Path,
        permit: VerifiedCreditPermit,
        key: AeadKey,
    ) -> Result<Self, CreditSignatureError> {
        mini_durable::create_dir_all(directory)
            .map_err(|_| CreditSignatureError::JournalFailure)?;
        let journal = Self {
            directory: directory.into(),
            permit,
            key,
        };
        let _lock = journal.lock()?;
        match fs::symlink_metadata(journal.path()) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err(CreditSignatureError::JournalFailure),
        }
        // A missing data file must never silently turn a used permit into fresh
        // allowance. Burn initialization before writing; partial creation fails closed.
        let marker = journal.directory.join("initialized");
        let marker_file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
            .map_err(|_| CreditSignatureError::JournalFailure)?;
        marker_file
            .sync_all()
            .map_err(|_| CreditSignatureError::JournalFailure)?;
        mini_durable::sync_parent(&marker).map_err(|_| CreditSignatureError::JournalFailure)?;
        journal.write(&[])?;
        Ok(journal)
    }
    /// Recovery fails closed if state is missing, corrupt, or belongs to another permit.
    pub fn open(
        directory: &Path,
        permit: VerifiedCreditPermit,
        key: AeadKey,
    ) -> Result<Self, CreditSignatureError> {
        let journal = Self {
            directory: directory.into(),
            permit,
            key,
        };
        let _lock = journal.lock()?;
        journal.read()?;
        Ok(journal)
    }
    pub fn outbox(&self) -> Result<Vec<SignedCreditIou>, CreditSignatureError> {
        let _lock = self.lock()?;
        Ok(self.read()?.1)
    }
    fn path(&self) -> PathBuf {
        self.directory.join("credit.enc")
    }
    fn lock(&self) -> Result<fs::File, CreditSignatureError> {
        mini_durable::lock_exclusive(&self.directory.join("credit.lock"))
            .map_err(|_| CreditSignatureError::JournalFailure)
    }
    fn aad(&self) -> Vec<u8> {
        let mut bytes = DOMAIN.to_vec();
        bytes.extend_from_slice(self.permit.id());
        bytes
    }
    fn read(&self) -> Result<(CreditUse, Vec<SignedCreditIou>), CreditSignatureError> {
        let fail = || CreditSignatureError::JournalFailure;
        let mut bytes = Vec::new();
        fs::File::open(self.path())
            .map_err(|_| fail())?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| fail())?;
        if bytes.len() < 28 || bytes.len() as u64 > MAX_BYTES {
            return Err(fail());
        }
        let nonce = AeadNonce::from_bytes(&bytes[..12]).map_err(|_| fail())?;
        let plain = self
            .key
            .decrypt(&nonce, &bytes[12..], &self.aad())
            .map_err(|_| fail())?;
        let mut at = 0;
        let mut coupons = Vec::new();
        let mut usage = CreditUse::default();
        while at < plain.len() {
            if plain.len() - at < 4 || coupons.len() >= MAX_COUPONS {
                return Err(fail());
            }
            let len =
                u32::from_be_bytes(plain[at..at + 4].try_into().map_err(|_| fail())?) as usize;
            at += 4;
            if len > plain.len() - at {
                return Err(fail());
            }
            let coupon = SignedCreditIou::decode(&plain[at..at + len]).map_err(|_| fail())?;
            verify_credit_iou(&coupon, &self.permit, self.permit.terms().issued_height)
                .map_err(|_| fail())?;
            if coupon.terms.sequence != usage.next_sequence {
                return Err(fail());
            }
            usage.signed_micro = usage
                .signed_micro
                .checked_add(coupon.terms.amount_micro)
                .ok_or_else(fail)?;
            if usage.signed_micro > self.permit.terms().limit_micro {
                return Err(fail());
            }
            usage.next_sequence += 1;
            coupons.push(coupon);
            at += len;
        }
        Ok((usage, coupons))
    }
    fn write(&self, coupons: &[SignedCreditIou]) -> Result<(), CreditSignatureError> {
        let mut plain = Vec::new();
        for coupon in coupons {
            let bytes = coupon.encode()?;
            plain.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            plain.extend_from_slice(&bytes);
        }
        let nonce = AeadNonce::generate().map_err(|_| CreditSignatureError::JournalFailure)?;
        let mut bytes = nonce.to_bytes().to_vec();
        bytes.extend(
            self.key
                .encrypt(&nonce, &plain, &self.aad())
                .map_err(|_| CreditSignatureError::JournalFailure)?,
        );
        mini_durable::atomic_replace(&self.path(), &bytes)
            .map_err(|_| CreditSignatureError::JournalFailure)
    }
}
impl CreditUseJournal for FileCreditJournal {
    fn load(&self, permit_id: &[u8; 32]) -> Result<CreditUse, CreditSignatureError> {
        if permit_id != self.permit.id() {
            return Err(CreditSignatureError::WrongPermit);
        }
        let _lock = self.lock()?;
        Ok(self.read()?.0)
    }
    fn compare_and_record(
        &mut self,
        expected: CreditUse,
        next: CreditUse,
        coupon: &SignedCreditIou,
    ) -> Result<bool, CreditSignatureError> {
        let _lock = self.lock()?;
        let (usage, mut coupons) = self.read()?;
        if usage != expected {
            return Ok(false);
        }
        if coupons.len() >= MAX_COUPONS {
            return Err(CreditSignatureError::JournalFailure);
        }
        verify_credit_iou(coupon, &self.permit, self.permit.terms().issued_height)?;
        if coupon.terms.sequence != usage.next_sequence
            || usage.next_sequence.checked_add(1) != Some(next.next_sequence)
            || usage.signed_micro.checked_add(coupon.terms.amount_micro) != Some(next.signed_micro)
            || next.signed_micro > self.permit.terms().limit_micro
        {
            return Err(CreditSignatureError::JournalFailure);
        }
        coupons.push(coupon.clone());
        self.write(&coupons)?;
        Ok(true)
    }
}
