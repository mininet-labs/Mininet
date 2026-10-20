//! Verifiable literal search of explicitly seeded public objects, not web ranking.
use super::*;
const SCAN_BYTES: u64 = 16 * 1024 * 1024;
const MAX_HITS: u16 = 8;
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    query: String,
    count: usize,
    complete: bool,
    next_after: Option<String>,
    scanned_bytes: u64,
}
fn query_valid(q: &str) -> Result<()> {
    if q.trim().is_empty() || q.len() > 256 || q.chars().any(char::is_control) {
        return Err(err(
            "search query must be1..256 bytes without control characters",
        ));
    }
    Ok(())
}
pub(crate) fn serve(
    home: &Path,
    b: &mut TcpBearer,
    c: &mut Channel,
    query: &str,
    after: Option<&str>,
    limit: u16,
) -> Result<()> {
    query_valid(query)?;
    if price(home)?.micro_per_mib > 0 {
        return Err(err("verified search sends whole small source objects; paid providers require explicit paid fetch instead"));
    }
    if !(1..=MAX_HITS).contains(&limit) {
        return Err(err("search limit must be1..8"));
    }
    if let Some(after) = after {
        ObjectId::parse(after).map_err(err)?;
    }
    let pool = pool(home)?;
    let leases: Vec<_> = pool
        .leases()
        .map_err(err)?
        .into_iter()
        .filter(|l| l.expires_at_ms > now() && after.is_none_or(|a| l.id.as_str() > a))
        .collect();
    let mut selected = Vec::new();
    let mut reserved = 0;
    let mut budget_complete = true;
    for lease in &leases {
        if lease.bytes > CHUNK as u64 {
            continue;
        }
        if reserved + lease.bytes > SCAN_BYTES {
            budget_complete = false;
            break;
        }
        reserved += lease.bytes;
        selected.push(lease.id.clone());
    }
    let objects = pool.read_batch(&selected, now(), SCAN_BYTES).map_err(err)?;
    let scanned = objects.iter().map(|o| o.len() as u64).sum();
    let mut found = Vec::new();
    let mut cursor = after.map(str::to_string);
    let mut complete = budget_complete;
    for (id, bytes) in selected.into_iter().zip(objects) {
        cursor = Some(id);
        let obj = Object::from_bytes(&bytes).map_err(err)?;
        if let mini_objects::Payload::Public(payload) = &obj.payload {
            if std::str::from_utf8(payload).is_ok_and(|text| text.contains(query)) {
                found.push(bytes);
            }
        }
        if found.len() == limit as usize {
            complete = false;
            break;
        }
    }
    send(
        b,
        c,
        &Page {
            query: query.into(),
            count: found.len(),
            complete,
            next_after: if complete { None } else { cursor },
            scanned_bytes: scanned,
        },
    )?;
    for bytes in found {
        send_bytes(b, c, &bytes)?;
    }
    Ok(())
}
fn excerpt(text: &str, query: &str) -> String {
    let byte = text.find(query).unwrap_or(0);
    text[byte..]
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(512)
        .collect()
}
fn ask(
    home: &Path,
    store: &Path,
    contact: &Contact,
    query: &str,
    after: Option<String>,
    limit: u16,
    deadline: Instant,
) -> Result<(Page, Vec<Value>)> {
    let peer = Did::parse(&contact.peer).map_err(err)?;
    let mut s = connect_until(home, &contact.address, &peer, deadline)?;
    send(
        &mut s.bearer,
        &mut s.channel,
        &Request::Search {
            query: query.into(),
            after: after.clone(),
            limit,
        },
    )?;
    let page: Page = recv(&mut s.bearer, &mut s.channel)?;
    if page.query != query
        || page.count > limit as usize
        || page.scanned_bytes > SCAN_BYTES
        || (page.complete && page.next_after.is_some())
    {
        return Err(err("invalid search response bounds"));
    }
    if !page.complete {
        let next = page
            .next_after
            .as_ref()
            .ok_or_else(|| err("incomplete search lacks continuation"))?;
        ObjectId::parse(next).map_err(err)?;
        if after.as_ref().is_some_and(|a| next <= a) {
            return Err(err("search continuation made no progress"));
        }
    }
    let mut hits = Vec::new();
    let mut seen = BTreeSet::new();
    for _ in 0..page.count {
        let bytes = receive_bytes(&mut s.bearer, &mut s.channel, CHUNK)?;
        let obj = Object::from_bytes(&bytes).map_err(err)?;
        if !seen.insert(obj.id().as_str().to_string())
            || after
                .as_ref()
                .is_some_and(|a| obj.id().as_str() <= a.as_str())
        {
            return Err(err("duplicate/out-of-range search object"));
        }
        if page
            .next_after
            .as_ref()
            .is_some_and(|end| obj.id().as_str() > end.as_str())
        {
            return Err(err("search object exceeds continuation"));
        }
        ingest(home, store, &obj)?;
        let mini_objects::Payload::Public(payload) = &obj.payload else {
            return Err(err("private search result rejected"));
        };
        let text = std::str::from_utf8(payload).map_err(err)?;
        if !text.contains(query) {
            return Err(err("provider result does not match literal query"));
        }
        hits.push(json!({"object_id":obj.id().as_str(),"author":obj.author_human.as_str(),"excerpt":excerpt(text,query),"content_verified":true,"provider":contact.peer}));
    }
    Ok((page, hits))
}
pub(super) fn run(home: &Path, store: &Path, mut args: Vec<String>) -> Result<Value> {
    let query = need(&mut args, "--query")?;
    query_valid(&query)?;
    let selected = selected_contacts(home, &mut args)?;
    let after = take(&mut args, "--after")?;
    if after.is_some() && selected.len() != 1 {
        return Err(err("continuation requires exactly one --contact"));
    }
    if let Some(after) = &after {
        ObjectId::parse(after).map_err(err)?;
    }
    let limit = take(&mut args, "--limit")?
        .map(|s| number(&s))
        .transpose()?
        .unwrap_or(8);
    if !(1..=MAX_HITS as u64).contains(&limit) {
        return Err(err("limit must be1..8 per provider"));
    }
    done(&args)?;
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut results: BTreeMap<String, Value> = BTreeMap::new();
    let mut failures = Vec::new();
    let mut continuations = Vec::new();
    for (name, c) in selected {
        match ask(
            home,
            store,
            &c,
            &query,
            after.clone(),
            limit as u16,
            deadline,
        ) {
            Ok((page, hits)) => {
                for mut hit in hits {
                    let id = hit["object_id"]
                        .as_str()
                        .ok_or_else(|| err("missing result id"))?
                        .to_string();
                    if let Some(old) = results.get_mut(&id) {
                        old["contacts"]
                            .as_array_mut()
                            .ok_or_else(|| err("invalid contact list"))?
                            .push(json!(name));
                    } else {
                        hit["contacts"] = json!([name]);
                        results.insert(id, hit);
                    }
                }
                if !page.complete {
                    continuations.push(json!({"contact":name,"after":page.next_after}));
                }
            }
            Err(e) => failures.push(json!({"contact":name,"error":e.to_string()})),
        }
    }
    Ok(
        json!({"query":query,"results":results.into_values().collect::<Vec<_>>(),"complete":failures.is_empty() && continuations.is_empty(),"continuations":continuations,"failures":failures,"ordering":"object-id","paid_ranking":false,"query_disclosed_to_contacts":true,"note":"Literal search over public seeded UTF-8 objects <=64KiB each, not a complete web index. Only received signed hits are verified; providers may omit results."}),
    )
}
