//! Batch 3 (D-0066, `docs/design/self-hosted-forge-spine.md`): the pieces
//! the design doc named as missing from `lib.rs`'s existing release
//! registry relative to TUF's role separation -- rollback protection and a
//! release transparency log. (Metadata/freshness expiry and independent-
//! builder-quorum tightening are `mini-update`'s job, one layer up, since
//! both need device-local state or a second crate this one must not depend
//! on.) Adapted to Mininet's identity-root/governance model rather than
//! TUF's PKI role separation, per Directive 14: reuse the existing
//! object/index machinery instead of inventing a parallel metadata format.

use crate::{parse_release_payload, ForgeError, Result};
use mini_objects::{Object, ObjectId, ObjectType};
use mini_store::{Backend, Store};

/// A dotted-numeric version (`"1.2.3"`), comparable so rollback can be
/// detected mechanically rather than by trusting a free-form string's
/// lexical order (`"9.0.0" < "10.0.0"` lexically, which would silently
/// defeat a naive string-based check).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(Vec<u32>);

/// Maximum dotted components a version string may have (`"1.2.3.4"` is the
/// most any real release scheme here needs; unbounded components would be
/// a hostile-input amplification vector).
pub const MAX_VERSION_COMPONENTS: usize = 8;

impl Version {
    /// Parse a dotted-numeric version string. Every component must be a
    /// valid `u32`; empty strings, empty components (`"1..2"`), leading/
    /// trailing dots, and non-numeric components are all rejected --
    /// strict on purpose, since a version string that fails to parse
    /// several different ways is exactly the kind of ambiguity a rollback
    /// check cannot afford to guess through.
    pub fn parse(s: &str) -> Result<Self> {
        if s.is_empty() {
            return Err(ForgeError::BadVersion);
        }
        let parts: Vec<&str> = s.split('.').collect();
        if parts.is_empty() || parts.len() > MAX_VERSION_COMPONENTS {
            return Err(ForgeError::BadVersion);
        }
        let mut components = Vec::with_capacity(parts.len());
        for part in parts {
            if part.is_empty() || (part.len() > 1 && part.starts_with('0')) {
                return Err(ForgeError::BadVersion); // no empty or leading-zero components
            }
            let n: u32 = part.parse().map_err(|_| ForgeError::BadVersion)?;
            components.push(n);
        }
        Ok(Version(components))
    }
}

/// Refuse adoption of `candidate` if it is not strictly greater than
/// `running` -- rollback/downgrade protection. `running = None` means the
/// device has nothing adopted yet, so there is nothing to roll back from.
///
/// Comparison is component-wise after padding the shorter version with
/// trailing zeros (`"1.2" > "1.1.9"` compares as `[1,2,0] > [1,1,9]`,
/// which is correct; without padding, `Vec`'s lexicographic `Ord` would
/// incorrectly rank `[1,2]` below `[1,2,0]`).
pub fn check_no_rollback(running: Option<&Version>, candidate: &Version) -> Result<()> {
    let Some(running) = running else {
        return Ok(());
    };
    let len = running.0.len().max(candidate.0.len());
    let padded = |v: &Version| -> Vec<u32> {
        let mut out = v.0.clone();
        out.resize(len, 0);
        out
    };
    if padded(candidate) <= padded(running) {
        return Err(ForgeError::RollbackRejected);
    }
    Ok(())
}

/// Fetch and parse a stored release object's version.
pub fn release_version<B: Backend>(store: &Store<B>, release_id: &ObjectId) -> Result<Version> {
    let rel = store.get(release_id)?;
    if rel.object_type != ObjectType::RELEASE {
        return Err(ForgeError::BadObject);
    }
    let (version, _branch, _artifact, _recipe) = parse_release_payload(&rel)?;
    Version::parse(&version)
}

/// Every `RELEASE` object this store has ever seen claiming `project_id`/
/// `branch` -- the store's own append-only, content-addressed nature
/// already makes this a transparency log; this is the missing query
/// surface over it (TUF's "snapshot" role, minus a separate signed
/// snapshot metadata format -- the object store itself is the snapshot).
/// Ordered by `(timestamp_ms, sequence)` so callers get a stable,
/// author-claimed chronological view (an ordering hint, not a proof --
/// same caveat `Object::timestamp_ms` itself carries).
pub fn list_releases<B: Backend>(
    store: &Store<B>,
    project_id: &ObjectId,
    branch: &str,
) -> Result<Vec<Object>> {
    let mut out = Vec::new();
    for id in store.by_type(&ObjectType::RELEASE)? {
        let rel = store.get(&id)?;
        let claims_project = rel
            .links
            .iter()
            .any(|l| l.rel == "project" && &l.target == project_id);
        if !claims_project {
            continue;
        }
        let Ok((_, claimed_branch, _, _)) = parse_release_payload(&rel) else {
            continue; // malformed release objects are skipped, not fatal to the log
        };
        if claimed_branch != branch {
            continue;
        }
        out.push(rel);
    }
    out.sort_by_key(|r| (r.timestamp_ms, r.sequence));
    Ok(out)
}

/// Strict transparency-log query. Unlike [`list_releases`], this refuses a
/// malformed `RELEASE` object that claims the requested project, so observers
/// cannot silently construct different logs by ignoring malformed entries.
pub fn list_releases_strict<B: Backend>(
    store: &Store<B>,
    project_id: &ObjectId,
    branch: &str,
) -> Result<Vec<Object>> {
    let mut out = Vec::new();
    for id in store.by_type(&ObjectType::RELEASE)? {
        let rel = store.get(&id)?;
        let claims_project = rel
            .links
            .iter()
            .any(|l| l.rel == "project" && &l.target == project_id);
        if !claims_project {
            continue;
        }
        let Ok((_, claimed_branch, _, _)) = parse_release_payload(&rel) else {
            return Err(ForgeError::BadObject);
        };
        if claimed_branch == branch {
            out.push(rel);
        }
    }
    out.sort_by_key(|r| (r.timestamp_ms, r.sequence));
    Ok(out)
}

/// Two `RELEASE` objects for the same project/branch that claim the same
/// version but disagree on the artifact digest -- evidence of
/// equivocation: a publisher (or an attacker who obtained a signing key)
/// showed different builds to different observers under an identical
/// version label. This is exactly the attack a Certificate-Transparency-
/// style log is meant to make detectable: no single observer's view is
/// trusted as complete, but two observers comparing logs will disagree
/// about what "version 1.2.3" was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Equivocation {
    pub version: String,
    pub first: ObjectId,
    pub second: ObjectId,
    pub first_artifact_digest: [u8; 32],
    pub second_artifact_digest: [u8; 32],
}

/// Scan every release this store has seen for `project_id`/`branch` and
/// report every equivocating pair. `O(n^2)` in the number of releases for
/// one project/branch, which is fine -- release counts are small (tens to
/// low hundreds over a project's life), not the kind of scale this would
/// need to be sub-quadratic for.
pub fn detect_equivocation<B: Backend>(
    store: &Store<B>,
    project_id: &ObjectId,
    branch: &str,
) -> Result<Vec<Equivocation>> {
    let releases = list_releases(store, project_id, branch)?;
    let mut parsed: Vec<(ObjectId, String, [u8; 32])> = Vec::with_capacity(releases.len());
    for rel in &releases {
        if let Ok((version, _, artifact_digest, _)) = parse_release_payload(rel) {
            parsed.push((rel.id().clone(), version, artifact_digest));
        }
    }
    let mut found = Vec::new();
    for i in 0..parsed.len() {
        for j in (i + 1)..parsed.len() {
            let (id_a, version_a, digest_a) = &parsed[i];
            let (id_b, version_b, digest_b) = &parsed[j];
            if version_a == version_b && digest_a != digest_b {
                found.push(Equivocation {
                    version: version_a.clone(),
                    first: id_a.clone(),
                    second: id_b.clone(),
                    first_artifact_digest: *digest_a,
                    second_artifact_digest: *digest_b,
                });
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse_and_compare_correctly() {
        assert!(Version::parse("1.2.3").unwrap() < Version::parse("1.2.4").unwrap());
        assert!(Version::parse("1.9.0").unwrap() < Version::parse("1.10.0").unwrap());
        assert!(Version::parse("9.0.0").unwrap() < Version::parse("10.0.0").unwrap());
        assert_eq!(
            Version::parse("1.2.3").unwrap(),
            Version::parse("1.2.3").unwrap()
        );
    }

    #[test]
    fn shorter_and_longer_versions_compare_by_zero_padding() {
        assert!(Version::parse("1.2").unwrap() > Version::parse("1.1.9").unwrap());
        assert_eq!(Version::parse("1.2").unwrap().0, vec![1, 2]);
        // 1.2 == 1.2.0 in comparison terms, via padding, though they are
        // not `Eq` (different internal lengths) -- check_no_rollback uses
        // the padded comparison, not raw equality.
        let running = Version::parse("1.2").unwrap();
        let candidate = Version::parse("1.2.0").unwrap();
        assert!(check_no_rollback(Some(&running), &candidate).is_err());
    }

    #[test]
    fn malformed_version_strings_are_rejected() {
        assert!(Version::parse("").is_err());
        assert!(Version::parse("1..2").is_err());
        assert!(Version::parse(".1.2").is_err());
        assert!(Version::parse("1.2.").is_err());
        assert!(Version::parse("1.a.2").is_err());
        assert!(Version::parse("01.2.3").is_err());
        assert!(Version::parse(&"1.".repeat(20)).is_err());
    }

    #[test]
    fn first_adoption_has_nothing_to_roll_back_from() {
        let candidate = Version::parse("1.0.0").unwrap();
        assert!(check_no_rollback(None, &candidate).is_ok());
    }

    #[test]
    fn a_downgrade_is_rejected() {
        let running = Version::parse("2.0.0").unwrap();
        let candidate = Version::parse("1.9.9").unwrap();
        assert!(matches!(
            check_no_rollback(Some(&running), &candidate),
            Err(ForgeError::RollbackRejected)
        ));
    }

    #[test]
    fn an_identical_version_is_rejected_as_a_non_upgrade() {
        let running = Version::parse("2.0.0").unwrap();
        let candidate = Version::parse("2.0.0").unwrap();
        assert!(check_no_rollback(Some(&running), &candidate).is_err());
    }

    #[test]
    fn a_genuine_upgrade_is_accepted() {
        let running = Version::parse("2.0.0").unwrap();
        let candidate = Version::parse("2.0.1").unwrap();
        assert!(check_no_rollback(Some(&running), &candidate).is_ok());
    }
}
