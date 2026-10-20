//! Owner-local contacts and signed availability hints. Never a canonical registry.
use super::*;
use mini_seed::catalog::{self, Advertisement, Catalog};
use std::collections::{BTreeMap, BTreeSet};

mod search;
pub(super) use search::serve as serve_search;

const MAX_CONTACTS: usize = 64;
const MAX_CONTACT_BYTES: usize = 64 * 1024;
const MAX_AD_BYTES: usize = 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Contact {
    address: String,
    peer: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Announcement {
    address: String,
    ttl_ms: u64,
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn address(s: &str) -> Result<String> {
    let a: std::net::SocketAddr = s.parse().map_err(err)?;
    if a.port() == 0
        || a.ip().is_unspecified()
        || a.ip().is_multicast()
        || matches!(a.ip(),std::net::IpAddr::V4(ip) if ip.is_broadcast())
    {
        return Err(err(
            "contact/announcement needs a unicast numeric endpoint with nonzero port",
        ));
    }
    Ok(a.to_string())
}
fn contacts(home: &Path) -> Result<BTreeMap<String, Contact>> {
    let path = home.join("seed-contacts.json");
    if !path.try_exists().map_err(err)? {
        return Ok(BTreeMap::new());
    }
    let all: BTreeMap<String, Contact> =
        serde_json::from_slice(&read_file(&path, MAX_CONTACT_BYTES)?).map_err(err)?;
    if all.len() > MAX_CONTACTS {
        return Err(err("too many seed contacts"));
    }
    let mut seen = BTreeSet::new();
    for (name, c) in &all {
        if !valid_name(name)
            || address(&c.address)? != c.address
            || !seen.insert((c.peer.clone(), c.address.clone()))
        {
            return Err(err("invalid or duplicate seed contact"));
        }
        Did::parse(&c.peer).map_err(err)?;
    }
    Ok(all)
}
fn trusted<'a>(
    cache: &'a mini_sync::KelCache,
    obj: &Object,
) -> Result<(&'a did_mini::Kel, &'a did_mini::Kel)> {
    let root = cache
        .get(&obj.author_human)
        .ok_or_else(|| err("advertisement root is not trusted"))?;
    let device = cache
        .get(&obj.author_device)
        .ok_or_else(|| err("advertisement device is not trusted"))?;
    Ok((root, device))
}
fn local_catalog(home: &Path) -> Result<Catalog> {
    Catalog::open(&home.join("seed-catalog")).map_err(err)
}
fn announcement(home: &Path) -> Result<Announcement> {
    let a: Announcement =
        serde_json::from_slice(&read_file(&home.join("seed-announcement.json"), 4096)?)
            .map_err(err)?;
    if address(&a.address)? != a.address || !(1000..=900_000).contains(&a.ttl_ms) {
        return Err(err("invalid announcement configuration"));
    }
    Ok(a)
}
pub(super) fn advertise(home: &Path, b: &mut TcpBearer, c: &mut Channel) -> Result<()> {
    let identity = crate::identity::load(home)?;
    let _lock = mini_durable::lock_exclusive(&home.join("seed-announcement.lock")).map_err(err)?;
    let a = announcement(home)?;
    let checkpoint = home.join("seed-announcement-issued");
    let previous = if checkpoint.try_exists().map_err(err)? {
        let text = String::from_utf8(read_file(&checkpoint, 32)?).map_err(err)?;
        number(&text)?
    } else {
        0
    };
    let issued = now().max(
        previous
            .checked_add(1)
            .ok_or_else(|| err("advertisement clock exhausted"))?,
    );
    let ad = Advertisement {
        provider: identity.human_did().to_string(),
        address: a.address,
        issued_at_ms: issued,
        expires_at_ms: issued
            .checked_add(a.ttl_ms)
            .ok_or_else(|| err("advertisement expiry overflow"))?,
        entries: pool(home)?.inventory(now()).map_err(err)?,
    };
    let obj = catalog::sign(&ad, &identity.human_did(), &identity.device, now()).map_err(err)?;
    let bytes = obj.to_bytes();
    if bytes.len() > MAX_AD_BYTES {
        return Err(err("advertisement exceeds bound"));
    }
    // Reserve issuance before publication; a failed network send never reuses a timestamp.
    mini_durable::atomic_replace(&checkpoint, issued.to_string().as_bytes()).map_err(err)?;
    drop(_lock);
    send(b, c, &json!({"bytes":bytes.len()}))?;
    for chunk in bytes.chunks(CHUNK) {
        send_bytes(b, c, chunk)?;
    }
    Ok(())
}
fn pull_ad(home: &Path, contact: &Contact, deadline: Instant) -> Result<Advertisement> {
    let peer = Did::parse(&contact.peer).map_err(err)?;
    let mut session = connect_until(home, &contact.address, &peer, deadline)?;
    send(
        &mut session.bearer,
        &mut session.channel,
        &Request::Advertisement,
    )?;
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Header {
        bytes: usize,
    }
    let header: Header = recv(&mut session.bearer, &mut session.channel)?;
    if header.bytes == 0 || header.bytes > MAX_AD_BYTES {
        return Err(err("invalid advertisement length"));
    }
    let mut bytes = Vec::with_capacity(header.bytes);
    while bytes.len() < header.bytes {
        let chunk = receive_bytes(&mut session.bearer, &mut session.channel, CHUNK)?;
        if chunk.is_empty() || chunk.len() > header.bytes - bytes.len() {
            return Err(err("invalid advertisement chunk"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let obj = Object::from_bytes(&bytes).map_err(err)?;
    if obj.author_human != peer {
        return Err(err(
            "advertisement author differs from authenticated contact",
        ));
    }
    let identity = crate::identity::load(home)?;
    let cache = crate::store::build_kel_cache(home, &identity)?;
    let (root, device) = trusted(&cache, &obj)?;
    local_catalog(home)?
        .replace(&obj, root, device, now())
        .map_err(err)
}
fn selected_contacts(home: &Path, args: &mut Vec<String>) -> Result<Vec<(String, Contact)>> {
    let all = contacts(home)?;
    if let Some(name) = take(args, "--contact")? {
        Ok(vec![(
            name.clone(),
            all.get(&name)
                .ok_or_else(|| err("unknown seed contact"))?
                .clone(),
        )])
    } else {
        if all.len() > 16 {
            return Err(err(
                "select --contact when more than16 contacts are configured",
            ));
        }
        Ok(all.into_iter().collect())
    }
}
fn refresh(home: &Path, selected: &[(String, Contact)]) -> Value {
    let mut refreshed = Vec::new();
    let mut failures = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(120);
    for (name, c) in selected {
        match pull_ad(home,c,deadline){Ok(ad)=>refreshed.push(json!({"contact":name,"provider":ad.provider,"objects":ad.entries.len(),"expires_at_ms":ad.expires_at_ms})),Err(e)=>failures.push(json!({"contact":name,"error":e.to_string()}))}
    }
    json!({"refreshed":refreshed,"failures":failures,"complete":failures.is_empty(),"availability_proven":false})
}
fn current_ads(home: &Path) -> Result<(Vec<Advertisement>, Vec<Value>)> {
    let identity = crate::identity::load(home)?;
    let cache = crate::store::build_kel_cache(home, &identity)?;
    let mut ads = Vec::new();
    let mut rejected = Vec::new();
    for obj in local_catalog(home)?.objects(now()).map_err(err)? {
        match trusted(&cache, &obj)
            .and_then(|(root, device)| catalog::verify(&obj, root, device, now()).map_err(err))
        {
            Ok(mut ad) => {
                ad.entries.retain(|e| e.expires_at_ms > now());
                ads.push(ad);
            }
            Err(e) => {
                rejected.push(json!({"provider":obj.author_human.as_str(),"error":e.to_string()}))
            }
        }
    }
    Ok((ads, rejected))
}
fn candidates(home: &Path, id: &str) -> Result<Vec<(String, Contact, Advertisement)>> {
    ObjectId::parse(id).map_err(err)?;
    let (mut ads, _) = current_ads(home)?;
    ads.retain(|a| a.entries.iter().any(|e| e.id == id));
    let mut results = Vec::new();
    for (name, c) in contacts(home)? {
        if let Some(ad) = ads.iter().find(|a| a.provider == c.peer) {
            results.push((name, c, ad.clone()));
        }
    }
    if results.len() > 16 {
        return Err(err(
            "more than16 matching endpoints; remove stale contacts before retrieval",
        ));
    }
    Ok(results)
}
fn retrieve(home: &Path, store: &Path, id: &str, days: Option<u16>) -> Result<Value> {
    let mut failures = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(120);
    for (name, c, _ad) in candidates(home, id)? {
        let peer = Did::parse(&c.peer).map_err(err)?;
        match fetch(
            home,
            store,
            &c.address,
            &peer,
            id,
            FetchOptions {
                max_micro: 0,
                payment: None,
                days,
                deadline: Some(deadline),
            },
        ) {
            Ok(result) => {
                return Ok(
                    json!({"retrieved":true,"contact":name,"delivery":result,"failures":failures}),
                )
            }
            Err(e) => failures.push(json!({"contact":name,"error":e.to_string()})),
        }
    }
    Ok(
        json!({"retrieved":false,"failures":failures,"reason":"No configured advertised provider completed a verified free transfer. Refresh the catalog or use explicit paid fetch.","availability_proven":false}),
    )
}
pub(super) fn dispatch(
    home: &Path,
    store: &Path,
    verb: &str,
    mut args: Vec<String>,
) -> Result<Value> {
    match verb {
        "search" => search::run(home, store, args),
        "contact" => {
            if args.is_empty() {
                return Err(err("contact add | list | remove"));
            }
            let action = args.remove(0);
            if action == "list" {
                done(&args)?;
                return Ok(json!({"contacts":contacts(home)?,"identity_trust_granted":false}));
            }
            let name = need(&mut args, "--name")?;
            if !valid_name(&name) {
                return Err(err("invalid contact name"));
            }
            let contact = if action == "add" {
                let a = address(&need(&mut args, "--addr")?)?;
                let peer = Did::parse(&need(&mut args, "--peer")?).map_err(err)?;
                let identity = crate::identity::load(home)?;
                let cache = crate::store::build_kel_cache(home, &identity)?;
                let root = cache
                    .get(&peer)
                    .ok_or_else(|| err("trust this peer's KEL before activating its endpoint"))?;
                if root.did() != peer || root.delegator().is_some() {
                    return Err(err("peer KEL mismatch"));
                }
                Some(Contact {
                    address: a,
                    peer: peer.to_string(),
                })
            } else if action == "remove" {
                None
            } else {
                return Err(err("unknown contact action"));
            };
            done(&args)?;
            safe_dir(home)?;
            let _lock =
                mini_durable::lock_exclusive(&home.join("seed-contacts.lock")).map_err(err)?;
            let mut all = contacts(home)?;
            if let Some(contact) = contact {
                if all.contains_key(&name) {
                    return Err(err(
                        "contact exists; explicitly remove it before changing endpoints",
                    ));
                }
                if all.len() >= MAX_CONTACTS
                    || all
                        .values()
                        .any(|c| c.peer == contact.peer && c.address == contact.address)
                {
                    return Err(err("contact duplicate or limit exceeded"));
                }
                all.insert(name.clone(), contact);
            } else {
                all.remove(&name);
            }
            let bytes = serde_json::to_vec(&all).map_err(err)?;
            if bytes.len() > MAX_CONTACT_BYTES {
                return Err(err("contacts exceed bound"));
            }
            mini_durable::atomic_replace(&home.join("seed-contacts.json"), &bytes).map_err(err)?;
            Ok(json!({"contact":name,"action":action,"identity_trust_granted":false}))
        }
        "announce" => {
            let a = address(&need(&mut args, "--addr")?)?;
            let ttl = number(&need(&mut args, "--ttl-ms")?)?;
            done(&args)?;
            if !(1000..=900_000).contains(&ttl) {
                return Err(err("ttl-ms must be1000..900000"));
            }
            crate::identity::load(home)?;
            safe_dir(home)?;
            let _lock =
                mini_durable::lock_exclusive(&home.join("seed-announcement.lock")).map_err(err)?;
            let a = Announcement {
                address: a,
                ttl_ms: ttl,
            };
            mini_durable::atomic_replace(
                &home.join("seed-announcement.json"),
                &serde_json::to_vec(&a).map_err(err)?,
            )
            .map_err(err)?;
            Ok(
                json!({"announcement":a,"automatic_networking":false,"note":"Selected seeded object IDs and this endpoint may now be advertised to allowed peers on request."}),
            )
        }
        "unannounce" => {
            done(&args)?;
            safe_dir(home)?;
            let _lock =
                mini_durable::lock_exclusive(&home.join("seed-announcement.lock")).map_err(err)?;
            let path = home.join("seed-announcement.json");
            if path.try_exists().map_err(err)? {
                if path.is_symlink() {
                    return Err(err("announcement cannot be a symlink"));
                }
                fs::remove_file(path).map_err(err)?;
                mini_durable::sync_directory(home).map_err(err)?;
            }
            Ok(
                json!({"announcing":false,"note":"Previously signed hints remain valid until their bounded expiry; new serving always checks live leases."}),
            )
        }
        "catalog" => {
            if args.is_empty() {
                return Err(err("catalog refresh | list | remove"));
            }
            let action = args.remove(0);
            match action.as_str() {
                "pull" => {
                    let selected = selected_contacts(home, &mut args)?;
                    done(&args)?;
                    let deadline = Instant::now() + Duration::from_secs(120);
                    let mut reports = Vec::new();
                    let mut failures = Vec::new();
                    for (name, c) in selected {
                        match pull_catalog(home, &c, deadline) {
                            Ok(r) => reports.push(json!({"contact":name,"report":r})),
                            Err(e) => failures.push(json!({"contact":name,"error":e.to_string()})),
                        }
                    }
                    Ok(
                        json!({"reports":reports,"failures":failures,"complete":failures.is_empty(),"endpoints_activated":0}),
                    )
                }
                "import" => {
                    let path = need(&mut args, "--file")?;
                    done(&args)?;
                    let obj = Object::from_bytes(&read_file(Path::new(&path), MAX_AD_BYTES)?)
                        .map_err(err)?;
                    let ad = accept(home, &obj)?;
                    Ok(json!({"provider":ad.provider,"endpoints_activated":0}))
                }
                "export" => {
                    let peer = need(&mut args, "--peer")?;
                    Did::parse(&peer).map_err(err)?;
                    let output = need(&mut args, "--output")?;
                    done(&args)?;
                    let obj = local_catalog(home)?
                        .objects(now())
                        .map_err(err)?
                        .into_iter()
                        .find(|o| o.author_human.as_str() == peer)
                        .ok_or_else(|| err("active provider advertisement missing"))?;
                    let identity = crate::identity::load(home)?;
                    let cache = crate::store::build_kel_cache(home, &identity)?;
                    let (root, device) = trusted(&cache, &obj)?;
                    catalog::verify(&obj, root, device, now()).map_err(err)?;
                    let mut f = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&output)
                        .map_err(err)?;
                    f.write_all(&obj.to_bytes())
                        .and_then(|_| f.sync_all())
                        .map_err(err)?;
                    mini_durable::sync_parent(Path::new(&output)).map_err(err)?;
                    Ok(json!({"output":output,"provider":peer}))
                }
                "refresh" => {
                    let selected = selected_contacts(home, &mut args)?;
                    done(&args)?;
                    Ok(refresh(home, &selected))
                }
                "list" => {
                    done(&args)?;
                    let (ads, rejected) = current_ads(home)?;
                    Ok(
                        json!({"providers":ads,"rejected":rejected,"availability_proven":false,"canonical_registry":false}),
                    )
                }
                "remove" => {
                    let peer = need(&mut args, "--peer")?;
                    done(&args)?;
                    Ok(
                        json!({"removed":local_catalog(home)?.remove(&peer).map_err(err)?,"identity_trust_revoked":false}),
                    )
                }
                _ => Err(err("unknown catalog action")),
            }
        }
        "locate" => {
            let id = need(&mut args, "--id")?;
            done(&args)?;
            let results:Vec<_>=candidates(home,&id)?.into_iter().map(|(name,c,ad)|json!({"contact":name,"provider":c.peer,"active_endpoint":c.address,"advertised_endpoint":ad.address,"hint_expires_at_ms":ad.expires_at_ms})).collect();
            Ok(
                json!({"object_id":id,"candidates":results,"availability_proven":false,"paid_ranking":false}),
            )
        }
        "retrieve" => {
            let id = need(&mut args, "--id")?;
            let days = take(&mut args, "--reseed-days")?
                .map(|d| number(&d).and_then(|n| u16::try_from(n).map_err(err)))
                .transpose()?;
            if days.is_some_and(|d| !(1..=3650).contains(&d)) {
                return Err(err("reseed-days must be1..3650"));
            }
            done(&args)?;
            retrieve(home, store, &id, days)
        }
        _ => Err(err("unknown mesh command")),
    }
}

const MAX_EXCHANGE: usize = 8 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogHeader {
    count: usize,
    total_bytes: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObjectHeader {
    bytes: usize,
}
fn accept(home: &Path, obj: &Object) -> Result<Advertisement> {
    let identity = crate::identity::load(home)?;
    let cache = crate::store::build_kel_cache(home, &identity)?;
    let (root, device) = trusted(&cache, obj)?;
    local_catalog(home)?
        .replace(obj, root, device, now())
        .map_err(err)
}
pub(super) fn serve_catalog(home: &Path, b: &mut TcpBearer, c: &mut Channel) -> Result<()> {
    let identity = crate::identity::load(home)?;
    let cache = crate::store::build_kel_cache(home, &identity)?;
    let mut objects = Vec::new();
    let mut total = 0;
    for obj in local_catalog(home)?.objects(now()).map_err(err)? {
        if trusted(&cache, &obj)
            .and_then(|(root, device)| catalog::verify(&obj, root, device, now()).map_err(err))
            .is_err()
        {
            continue;
        }
        let bytes = obj.to_bytes();
        total += bytes.len();
        if bytes.len() > MAX_AD_BYTES || total > MAX_EXCHANGE {
            return Err(err("catalog exchange exceeds bounds"));
        }
        objects.push(bytes);
    }
    send(
        b,
        c,
        &CatalogHeader {
            count: objects.len(),
            total_bytes: total,
        },
    )?;
    for bytes in objects {
        send(b, c, &ObjectHeader { bytes: bytes.len() })?;
        for chunk in bytes.chunks(CHUNK) {
            send_bytes(b, c, chunk)?;
        }
    }
    Ok(())
}
fn pull_catalog(home: &Path, contact: &Contact, deadline: Instant) -> Result<Value> {
    let peer = Did::parse(&contact.peer).map_err(err)?;
    let mut s = connect_until(home, &contact.address, &peer, deadline)?;
    send(&mut s.bearer, &mut s.channel, &Request::Catalog)?;
    let header: CatalogHeader = recv(&mut s.bearer, &mut s.channel)?;
    if header.count > 64 || header.total_bytes > MAX_EXCHANGE {
        return Err(err("invalid catalog exchange bounds"));
    }
    let mut received = 0;
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    let mut seen = BTreeSet::new();
    for _ in 0..header.count {
        let object: ObjectHeader = recv(&mut s.bearer, &mut s.channel)?;
        if object.bytes == 0
            || object.bytes > MAX_AD_BYTES
            || object.bytes > header.total_bytes - received
        {
            return Err(err("invalid catalog object size"));
        }
        let mut bytes = Vec::with_capacity(object.bytes);
        while bytes.len() < object.bytes {
            let chunk = receive_bytes(&mut s.bearer, &mut s.channel, CHUNK)?;
            if chunk.is_empty() || chunk.len() > object.bytes - bytes.len() {
                return Err(err("invalid catalog chunk"));
            }
            bytes.extend_from_slice(&chunk);
        }
        received += bytes.len();
        let result = Object::from_bytes(&bytes).map_err(err).and_then(|o| {
            if !seen.insert(o.author_human.as_str().to_owned()) {
                return Err(err("duplicate provider snapshot"));
            }
            accept(home, &o)
        });
        match result {
            Ok(ad) => accepted.push(ad.provider),
            Err(e) => rejected.push(e.to_string()),
        }
    }
    if received != header.total_bytes {
        return Err(err("catalog byte count mismatch"));
    }
    Ok(
        json!({"accepted":accepted,"rejected":rejected,"endpoints_activated":0,"availability_proven":false}),
    )
}
