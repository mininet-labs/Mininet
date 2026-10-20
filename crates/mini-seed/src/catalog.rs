//! Owner-local signed discovery hints. This is not a canonical registry, a
//! trust list, proof of availability, or permission to dial a discovered host.
//! A snapshot is authenticated at replacement using caller-supplied current
//! KELs. Callers must independently maintain owner trust and recheck current
//! delegation/session authentication before using stored hints.
use crate::{
    id_valid, plain_path, read_bounded, Lease, Result, SeedError, MAX_OBJECT_BYTES,
    MAX_POOL_OBJECTS,
};
use did_mini::{Controller, Did, Kel};
use mini_objects::{verify_provenance, Object, ObjectBuilder, ObjectType, Payload};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

pub const ADVERTISEMENT_TYPE: &str = "mini/seed-advertisement-v1";
pub const MAX_ADVERTISEMENT_TTL_MS: u64 = 15 * 60 * 1000;
pub const MAX_CLOCK_SKEW_MS: u64 = 30 * 1000;
pub const MAX_PROVIDERS: usize = 64;
pub const MAX_ADVERTISEMENT_BYTES: u64 = 1024 * 1024;
pub const MAX_CATALOG_BYTES: u64 = 8 * 1024 * 1024;
const MAX_LEASE_MS: u64 = 3650 * 86_400_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Advertisement {
    pub provider: String,
    pub address: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub entries: Vec<Lease>,
}
fn validate(ad: &Advertisement, now_ms: Option<u64>) -> Result<()> {
    Did::parse(&ad.provider).map_err(|e| SeedError(e.to_string()))?;
    if ad.provider.len() > 256 || ad.address.len() > 128 || ad.entries.len() > MAX_POOL_OBJECTS {
        return Err(SeedError(
            "advertisement exceeds provider/address/entry bounds".into(),
        ));
    }
    let addr: SocketAddr = ad
        .address
        .parse()
        .map_err(|_| SeedError("advertisement address must be numeric IP:port".into()))?;
    if addr.port() == 0
        || addr.ip().is_unspecified()
        || addr.ip().is_multicast()
        || matches!(addr.ip(), std::net::IpAddr::V4(ip) if ip.is_broadcast())
        || addr.to_string() != ad.address
    {
        return Err(SeedError(
            "advertisement requires a canonical unicast numeric endpoint".into(),
        ));
    }
    if ad.expires_at_ms <= ad.issued_at_ms
        || ad
            .expires_at_ms
            .checked_sub(ad.issued_at_ms)
            .is_none_or(|d| d > MAX_ADVERTISEMENT_TTL_MS)
    {
        return Err(SeedError(
            "advertisement lifetime must be positive and at most 15 minutes".into(),
        ));
    }
    if let Some(now) = now_ms {
        if ad.issued_at_ms > now.saturating_add(MAX_CLOCK_SKEW_MS) || ad.expires_at_ms <= now {
            return Err(SeedError(
                "advertisement is expired or issued too far in the future".into(),
            ));
        }
    }
    let max_lease = ad
        .issued_at_ms
        .checked_add(MAX_LEASE_MS)
        .ok_or_else(|| SeedError("advertisement lease timestamp overflow".into()))?;
    let mut seen = BTreeSet::new();
    for lease in &ad.entries {
        id_valid(&lease.id)?;
        if !seen.insert(&lease.id)
            || lease.bytes == 0
            || lease.bytes > MAX_OBJECT_BYTES
            || lease.expires_at_ms <= ad.issued_at_ms
            || lease.expires_at_ms > max_lease
        {
            return Err(SeedError(
                "advertisement has duplicate or invalid object leases".into(),
            ));
        }
    }
    Ok(())
}
fn decode(object: &Object, now_ms: Option<u64>) -> Result<Advertisement> {
    object
        .verify_integrity(object.id())
        .map_err(|e| SeedError(e.to_string()))?;
    if object.object_type != ObjectType::Custom(ADVERTISEMENT_TYPE.into())
        || !object.links.is_empty()
    {
        return Err(SeedError(
            "expected an unlinked public seed advertisement object".into(),
        ));
    }
    let Payload::Public(payload) = &object.payload else {
        return Err(SeedError("encrypted advertisements are forbidden".into()));
    };
    if payload.len() as u64 > MAX_ADVERTISEMENT_BYTES {
        return Err(SeedError("advertisement payload exceeds bound".into()));
    }
    let ad: Advertisement = serde_json::from_slice(payload)?;
    validate(&ad, now_ms)?;
    if ad.provider != object.author_human.as_str() || object.timestamp_ms != ad.issued_at_ms {
        return Err(SeedError(
            "advertisement provider/time differs from signed author/time".into(),
        ));
    }
    Ok(ad)
}
/// Sign a complete owner-selected inventory snapshot. Publishing and trusting
/// it remain separate explicit actions. Empty entries withdraw all objects.
pub fn sign(ad: &Advertisement, root: &Did, device: &Controller, now_ms: u64) -> Result<Object> {
    validate(ad, Some(now_ms))?;
    if root.as_str() != ad.provider {
        return Err(SeedError(
            "advertisement signer differs from provider".into(),
        ));
    }
    let payload = serde_json::to_vec(ad)?;
    if payload.len() as u64 > MAX_ADVERTISEMENT_BYTES {
        return Err(SeedError("advertisement payload exceeds bound".into()));
    }
    let object = ObjectBuilder::new(ObjectType::Custom(ADVERTISEMENT_TYPE.into()))
        .timestamp_ms(ad.issued_at_ms)
        .payload(Payload::Public(payload))
        .sign(root, device)
        .map_err(|e| SeedError(e.to_string()))?;
    if object.to_bytes().len() as u64 > MAX_ADVERTISEMENT_BYTES {
        return Err(SeedError("signed advertisement exceeds wire bound".into()));
    }
    Ok(object)
}
/// Verify exact content integrity, signature, current delegation and required
/// object capability against KELs the caller obtained through its trust path.
pub fn verify(object: &Object, root: &Kel, device: &Kel, now_ms: u64) -> Result<Advertisement> {
    // Check bounded public fields before serializing a mutable Object.
    if !object.links.is_empty()
        || !matches!(&object.payload, Payload::Public(p) if p.len() as u64 <= MAX_ADVERTISEMENT_BYTES)
        || object.object_type != ObjectType::Custom(ADVERTISEMENT_TYPE.into())
    {
        return Err(SeedError("not a bounded public seed advertisement".into()));
    }
    if object.to_bytes().len() as u64 > MAX_ADVERTISEMENT_BYTES {
        return Err(SeedError("signed advertisement exceeds wire bound".into()));
    }
    let ad = decode(object, Some(now_ms))?;
    verify_provenance(object, root, device).map_err(|e| SeedError(e.to_string()))?;
    Ok(ad)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    issued_at_ms: u64,
    object_id: String,
    object: Option<Vec<u8>>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u8,
    providers: BTreeMap<String, Record>,
}
#[derive(Debug, Clone)]
pub struct Catalog {
    root: PathBuf,
}
impl Catalog {
    pub fn open(path: &Path) -> Result<Self> {
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()?.join(path)
        };
        plain_path(&path)?;
        mini_durable::create_dir_all(&path)?;
        let catalog = Self {
            root: fs::canonicalize(path)?,
        };
        let _lock = catalog.lock()?;
        if !catalog.root.join("catalog.json").try_exists()? {
            // An unrelated nonempty directory is never treated as new state.
            for entry in fs::read_dir(&catalog.root)? {
                if entry?.file_name() != "catalog.lock" {
                    return Err(SeedError(
                        "catalog state missing from nonempty directory".into(),
                    ));
                }
            }
            catalog.save(&State {
                version: 1,
                providers: BTreeMap::new(),
            })?;
        }
        catalog.load()?;
        Ok(catalog)
    }
    fn lock(&self) -> Result<File> {
        plain_path(&self.root)?;
        let path = self.root.join("catalog.lock");
        plain_path(&path)?;
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if !meta.is_file() {
                return Err(SeedError("catalog lock must be regular".into()));
            }
        }
        let lock = mini_durable::lock_exclusive(&path)?;
        self.cleanup_temps()?;
        Ok(lock)
    }
    fn cleanup_temps(&self) -> Result<()> {
        let mut removals = Vec::new();
        for (index, entry) in fs::read_dir(&self.root)?.enumerate() {
            if index >= 128 {
                return Err(SeedError("too many catalog metadata entries".into()));
            }
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| SeedError("untrusted catalog filename".into()))?;
            let meta = fs::symlink_metadata(entry.path())?;
            if !meta.is_file() || meta.file_type().is_symlink() {
                return Err(SeedError(
                    "catalog entries must be regular non-symlink files".into(),
                ));
            }
            if matches!(name.as_str(), "catalog.json" | "catalog.lock") {
                continue;
            }
            if !crate::is_temp(&name, "catalog.json") || meta.len() > MAX_CATALOG_BYTES {
                return Err(SeedError(
                    "untrusted catalog filename or oversized orphan".into(),
                ));
            }
            removals.push(entry.path());
        }
        for path in removals {
            fs::remove_file(&path)?;
            mini_durable::sync_parent(&path)?;
        }
        Ok(())
    }
    fn load(&self) -> Result<State> {
        let state: State = serde_json::from_slice(&read_bounded(
            &self.root.join("catalog.json"),
            MAX_CATALOG_BYTES,
        )?)?;
        if state.version != 1 || state.providers.len() > MAX_PROVIDERS {
            return Err(SeedError("catalog version/provider limit invalid".into()));
        }
        for (provider, record) in &state.providers {
            Did::parse(provider).map_err(|e| SeedError(e.to_string()))?;
            id_valid(&record.object_id)?;
            if let Some(bytes) = &record.object {
                if bytes.len() as u64 > MAX_ADVERTISEMENT_BYTES {
                    return Err(SeedError("stored advertisement exceeds bound".into()));
                }
                let object = Object::from_bytes(bytes).map_err(|e| SeedError(e.to_string()))?;
                let ad = decode(&object, None)?;
                if object.id().as_str() != record.object_id
                    || ad.provider != *provider
                    || ad.issued_at_ms != record.issued_at_ms
                {
                    return Err(SeedError("catalog advertisement/highwater mismatch".into()));
                }
            }
        }
        Ok(state)
    }
    fn save(&self, state: &State) -> Result<()> {
        let bytes = serde_json::to_vec(state)?;
        if bytes.len() as u64 > MAX_CATALOG_BYTES {
            return Err(SeedError("catalog exceeds 8 MiB capacity".into()));
        }
        plain_path(&self.root.join("catalog.json"))?;
        mini_durable::atomic_replace(&self.root.join("catalog.json"), &bytes)?;
        Ok(())
    }
    /// Authenticate again on every replacement; no public constructor can
    /// manufacture a previously verified admission token.
    pub fn replace(
        &self,
        object: &Object,
        root: &Kel,
        device: &Kel,
        now_ms: u64,
    ) -> Result<Advertisement> {
        let ad = verify(object, root, device, now_ms)?;
        let _lock = self.lock()?;
        let mut state = self.load()?;
        if let Some(old) = state.providers.get(&ad.provider) {
            if ad.issued_at_ms < old.issued_at_ms {
                return Err(SeedError("provider advertisement rollback".into()));
            }
            if ad.issued_at_ms == old.issued_at_ms {
                if object.id().as_str() != old.object_id {
                    return Err(SeedError("provider advertisement equivocation".into()));
                }
                if old.object.is_none() {
                    return Err(SeedError("removed advertisement cannot be replayed".into()));
                }
                return Ok(ad);
            }
        } else if state.providers.len() >= MAX_PROVIDERS {
            return Err(SeedError(
                "catalog provider/highwater capacity reached".into(),
            ));
        }
        state.providers.insert(
            ad.provider.clone(),
            Record {
                issued_at_ms: ad.issued_at_ms,
                object_id: object.id().as_str().into(),
                object: Some(object.to_bytes()),
            },
        );
        self.save(&state)?;
        Ok(ad)
    }
    /// Forget active hints while retaining rollback protection. Tombstones
    /// count toward the bounded 64-provider highwater capacity.
    pub fn remove(&self, provider: &str) -> Result<bool> {
        Did::parse(provider).map_err(|e| SeedError(e.to_string()))?;
        let _lock = self.lock()?;
        let mut state = self.load()?;
        let removed = state
            .providers
            .get_mut(provider)
            .is_some_and(|r| r.object.take().is_some());
        if removed {
            self.save(&state)?;
        }
        Ok(removed)
    }
    /// Original signed snapshots for revalidation against current caller KELs.
    /// Returned objects are integrity checked, not current trust assertions.
    /// Lease expiry must still be checked in the authenticated advertisement.
    pub fn objects(&self, now_ms: u64) -> Result<Vec<Object>> {
        let _lock = self.lock()?;
        let state = self.load()?;
        let mut objects = Vec::new();
        for record in state.providers.values() {
            if let Some(bytes) = &record.object {
                let object = Object::from_bytes(bytes).map_err(|e| SeedError(e.to_string()))?;
                let ad = decode(&object, None)?;
                if ad.expires_at_ms > now_ms
                    && ad.issued_at_ms <= now_ms.saturating_add(MAX_CLOCK_SKEW_MS)
                {
                    objects.push(object);
                }
            }
        }
        Ok(objects)
    }
    pub fn inventory(&self, now_ms: u64) -> Result<Vec<Advertisement>> {
        let _lock = self.lock()?;
        let state = self.load()?;
        let mut ads = Vec::new();
        for record in state.providers.values() {
            if let Some(bytes) = &record.object {
                let object = Object::from_bytes(bytes).map_err(|e| SeedError(e.to_string()))?;
                let mut ad = decode(&object, None)?;
                if ad.expires_at_ms <= now_ms
                    || ad.issued_at_ms > now_ms.saturating_add(MAX_CLOCK_SKEW_MS)
                {
                    continue;
                }
                ad.entries.retain(|e| e.expires_at_ms > now_ms);
                ads.push(ad);
            }
        }
        Ok(ads)
    }
    pub fn providers_for(&self, object_id: &str, now_ms: u64) -> Result<Vec<Advertisement>> {
        id_valid(object_id)?;
        Ok(self
            .inventory(now_ms)?
            .into_iter()
            .filter(|ad| ad.entries.iter().any(|e| e.id == object_id))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Capabilities;
    fn party(seed: u8) -> (Controller, Controller) {
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
    fn fixture(root: &Controller, device: &Controller, issued: u64) -> Advertisement {
        let obj = ObjectBuilder::new(ObjectType::Custom("catalog-test".into()))
            .payload(Payload::Public(b"source".to_vec()))
            .sign(&root.did(), device)
            .unwrap();
        Advertisement {
            provider: root.did().to_string(),
            address: "127.0.0.1:8900".into(),
            issued_at_ms: issued,
            expires_at_ms: issued + MAX_ADVERTISEMENT_TTL_MS,
            entries: vec![Lease {
                id: obj.id().as_str().into(),
                bytes: obj.to_bytes().len() as u64,
                expires_at_ms: issued + 86_400_000,
            }],
        }
    }
    #[test]
    fn verifies_authorship_current_delegation_and_exact_signed_content() {
        let (root, device) = party(10);
        let ad = fixture(&root, &device, 100);
        let object = sign(&ad, &root.did(), &device, 100).unwrap();
        assert_eq!(
            verify(&object, &root.kel(), &device.kel(), 100).unwrap(),
            ad
        );
        let (other, _) = party(20);
        assert!(verify(&object, &other.kel(), &device.kel(), 100).is_err());
        let mut tampered = object.clone();
        tampered.payload = Payload::Public(b"{}".to_vec());
        assert!(verify(&tampered, &root.kel(), &device.kel(), 100).is_err());
        let (undelegated_root, _) = party(30);
        let undelegated = Controller::incept_device_single_from_seeds(
            &undelegated_root.did(),
            &[40; 32],
            &[41; 32],
        )
        .unwrap();
        let ad = fixture(&undelegated_root, &undelegated, 100);
        let object = sign(&ad, &undelegated_root.did(), &undelegated, 100).unwrap();
        assert!(verify(&object, &undelegated_root.kel(), &undelegated.kel(), 100).is_err());
    }
    #[test]
    fn lease_and_ad_expiry_rollback_and_equivocation_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let (root, device) = party(10);
        let mut ad = fixture(&root, &device, 100);
        ad.entries[0].expires_at_ms = 101;
        let object = sign(&ad, &root.did(), &device, 100).unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        catalog
            .replace(&object, &root.kel(), &device.kel(), 100)
            .unwrap();
        assert_eq!(
            catalog.providers_for(&ad.entries[0].id, 100).unwrap().len(),
            1
        );
        assert_eq!(catalog.objects(100).unwrap(), vec![object.clone()]);
        assert!(catalog.objects(ad.expires_at_ms).unwrap().is_empty());
        assert!(catalog
            .providers_for(&ad.entries[0].id, 101)
            .unwrap()
            .is_empty());
        assert!(catalog.inventory(ad.expires_at_ms).unwrap().is_empty());
        drop(catalog);
        let orphan = dir.path().join("catalog.json.123-0.tmp");
        fs::write(&orphan, b"interrupted atomic write").unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        assert!(!orphan.exists());
        let mut older = fixture(&root, &device, 99);
        let old = sign(&older, &root.did(), &device, 100).unwrap();
        assert!(catalog
            .replace(&old, &root.kel(), &device.kel(), 100)
            .is_err());
        older.issued_at_ms = 100;
        let conflict = sign(&older, &root.did(), &device, 100).unwrap();
        assert!(catalog
            .replace(&conflict, &root.kel(), &device.kel(), 100)
            .is_err());
        assert!(verify(&object, &root.kel(), &device.kel(), ad.expires_at_ms).is_err());
    }
    #[test]
    fn full_snapshot_replacement_and_owner_removal_withdraw_old_entries() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        let (root, device) = party(10);
        let mut ad = fixture(&root, &device, 100);
        let id = ad.entries[0].id.clone();
        let first = sign(&ad, &root.did(), &device, 100).unwrap();
        catalog
            .replace(&first, &root.kel(), &device.kel(), 100)
            .unwrap();
        ad.issued_at_ms = 101;
        ad.entries.clear();
        let withdrawn = sign(&ad, &root.did(), &device, 101).unwrap();
        catalog
            .replace(&withdrawn, &root.kel(), &device.kel(), 101)
            .unwrap();
        assert!(catalog.providers_for(&id, 101).unwrap().is_empty());
        assert!(catalog.remove(&ad.provider).unwrap());
        assert!(!catalog.remove(&ad.provider).unwrap());
        assert!(catalog
            .replace(&withdrawn, &root.kel(), &device.kel(), 101)
            .is_err());
        assert!(catalog.inventory(101).unwrap().is_empty());
    }
    #[test]
    fn bounds_fail_before_replacing_good_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        let (root, device) = party(10);
        let original = fixture(&root, &device, 100);
        let object = sign(&original, &root.did(), &device, 100).unwrap();
        catalog
            .replace(&object, &root.kel(), &device.kel(), 100)
            .unwrap();
        let mut invalid = original.clone();
        invalid.entries.push(invalid.entries[0].clone());
        assert!(sign(&invalid, &root.did(), &device, 100).is_err());
        invalid = original.clone();
        invalid.expires_at_ms += 1;
        assert!(sign(&invalid, &root.did(), &device, 100).is_err());
        invalid = original.clone();
        invalid.issued_at_ms = 100 + MAX_CLOCK_SKEW_MS + 1;
        assert!(sign(&invalid, &root.did(), &device, 100).is_err());
        invalid = original.clone();
        invalid.address = "provider.example:8000".into();
        assert!(sign(&invalid, &root.did(), &device, 100).is_err());
        let mut state = catalog.load().unwrap();
        let before = fs::read(dir.path().join("catalog.json")).unwrap();
        state.providers.get_mut(&original.provider).unwrap().object =
            Some(vec![0; MAX_CATALOG_BYTES as usize]);
        assert!(catalog.save(&state).is_err());
        assert_eq!(fs::read(dir.path().join("catalog.json")).unwrap(), before);
        assert_eq!(catalog.inventory(100).unwrap(), vec![original]);
    }
    #[cfg(unix)]
    #[test]
    fn symlink_catalog_root_metadata_and_lock_are_rejected() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), dir.path().join("alias")).unwrap();
        assert!(Catalog::open(&dir.path().join("alias")).is_err());
        let path = dir.path().join("catalog");
        let catalog = Catalog::open(&path).unwrap();
        let state = path.join("catalog.json");
        fs::rename(&state, outside.path().join("saved")).unwrap();
        symlink(outside.path().join("saved"), &state).unwrap();
        assert!(catalog.inventory(0).is_err());
        fs::remove_file(&state).unwrap();
        fs::rename(outside.path().join("saved"), &state).unwrap();
        fs::remove_file(path.join("catalog.lock")).unwrap();
        symlink(&state, path.join("catalog.lock")).unwrap();
        assert!(catalog.inventory(0).is_err());
    }
}
