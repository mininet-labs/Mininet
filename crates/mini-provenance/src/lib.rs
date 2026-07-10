//! SLSA/in-toto-style build provenance as real, signed, content-addressed
//! objects — self-hosted forge spine Batch 2a (D-0068,
//! `docs/design/self-hosted-forge-spine.md`).
//!
//! ## What this closes
//!
//! The founder-adopted external audit named a specific, real gap: this
//! tree's CI runs a same-runner clean-rebuild comparison and its own
//! workflow already says so honestly, but nothing turns "builder X got
//! digest D" into a *queryable, signed, independently-countable* claim the
//! way `mini_forge::release`'s artifact attestations already are for a
//! *cut* release. This crate generalizes that exact pattern — "N distinct
//! verified identity roots agree" — to the build stage, before a release
//! is even proposed: [`record_provenance`] signs a builder's environment
//! digest, the exact commands run (as a digest, not raw logs), every
//! output digest produced, whether networking was enabled, and a
//! self-declared reproducibility group; [`independent_agreement`] counts
//! how many *distinct* identity roots — excluding the subject's own author,
//! the same exclusion the audit specifically asked for ("do not count...
//! the release author's own build") — agree on a given output digest.
//!
//! ## Honest limit
//!
//! Code can verify *distinct identity roots* agree. It cannot verify
//! *administratively independent infrastructure* — three containers on one
//! host, signed by three keys the same person controls, are
//! indistinguishable from three real builders to anything in this crate.
//! That is a policy/process fact about who controls which signing key, the
//! same caveat `mini_forge::release`'s own docs already carry for release
//! attestations, unchanged here.
//!
//! ## What this does not do
//!
//! Nothing here *runs* a build. Sandboxed execution (WASI/Wasmtime, Batch
//! 2b) is a separate, deliberately deferred decision — see
//! `docs/design/self-hosted-forge-spine.md`. This crate only makes the
//! *result* of a build (wherever and however it ran) into a real,
//! verifiable, independently-countable claim.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;

pub use error::{ProvenanceError, Result};

use did_mini::Did;
use mini_forge::IdentityOracle;
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};

/// Build-provenance object type.
pub const PROVENANCE_TYPE: &str = "mini/build-provenance";
/// Maximum bytes for a self-declared reproducibility group label.
pub const MAX_GROUP_BYTES: usize = 128;
/// Maximum output digests a single provenance record may claim (hostile-
/// input bound, matching every other crate's list-length caps in this
/// tree).
pub const MAX_OUTPUTS: usize = 1024;

/// One builder's claim about how a subject (a commit or artifact
/// `ObjectId`) was built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildProvenance {
    /// Digest identifying the builder's environment (OS/toolchain/image).
    pub environment_digest: [u8; 32],
    /// Digest of the exact commands/pipeline recipe run -- not the raw
    /// logs, so this stays small and comparable byte-for-byte.
    pub commands_digest: [u8; 32],
    /// Every artifact digest this build produced.
    pub output_digests: Vec<[u8; 32]>,
    /// Self-declared label grouping builds meant to be bit-comparable
    /// (e.g. `"linux-x86_64-rustc1.83"`). Not verified against anything --
    /// a hint for humans and tooling deciding which builds "should" agree,
    /// never load-bearing for [`independent_agreement`]'s count itself.
    pub reproducibility_group: String,
    /// Whether this build had network access -- a build that reached the
    /// network is weaker evidence of reproducibility than one that
    /// didn't, and callers may want to weight or filter on this.
    pub network_enabled: bool,
    /// When the build started, in milliseconds (author-claimed, an
    /// ordering hint like every other timestamp in this tree, not proof).
    pub started_ms: u64,
    /// When the build finished.
    pub finished_ms: u64,
}

impl BuildProvenance {
    fn validate(&self) -> Result<()> {
        if self.output_digests.is_empty() || self.output_digests.len() > MAX_OUTPUTS {
            return Err(ProvenanceError::NoOutputs);
        }
        if self.reproducibility_group.is_empty()
            || self.reproducibility_group.len() > MAX_GROUP_BYTES
        {
            return Err(ProvenanceError::BadGroup);
        }
        if self.finished_ms < self.started_ms {
            return Err(ProvenanceError::BadTimeRange);
        }
        Ok(())
    }

    fn encode(&self) -> Vec<u8> {
        let mut w = Vec::new();
        w.extend_from_slice(&self.environment_digest);
        w.extend_from_slice(&self.commands_digest);
        w.extend_from_slice(&(self.output_digests.len() as u32).to_be_bytes());
        for d in &self.output_digests {
            w.extend_from_slice(d);
        }
        put_str(&mut w, &self.reproducibility_group);
        w.push(u8::from(self.network_enabled));
        w.extend_from_slice(&self.started_ms.to_be_bytes());
        w.extend_from_slice(&self.finished_ms.to_be_bytes());
        w
    }

    fn decode(b: &[u8]) -> Option<Self> {
        let mut off = 0usize;
        let environment_digest = take_digest(b, &mut off)?;
        let commands_digest = take_digest(b, &mut off)?;
        if off + 4 > b.len() {
            return None;
        }
        let n = u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]) as usize;
        off += 4;
        if n == 0 || n > MAX_OUTPUTS {
            return None;
        }
        let mut output_digests = Vec::with_capacity(n);
        for _ in 0..n {
            output_digests.push(take_digest(b, &mut off)?);
        }
        let reproducibility_group = take_str(b, &mut off)?;
        if reproducibility_group.is_empty() || reproducibility_group.len() > MAX_GROUP_BYTES {
            return None;
        }
        let network_enabled = match b.get(off).copied()? {
            0 => false,
            1 => true,
            _ => return None,
        };
        off += 1;
        if off + 16 > b.len() {
            return None;
        }
        let started_ms = u64::from_be_bytes(b[off..off + 8].try_into().ok()?);
        off += 8;
        let finished_ms = u64::from_be_bytes(b[off..off + 8].try_into().ok()?);
        off += 8;
        if off != b.len() || finished_ms < started_ms {
            return None; // strict: no trailing bytes
        }
        Some(BuildProvenance {
            environment_digest,
            commands_digest,
            output_digests,
            reproducibility_group,
            network_enabled,
            started_ms,
            finished_ms,
        })
    }
}

/// Sign and store a [`BuildProvenance`] claim about `subject`.
#[allow(clippy::too_many_arguments)]
pub fn record_provenance<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &did_mini::Controller,
    subject: &ObjectId,
    provenance: &BuildProvenance,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    provenance.validate()?;
    let obj = ObjectBuilder::new(ObjectType::Custom(PROVENANCE_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(provenance.encode()))
        .link("subject", subject.clone())
        .sign(human, device)?;
    store.insert(&obj)?;
    Ok(obj)
}

/// One author-verified provenance record read back from the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceRecord {
    /// The identity root that signed this claim.
    pub builder: Did,
    pub provenance: BuildProvenance,
}

/// Every author-verified provenance record recorded against `subject`.
pub fn list_provenance<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
    subject: &ObjectId,
) -> Result<Vec<ProvenanceRecord>> {
    let mut out = Vec::new();
    for id in store.linking_to(subject)? {
        let obj = match store.get(&id) {
            Ok(o) => o,
            Err(_) => continue,
        };
        if obj.object_type != ObjectType::Custom(PROVENANCE_TYPE.to_string()) {
            continue;
        }
        if !author_verified(oracle, &obj) {
            continue;
        }
        let bytes = match &obj.payload {
            Payload::Public(b) => b,
            Payload::Encrypted(_) => continue,
        };
        let Some(provenance) = BuildProvenance::decode(bytes) else {
            continue;
        };
        out.push(ProvenanceRecord {
            builder: obj.author_human.clone(),
            provenance,
        });
    }
    Ok(out)
}

/// How many **distinct** identity roots — excluding `subject`'s own author,
/// if `subject` resolves — have a verified [`BuildProvenance`] record
/// against `subject` whose `output_digests` contains `expected_output`.
/// The exact generalization of `mini_forge::verify_release_artifact_only`'s
/// "N distinct verified identity roots agree, author excluded" pattern to
/// the build stage.
pub fn independent_agreement<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
    subject: &ObjectId,
    expected_output: [u8; 32],
) -> Result<u32> {
    let excluded_author = store
        .get(subject)
        .ok()
        .map(|s| s.author_human.as_str().to_string());
    let mut roots: Vec<String> = Vec::new();
    for record in list_provenance(store, oracle, subject)? {
        if !record.provenance.output_digests.contains(&expected_output) {
            continue;
        }
        let scid = record.builder.scid().to_string();
        if excluded_author.as_deref() == Some(record.builder.as_str()) {
            continue;
        }
        if !roots.contains(&scid) {
            roots.push(scid);
        }
    }
    Ok(roots.len() as u32)
}

fn author_verified(oracle: &dyn IdentityOracle, obj: &Object) -> bool {
    let Some(root) = oracle.kel(&obj.author_human) else {
        return false;
    };
    let Some(device) = oracle.kel(&obj.author_device) else {
        return false;
    };
    mini_objects::verify_provenance(obj, root, device).is_ok()
}

fn put_str(w: &mut Vec<u8>, s: &str) {
    w.extend_from_slice(&(s.len() as u32).to_be_bytes());
    w.extend_from_slice(s.as_bytes());
}

fn take_str(b: &[u8], off: &mut usize) -> Option<String> {
    if *off + 4 > b.len() {
        return None;
    }
    let len = u32::from_be_bytes([b[*off], b[*off + 1], b[*off + 2], b[*off + 3]]) as usize;
    *off += 4;
    if *off + len > b.len() || len > MAX_GROUP_BYTES {
        return None;
    }
    let s = String::from_utf8(b[*off..*off + len].to_vec()).ok()?;
    *off += len;
    Some(s)
}

fn take_digest(b: &[u8], off: &mut usize) -> Option<[u8; 32]> {
    if *off + 32 > b.len() {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&b[*off..*off + 32]);
    *off += 32;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::{Capabilities, Controller};
    use mini_forge::KelDirectory;
    use mini_store::MemoryBackend;

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

    fn oracle_of(identities: &[&Controller]) -> KelDirectory {
        let mut dir = KelDirectory::new();
        for c in identities {
            dir.insert(c.kel());
        }
        dir
    }

    fn a_subject<B: Backend>(store: &mut Store<B>, author: &Did, device: &Controller) -> ObjectId {
        let obj = ObjectBuilder::new(ObjectType::COMMIT)
            .payload(Payload::Public(b"a commit".to_vec()))
            .sign(author, device)
            .unwrap();
        store.insert(&obj).unwrap();
        obj.id().clone()
    }

    fn provenance(output: [u8; 32]) -> BuildProvenance {
        BuildProvenance {
            environment_digest: [1u8; 32],
            commands_digest: [2u8; 32],
            output_digests: vec![output],
            reproducibility_group: "linux-x86_64".to_string(),
            network_enabled: false,
            started_ms: 100,
            finished_ms: 200,
        }
    }

    #[test]
    fn no_outputs_is_rejected() {
        let (root, device) = identity(1);
        let mut store = Store::new(MemoryBackend::new());
        let subject = a_subject(&mut store, &root.did(), &device);
        let mut p = provenance([9u8; 32]);
        p.output_digests.clear();
        assert!(matches!(
            record_provenance(&mut store, &root.did(), &device, &subject, &p, 0, 1),
            Err(ProvenanceError::NoOutputs)
        ));
    }

    #[test]
    fn finished_before_started_is_rejected() {
        let (root, device) = identity(1);
        let mut store = Store::new(MemoryBackend::new());
        let subject = a_subject(&mut store, &root.did(), &device);
        let mut p = provenance([9u8; 32]);
        p.started_ms = 500;
        p.finished_ms = 100;
        assert!(matches!(
            record_provenance(&mut store, &root.did(), &device, &subject, &p, 0, 1),
            Err(ProvenanceError::BadTimeRange)
        ));
    }

    #[test]
    fn round_trips_through_the_store() {
        let (root, device) = identity(1);
        let (builder_root, builder_device) = identity(50);
        let mut store = Store::new(MemoryBackend::new());
        let subject = a_subject(&mut store, &root.did(), &device);
        let p = provenance([9u8; 32]);
        record_provenance(
            &mut store,
            &builder_root.did(),
            &builder_device,
            &subject,
            &p,
            1000,
            1,
        )
        .unwrap();

        let oracle = oracle_of(&[&root, &device, &builder_root, &builder_device]);
        let records = list_provenance(&store, &oracle, &subject).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].builder, builder_root.did());
        assert_eq!(records[0].provenance, p);
    }

    #[test]
    fn independent_builders_agreeing_are_counted_once_each() {
        let (root, device) = identity(1);
        let (b1_root, b1_device) = identity(50);
        let (b2_root, b2_device) = identity(90);
        let (b3_root, b3_device) = identity(130);
        let mut store = Store::new(MemoryBackend::new());
        let subject = a_subject(&mut store, &root.did(), &device);
        let output = [9u8; 32];

        for (r, d) in [
            (&b1_root, &b1_device),
            (&b2_root, &b2_device),
            (&b3_root, &b3_device),
        ] {
            record_provenance(
                &mut store,
                &r.did(),
                d,
                &subject,
                &provenance(output),
                1000,
                1,
            )
            .unwrap();
        }

        let oracle = oracle_of(&[
            &root, &device, &b1_root, &b1_device, &b2_root, &b2_device, &b3_root, &b3_device,
        ]);
        assert_eq!(
            independent_agreement(&store, &oracle, &subject, output).unwrap(),
            3
        );
    }

    #[test]
    fn the_subjects_own_author_building_it_themselves_does_not_count() {
        let (root, device) = identity(1);
        let mut store = Store::new(MemoryBackend::new());
        let subject = a_subject(&mut store, &root.did(), &device);
        let output = [9u8; 32];
        record_provenance(
            &mut store,
            &root.did(),
            &device,
            &subject,
            &provenance(output),
            1000,
            1,
        )
        .unwrap();

        let oracle = oracle_of(&[&root, &device]);
        assert_eq!(
            independent_agreement(&store, &oracle, &subject, output).unwrap(),
            0
        );
    }

    #[test]
    fn one_builder_signing_twice_still_counts_once() {
        let (root, device) = identity(1);
        let (mut builder_root, builder_device) = identity(50);
        let second_builder_device =
            Controller::incept_device_single_from_seeds(&builder_root.did(), &[60; 32], &[61; 32])
                .unwrap();
        builder_root
            .delegate_device(&second_builder_device.did(), Capabilities::primary())
            .unwrap();
        let mut store = Store::new(MemoryBackend::new());
        let subject = a_subject(&mut store, &root.did(), &device);
        let output = [9u8; 32];
        record_provenance(
            &mut store,
            &builder_root.did(),
            &builder_device,
            &subject,
            &provenance(output),
            1000,
            1,
        )
        .unwrap();
        record_provenance(
            &mut store,
            &builder_root.did(),
            &second_builder_device,
            &subject,
            &provenance(output),
            1001,
            1,
        )
        .unwrap();

        let oracle = oracle_of(&[
            &root,
            &device,
            &builder_root,
            &builder_device,
            &second_builder_device,
        ]);
        assert_eq!(
            independent_agreement(&store, &oracle, &subject, output).unwrap(),
            1
        );
    }

    #[test]
    fn disagreeing_output_digests_do_not_count_toward_a_different_expected_digest() {
        let (root, device) = identity(1);
        let (builder_root, builder_device) = identity(50);
        let mut store = Store::new(MemoryBackend::new());
        let subject = a_subject(&mut store, &root.did(), &device);
        record_provenance(
            &mut store,
            &builder_root.did(),
            &builder_device,
            &subject,
            &provenance([9u8; 32]),
            1000,
            1,
        )
        .unwrap();

        let oracle = oracle_of(&[&root, &device, &builder_root, &builder_device]);
        assert_eq!(
            independent_agreement(&store, &oracle, &subject, [7u8; 32]).unwrap(),
            0
        );
    }

    #[test]
    fn an_unvouched_builder_is_not_counted() {
        let (root, device) = identity(1);
        let (builder_root, builder_device) = identity(50);
        let mut store = Store::new(MemoryBackend::new());
        let subject = a_subject(&mut store, &root.did(), &device);
        let output = [9u8; 32];
        record_provenance(
            &mut store,
            &builder_root.did(),
            &builder_device,
            &subject,
            &provenance(output),
            1000,
            1,
        )
        .unwrap();

        // The oracle never learns the builder's KEL.
        let oracle = oracle_of(&[&root, &device]);
        assert_eq!(
            independent_agreement(&store, &oracle, &subject, output).unwrap(),
            0
        );
        assert!(list_provenance(&store, &oracle, &subject)
            .unwrap()
            .is_empty());
    }
}
