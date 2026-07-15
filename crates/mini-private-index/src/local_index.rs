//! [`LocalIndex`]: a local, in-memory store of [`PrivateIndexRecord`]s
//! keyed by lookup label. This is the "Phase 1: local-only primitive"
//! this crate implements — a real, replicated *network* private index
//! (research report §"replicated private index", §"role-separated query
//! path") is explicitly future work; this type only proves the
//! signature/rollback/label discipline a networked version would need to
//! enforce per-replica.

use std::collections::HashMap;

use did_mini::Kel;

use crate::error::{IndexError, Result};
use crate::label::LookupLabel;
use crate::record::PrivateIndexRecord;

/// A local map from lookup label to the newest valid record seen for it.
#[derive(Debug, Default)]
pub struct LocalIndex {
    records: HashMap<[u8; 32], PrivateIndexRecord>,
}

impl LocalIndex {
    pub fn new() -> Self {
        LocalIndex {
            records: HashMap::new(),
        }
    }

    /// Write `record`, enforcing:
    /// 1. `record.verify(writer_kel, now_ms)` passes.
    /// 2. If a record already exists at this label, `record.writer` must
    ///    match it exactly ([`IndexError::WriterMismatch`] otherwise) —
    ///    one writer cannot hijack another's label.
    /// 3. `record.sequence` must strictly exceed the stored record's
    ///    sequence ([`IndexError::RollbackRejected`] otherwise) — refuses
    ///    replays and rollbacks of an older record over a newer one.
    pub fn write(
        &mut self,
        record: PrivateIndexRecord,
        writer_kel: &Kel,
        now_ms: u64,
    ) -> Result<()> {
        record.verify(writer_kel, now_ms)?;
        let key = record.lookup_label.to_bytes();
        if let Some(existing) = self.records.get(&key) {
            if existing.writer.as_str() != record.writer.as_str() {
                return Err(IndexError::WriterMismatch);
            }
            if record.sequence <= existing.sequence {
                return Err(IndexError::RollbackRejected);
            }
        }
        self.records.insert(key, record);
        Ok(())
    }

    /// Look up the record at `label`. Returns `None` for both a missing
    /// label and an expired one — a caller cannot distinguish "nothing
    /// was ever published here" from "something was published here and
    /// has since expired" through this API, matching the research
    /// report's negative-lookup-indistinguishability goal at the local
    /// layer (§"negative lookups").
    pub fn lookup(&self, label: LookupLabel, now_ms: u64) -> Option<&PrivateIndexRecord> {
        let record = self.records.get(&label.to_bytes())?;
        if now_ms >= record.expires_at_ms {
            return None;
        }
        Some(record)
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;

    use crate::label::IndexEpoch;
    use crate::record::RecordSizeClass;

    fn label(byte: u8) -> LookupLabel {
        LookupLabel::from_bytes([byte; 32])
    }

    fn record(
        writer: &Controller,
        label: LookupLabel,
        sequence: u64,
        expires_at_ms: u64,
    ) -> PrivateIndexRecord {
        PrivateIndexRecord::issue(
            writer,
            IndexEpoch(1),
            label,
            RecordSizeClass::Small,
            b"ciphertext".to_vec(),
            expires_at_ms,
            sequence,
        )
        .unwrap()
    }

    #[test]
    fn a_freshly_written_record_is_found_by_lookup() {
        let writer = Controller::incept_single().unwrap();
        let mut index = LocalIndex::new();
        let r = record(&writer, label(1), 1, 2_000);
        index.write(r.clone(), &writer.kel(), 1_000).unwrap();
        assert_eq!(index.lookup(label(1), 1_500), Some(&r));
    }

    #[test]
    fn a_missing_label_looks_up_as_none() {
        let index = LocalIndex::new();
        assert_eq!(index.lookup(label(1), 1_000), None);
    }

    #[test]
    fn an_expired_record_looks_up_as_none_indistinguishably_from_missing() {
        let writer = Controller::incept_single().unwrap();
        let mut index = LocalIndex::new();
        let r = record(&writer, label(1), 1, 2_000);
        index.write(r, &writer.kel(), 1_000).unwrap();
        assert_eq!(index.lookup(label(1), 2_000), None);
        assert_eq!(index.lookup(label(9), 2_000), None);
    }

    #[test]
    fn writing_an_unverifiable_record_is_rejected() {
        let writer = Controller::incept_single().unwrap();
        let other = Controller::incept_single().unwrap();
        let mut index = LocalIndex::new();
        let r = record(&writer, label(1), 1, 2_000);
        assert_eq!(
            index.write(r, &other.kel(), 1_000),
            Err(IndexError::BadSignature)
        );
    }

    #[test]
    fn writing_an_already_expired_record_is_rejected() {
        let writer = Controller::incept_single().unwrap();
        let mut index = LocalIndex::new();
        let r = record(&writer, label(1), 1, 1_000);
        assert_eq!(
            index.write(r, &writer.kel(), 1_000),
            Err(IndexError::Expired)
        );
    }

    #[test]
    fn a_higher_sequence_from_the_same_writer_replaces_the_stored_record() {
        let writer = Controller::incept_single().unwrap();
        let mut index = LocalIndex::new();
        index
            .write(record(&writer, label(1), 1, 2_000), &writer.kel(), 1_000)
            .unwrap();
        let newer = record(&writer, label(1), 2, 3_000);
        index.write(newer.clone(), &writer.kel(), 1_000).unwrap();
        assert_eq!(index.lookup(label(1), 1_500), Some(&newer));
    }

    #[test]
    fn a_lower_or_equal_sequence_is_rejected_as_a_rollback() {
        let writer = Controller::incept_single().unwrap();
        let mut index = LocalIndex::new();
        index
            .write(record(&writer, label(1), 5, 2_000), &writer.kel(), 1_000)
            .unwrap();
        assert_eq!(
            index.write(record(&writer, label(1), 5, 2_000), &writer.kel(), 1_000),
            Err(IndexError::RollbackRejected)
        );
        assert_eq!(
            index.write(record(&writer, label(1), 3, 2_000), &writer.kel(), 1_000),
            Err(IndexError::RollbackRejected)
        );
    }

    #[test]
    fn a_different_writer_cannot_hijack_an_existing_label() {
        let writer = Controller::incept_single().unwrap();
        let attacker = Controller::incept_single().unwrap();
        let mut index = LocalIndex::new();
        index
            .write(record(&writer, label(1), 1, 2_000), &writer.kel(), 1_000)
            .unwrap();
        let hijack = record(&attacker, label(1), 2, 2_000);
        assert_eq!(
            index.write(hijack, &attacker.kel(), 1_000),
            Err(IndexError::WriterMismatch)
        );
    }

    #[test]
    fn distinct_labels_never_collide_in_the_same_index() {
        let writer = Controller::incept_single().unwrap();
        let mut index = LocalIndex::new();
        let a = record(&writer, label(1), 1, 2_000);
        let b = record(&writer, label(2), 1, 2_000);
        index.write(a.clone(), &writer.kel(), 1_000).unwrap();
        index.write(b.clone(), &writer.kel(), 1_000).unwrap();
        assert_eq!(index.len(), 2);
        assert_eq!(index.lookup(label(1), 1_500), Some(&a));
        assert_eq!(index.lookup(label(2), 1_500), Some(&b));
    }

    #[test]
    fn an_empty_index_reports_empty_and_zero_len() {
        let index = LocalIndex::new();
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }
}
