//! Bounded owner-reviewed linked history/media collections. No automatic publication.
use super::*;
use mini_forge::SourceNodeKind;
use mini_store::{CacheTier, FsBackend, Store};
use std::collections::{BTreeMap, VecDeque};
const MAX_NODES: usize = 4096;
const MAX_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_PREVIEW: usize = 2 * 1024 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Media,
    History,
    Links,
}
fn parse_kind(s: &str) -> Result<Kind> {
    match s {
        "media" => Ok(Kind::Media),
        "history" => Ok(Kind::History),
        "links" => Ok(Kind::Links),
        _ => Err(err("kind must be media, history or links")),
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expected {
    Any,
    MediaRoot,
    Manifest,
    Chunk,
    History(SourceNodeKind),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: String,
    bytes: u64,
    object_type: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    version: u8,
    owner: String,
    home: String,
    source: String,
    root: String,
    kind: Kind,
    days: u16,
    objects: Vec<Entry>,
    total_bytes: u64,
    reconstructed_bytes: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preview {
    id: String,
    plan: Plan,
}
fn root_expected(kind: Kind) -> Expected {
    match kind {
        Kind::Media => Expected::MediaRoot,
        Kind::History => Expected::History(SourceNodeKind::Commit),
        Kind::Links => Expected::Any,
    }
}
fn children(obj: &Object, expected: Expected) -> Result<Vec<(ObjectId, Expected)>> {
    if !matches!(obj.payload, mini_objects::Payload::Public(_)) {
        return Err(err("collection contains encrypted/private content"));
    }
    match expected {
        Expected::Any => Ok(obj
            .links
            .iter()
            .map(|l| (l.target.clone(), Expected::Any))
            .collect()),
        Expected::History(kind) => mini_forge::source_dependencies(obj, kind)
            .map(|v| {
                v.into_iter()
                    .map(|(id, k)| (id, Expected::History(k)))
                    .collect()
            })
            .map_err(err),
        Expected::MediaRoot
            if obj.object_type
                == mini_objects::ObjectType::Custom(mini_media::SUPERBLOCK_TYPE.into()) =>
        {
            let block = mini_media::read_superblock(obj).map_err(err)?;
            if obj.links.iter().any(|l| l.rel != "part") || block.parts.len() != obj.links.len() {
                return Err(err("superblock contains unexpected links"));
            }
            Ok(block
                .parts
                .into_iter()
                .map(|id| (id, Expected::Manifest))
                .collect())
        }
        Expected::MediaRoot | Expected::Manifest => {
            let manifest = mini_media::read_manifest(obj).map_err(err)?;
            if obj.links.iter().any(|l| l.rel != "chunk")
                || manifest.chunks.len() != obj.links.len()
            {
                return Err(err("media manifest contains unexpected links"));
            }
            Ok(manifest
                .chunks
                .into_iter()
                .map(|id| (id, Expected::Chunk))
                .collect())
        }
        Expected::Chunk => {
            let mini_objects::Payload::Public(bytes) = &obj.payload else {
                return Err(err("private chunk"));
            };
            if obj.object_type != mini_objects::ObjectType::Custom(mini_media::CHUNK_TYPE.into())
                || !obj.links.is_empty()
                || bytes.len() > mini_media::CHUNK_SIZE
            {
                return Err(err("invalid signed media chunk"));
            }
            Ok(Vec::new())
        }
    }
}
fn verify(cache: &mut mini_sync::KelCache, obj: &Object) -> Result<()> {
    match mini_sync::Ingest::check(cache, obj) {
        mini_sync::IngestOutcome::Accepted | mini_sync::IngestOutcome::AcceptedCarrier => Ok(()),
        _ => Err(err(
            "collection object signature/provenance is not verified",
        )),
    }
}
fn context(home: &Path, source: &Store<FsBackend>) -> Result<mini_sync::KelCache> {
    let identity = crate::identity::load(home)?;
    let mut cache = crate::store::build_kel_cache(home, &identity)?;
    cache.hydrate_from_store(source).map_err(err)?;
    Ok(cache)
}
fn source_object(
    source: &Store<FsBackend>,
    cache: &mut mini_sync::KelCache,
    id: &str,
    sharing: bool,
) -> Result<Object> {
    let id = ObjectId::parse(id).map_err(err)?;
    let obj = source.get(&id).map_err(err)?;
    if sharing
        && matches!(
            source.cache_tier(&id).map_err(err)?,
            CacheTier::PrivateOnly | CacheTier::ColdArchive
        )
    {
        return Err(err("collection includes a private/cold source object"));
    }
    verify(cache, &obj)?;
    if !matches!(obj.payload, mini_objects::Payload::Public(_)) {
        return Err(err("collection includes encrypted content"));
    }
    Ok(obj)
}
fn walk<F>(
    root: &str,
    kind: Kind,
    max_nodes: usize,
    max_bytes: u64,
    mut load: F,
) -> Result<(Vec<Entry>, u64)>
where
    F: FnMut(&str, u64) -> Result<Object>,
{
    ObjectId::parse(root).map_err(err)?;
    let mut queue = VecDeque::from([root.to_string()]);
    let mut expected = BTreeMap::from([(root.to_string(), root_expected(kind))]);
    let mut trees: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
    let mut file_sizes = BTreeMap::new();
    let mut entries = BTreeMap::new();
    let mut total = 0u64;
    while let Some(id) = queue.pop_front() {
        let obj = load(&id, max_bytes.saturating_sub(total))?;
        if obj.id().as_str() != id {
            return Err(err("collection loader substituted an object"));
        }
        let size = obj.to_bytes().len() as u64;
        if size > mini_seed::MAX_OBJECT_BYTES {
            return Err(err("collection object exceeds64MiB"));
        }
        total = total
            .checked_add(size)
            .ok_or_else(|| err("collection byte overflow"))?;
        if total > max_bytes {
            return Err(err("collection exceeds chosen byte limit"));
        }
        let dependencies = children(
            &obj,
            *expected
                .get(&id)
                .ok_or_else(|| err("missing graph expectation"))?,
        )?;
        if expected[&id] == Expected::History(SourceNodeKind::Tree) {
            trees.insert(
                id.clone(),
                dependencies
                    .iter()
                    .map(|(id, k)| {
                        (
                            id.as_str().to_string(),
                            *k == Expected::History(SourceNodeKind::Tree),
                        )
                    })
                    .collect(),
            );
        }
        if expected[&id] == Expected::History(SourceNodeKind::File) {
            if let mini_objects::Payload::Public(bytes) = &obj.payload {
                file_sizes.insert(id.clone(), bytes.len() as u64);
            }
        }
        entries.insert(
            id.clone(),
            Entry {
                id: id.clone(),
                bytes: size,
                object_type: format!("{:?}", obj.object_type),
            },
        );
        for (target, node_kind) in dependencies {
            let key = target.as_str().to_string();
            if let Some(old) = expected.get(&key) {
                if *old != node_kind {
                    return Err(err("linked object has conflicting required types"));
                }
            } else {
                if expected.len() >= max_nodes {
                    return Err(err("collection exceeds chosen object count"));
                }
                expected.insert(key.clone(), node_kind);
                queue.push_back(key);
            }
        }
    }
    check_tree_work(&trees, &file_sizes)?;
    Ok((entries.into_values().collect(), total))
}
fn check_tree_work(
    trees: &BTreeMap<String, Vec<(String, bool)>>,
    file_sizes: &BTreeMap<String, u64>,
) -> Result<()> {
    #[derive(Clone, Copy)]
    struct Work {
        height: usize,
        files: usize,
        bytes: u64,
    }
    let mut work: BTreeMap<String, Work> = BTreeMap::new();
    let mut active = std::collections::BTreeSet::new();
    for root in trees.keys() {
        let mut stack = vec![(root.clone(), false)];
        while let Some((id, exit)) = stack.pop() {
            if work.contains_key(&id) {
                continue;
            }
            let children = trees
                .get(&id)
                .ok_or_else(|| err("missing tree dependency"))?;
            if exit {
                let mut total = Work {
                    height: 1,
                    files: 0,
                    bytes: 0,
                };
                for (child, is_dir) in children {
                    let next = if *is_dir {
                        *work.get(child).ok_or_else(|| err("incomplete tree work"))?
                    } else {
                        Work {
                            height: 0,
                            files: 1,
                            bytes: *file_sizes
                                .get(child)
                                .ok_or_else(|| err("missing file dependency"))?,
                        }
                    };
                    total.height = total.height.max(next.height + 1);
                    total.files = total
                        .files
                        .checked_add(next.files)
                        .ok_or_else(|| err("checkout file count overflow"))?;
                    total.bytes = total
                        .bytes
                        .checked_add(next.bytes)
                        .ok_or_else(|| err("checkout byte count overflow"))?;
                    if total.height > mini_forge::MAX_TREE_DEPTH + 1 {
                        return Err(err("history exceeds native tree depth"));
                    }
                    if total.files > mini_forge::MAX_CHECKOUT_FILES
                        || total.bytes > mini_forge::MAX_CHECKOUT_BYTES as u64
                    {
                        return Err(err("history exceeds native expanded checkout limits"));
                    }
                }
                work.insert(id.clone(), total);
                active.remove(&id);
            } else {
                if !active.insert(id.clone()) {
                    return Err(err("source tree cycle"));
                }
                stack.push((id, true));
                for (child, is_dir) in children {
                    if *is_dir && !work.contains_key(child) {
                        stack.push((child.clone(), false));
                    }
                }
            }
        }
    }
    Ok(())
}
fn media_info(source: &Store<FsBackend>, root: &str, limit: u64) -> Result<u64> {
    let obj = source
        .get(&ObjectId::parse(root).map_err(err)?)
        .map_err(err)?;
    if obj.object_type == mini_objects::ObjectType::Custom(mini_media::SUPERBLOCK_TYPE.into()) {
        let block = mini_media::read_superblock(&obj).map_err(err)?;
        if block.total_len > limit {
            return Err(err("reconstructed media exceeds output byte bound"));
        }
        let mut parts = BTreeMap::new();
        let mut references = 0usize;
        for id in &block.parts {
            let count = if let Some(count) = parts.get(id.as_str()) {
                *count
            } else {
                let part = mini_media::read_manifest(&source.get(id).map_err(err)?).map_err(err)?;
                parts.insert(id.as_str().to_string(), part.chunks.len());
                part.chunks.len()
            };
            references = references
                .checked_add(count)
                .ok_or_else(|| err("media work overflow"))?;
            if references > MAX_NODES {
                return Err(err("reconstructed media exceeds4096 chunk references"));
            }
        }
        Ok(block.total_len)
    } else {
        let m = mini_media::read_manifest(&obj).map_err(err)?;
        if m.total_len > limit {
            return Err(err("reconstructed media exceeds output byte bound"));
        }
        Ok(m.total_len)
    }
}
struct OutputLimit<'a, W> {
    inner: &'a mut W,
    left: u64,
}
impl<W: Write> Write for OutputLimit<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.left {
            return Err(std::io::Error::other("media output limit exceeded"));
        }
        let n = self.inner.write(bytes)?;
        self.left -= n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
fn media_check(
    source: &Store<FsBackend>,
    root: &str,
    writer: &mut impl Write,
    limit: u64,
) -> Result<()> {
    media_info(source, root, limit)?;
    let mut bounded = OutputLimit {
        inner: writer,
        left: limit,
    };
    let writer = &mut bounded;
    let obj = source
        .get(&ObjectId::parse(root).map_err(err)?)
        .map_err(err)?;
    if obj.object_type == mini_objects::ObjectType::Custom(mini_media::SUPERBLOCK_TYPE.into()) {
        mini_media::assemble_superblock_to_writer(
            source,
            &mini_media::read_superblock(&obj).map_err(err)?,
            writer,
        )
        .map_err(err)
    } else {
        mini_media::assemble_to_writer(
            source,
            &mini_media::read_manifest(&obj).map_err(err)?,
            writer,
        )
        .map_err(err)
    }
}
fn canonical_path(path: &Path) -> Result<String> {
    path.canonicalize()
        .map_err(err)?
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| err("collection path must be valid UTF-8"))
}
fn plan(home: &Path, path: &Path, root: &str, kind: Kind, days: u16) -> Result<Preview> {
    if !(1..=3650).contains(&days) {
        return Err(err("days must be1..3650"));
    }
    safe_dir(home)?;
    let source = crate::store::open_store(path)?;
    let mut cache = context(home, &source)?;
    let (objects, total_bytes) = walk(root, kind, MAX_NODES, MAX_BYTES, |id, _| {
        source_object(&source, &mut cache, id, true)
    })?;
    if kind == Kind::Media {
        media_check(&source, root, &mut std::io::sink(), MAX_BYTES)?;
    }
    let owner = crate::identity::load(home)?.human_did().to_string();
    let plan = Plan {
        version: 1,
        owner,
        home: canonical_path(home)?,
        source: canonical_path(path)?,
        root: root.into(),
        kind,
        days,
        objects,
        total_bytes,
        reconstructed_bytes: if kind == Kind::Media {
            Some(media_info(&source, root, MAX_BYTES)?)
        } else {
            None
        },
    };
    let id = blake3::hash(&serde_json::to_vec(&plan).map_err(err)?)
        .to_hex()
        .to_string();
    Ok(Preview { id, plan })
}
fn create_document(path: &Path, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_PREVIEW {
        return Err(err("collection document exceeds bound"));
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(err)?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(err)?;
    mini_durable::sync_parent(path).map_err(err)
}
fn admit(home: &Path, path: &Path, preview: &Preview) -> Result<Vec<mini_seed::Lease>> {
    let source = crate::store::open_store(path)?;
    let mut cache = context(home, &source)?;
    let ids: Vec<_> = preview.plan.objects.iter().map(|e| e.id.clone()).collect();
    pool(home)?
        .admit_collection(&ids, preview.plan.days, now(), |id| {
            source_object(&source, &mut cache, id, true)
                .map_err(|e| mini_seed::SeedError(e.to_string()))
        })
        .map_err(err)
}
fn fetch_collection(home: &Path, path: &Path, mut args: Vec<String>) -> Result<Value> {
    let root = need(&mut args, "--root")?;
    ObjectId::parse(&root).map_err(err)?;
    let kind = parse_kind(&need(&mut args, "--kind")?)?;
    let max_nodes = usize::try_from(number(&need(&mut args, "--max-objects")?)?).map_err(err)?;
    let max_bytes = number(&need(&mut args, "--max-bytes")?)?;
    let max_downloads =
        usize::try_from(number(&need(&mut args, "--max-downloads")?)?).map_err(err)?;
    done(&args)?;
    if !(1..=MAX_NODES).contains(&max_nodes)
        || !(1..=MAX_BYTES).contains(&max_bytes)
        || !(1..=MAX_NODES).contains(&max_downloads)
    {
        return Err(err(
            "collection fetch limits must fit4096 objects/downloads and1GiB",
        ));
    }
    let source = crate::store::open_store(path)?;
    let mut cache = context(home, &source)?;
    let mut budget = mesh::RetrievalBudget {
        deadline: Instant::now() + Duration::from_secs(120),
        byte_limit: MAX_BYTES,
        traffic_left: max_bytes,
        attempts_left: max_downloads,
    };
    let mut downloaded = Vec::new();
    let mut reused = 0;
    let result = walk(&root, kind, max_nodes, max_bytes, |id, remaining_bytes| {
        let oid = ObjectId::parse(id).map_err(err)?;
        if source.contains(&oid).map_err(err)? {
            reused += 1;
            return source_object(&source, &mut cache, id, false);
        }
        if budget.attempts_left == 0 || remaining_bytes == 0 {
            return Err(err(
                "download window exhausted; rerun to resume from verified local objects",
            ));
        }
        budget.byte_limit = remaining_bytes.min(mini_seed::MAX_OBJECT_BYTES);
        let report = mesh::retrieve_until(home, path, id, None, false, &mut budget)?;
        if report["retrieved"] != true {
            return Err(err(format!("no provider completed {id}: {report}")));
        }
        downloaded.push(id.to_string());
        source_object(&source, &mut cache, id, false)
    });
    match result {
        Ok((objects, bytes)) => {
            if kind == Kind::Media {
                if let Err(e) = media_check(&source, &root, &mut std::io::sink(), max_bytes) {
                    return Ok(
                        json!({"complete":false,"root":root,"downloaded":downloaded,"reused":reused,"error":e.to_string(),"attempts_used":max_downloads-budget.attempts_left,"received_body_bytes":max_bytes-budget.traffic_left,"seeded":false}),
                    );
                }
            }
            Ok(
                json!({"complete":true,"root":root,"objects":objects.len(),"bytes":bytes,"downloaded":downloaded,"reused":reused,"attempts_used":max_downloads-budget.attempts_left,"received_body_bytes":max_bytes-budget.traffic_left,"seeded":false,"note":"Verified objects retained in the ordinary store. Review a collection preview before sharing."}),
            )
        }
        Err(e) => Ok(
            json!({"complete":false,"root":root,"downloaded":downloaded,"reused":reused,"error":e.to_string(),"resume_same_command":true,"attempts_used":max_downloads-budget.attempts_left,"received_body_bytes":max_bytes-budget.traffic_left,"seeded":false}),
        ),
    }
}
fn export_media(home: &Path, path: &Path, mut args: Vec<String>) -> Result<Value> {
    let root = need(&mut args, "--root")?;
    let output = need(&mut args, "--output")?;
    done(&args)?;
    let source = crate::store::open_store(path)?;
    let mut cache = context(home, &source)?;
    walk(&root, Kind::Media, MAX_NODES, MAX_BYTES, |id, _| {
        source_object(&source, &mut cache, id, false)
    })?;
    media_info(&source, &root, MAX_BYTES)?;
    let destination = std::path::PathBuf::from(&output);
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    safe_dir(parent)?;
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(err("output already exists"));
    }
    let nonce = mini_crypto::random_32().map_err(err)?;
    let temp = parent.join(format!(
        ".mininet-media-{}-{}.tmp",
        std::process::id(),
        blake3::hash(&nonce).to_hex()
    ));
    let mut created = false;
    let result = (|| -> Result<Value> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(err)?;
        created = true;
        media_check(&source, &root, &mut file, MAX_BYTES)?;
        file.sync_all().map_err(err)?;
        let bytes = file.metadata().map_err(err)?.len();
        drop(file);
        // Publish without overwriting a path created after the initial existence check.
        fs::hard_link(&temp, &destination).map_err(err)?;
        mini_durable::sync_parent(&destination).map_err(err)?;
        Ok(
            json!({"output":output,"root":root,"bytes":bytes,"digest_verified":true,"executed":false}),
        )
    })();
    let cleanup = if created {
        fs::remove_file(&temp).and_then(|_| mini_durable::sync_directory(parent))
    } else {
        Ok(())
    };
    match result {
        Ok(v) => {
            cleanup.map_err(err)?;
            Ok(v)
        }
        Err(e) => {
            let _ = cleanup;
            Err(e)
        }
    }
}
pub(super) fn dispatch(
    home: &Path,
    path: &Path,
    verb: &str,
    mut args: Vec<String>,
) -> Result<Value> {
    match verb {
        "collection-preview" => {
            let root = need(&mut args, "--root")?;
            let kind = parse_kind(&need(&mut args, "--kind")?)?;
            let days = u16::try_from(number(&need(&mut args, "--days")?)?).map_err(err)?;
            let output = need(&mut args, "--output")?;
            done(&args)?;
            let preview = plan(home, path, &root, kind, days)?;
            create_document(
                Path::new(&output),
                &serde_json::to_vec_pretty(&preview).map_err(err)?,
            )?;
            Ok(
                json!({"preview_id":preview.id,"output":output,"objects":preview.plan.objects.len(),"bytes":preview.plan.total_bytes,"days":days,"reconstructed_bytes":preview.plan.reconstructed_bytes,"sharing_started":false,"note":"Confirmation selects every listed public object and resets its pool retention from apply time."}),
            )
        }
        "collection-add" => {
            let file = need(&mut args, "--preview")?;
            let confirm = need(&mut args, "--confirm")?;
            done(&args)?;
            let preview: Preview =
                serde_json::from_slice(&read_file(Path::new(&file), MAX_PREVIEW)?).map_err(err)?;
            let checked = plan(
                home,
                path,
                &preview.plan.root,
                preview.plan.kind,
                preview.plan.days,
            )?;
            if preview != checked || confirm != checked.id {
                return Err(err("collection confirmation must match exact current source/home/objects/retention preview"));
            }
            let leases = admit(home, path, &checked)?;
            Ok(
                json!({"preview_id":checked.id,"admitted":leases.len(),"leases":leases,"source_modified":false,"payment_final":false}),
            )
        }
        "collection-fetch" => fetch_collection(home, path, args),
        "media-export" => export_media(home, path, args),
        _ => Err(err("unknown collection command")),
    }
}
