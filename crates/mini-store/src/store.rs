//! The store: object persistence, deterministic indexes, and head resolution.

use did_mini::Did;
use mini_objects::{Object, ObjectId, ObjectType, Payload};

use crate::backend::Backend;
use crate::{Result, StoreError};

const MAX_SUBJECT_BYTES: usize = 64;

/// Outcome of applying a head pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadState {
    /// The head advanced the slot (it was newer under the convergence rule).
    Applied,
    /// The head was older/equal and was ignored (slot unchanged).
    Stale,
}

/// A content-addressed object store over any [`Backend`].
#[derive(Debug)]
pub struct Store<B: Backend> {
    pub(crate) backend: B,
}

impl<B: Backend> Store<B> {
    /// Wrap a backend.
    pub fn new(backend: B) -> Self {
        Store { backend }
    }

    /// Persist an object and its index rows. Integrity holds by construction
    /// (an [`Object`]'s id is always derived from its bytes); signature and
    /// provenance verification are the ingest pipeline's job (crate docs).
    pub fn insert(&mut self, object: &Object) -> Result<()> {
        let id = object.id().as_str();
        self.backend.put_blob(id, &object.to_bytes())?;
        self.backend.put_meta(&format!("idx/id/{id}"), b"")?;
        self.backend.put_meta(
            &format!("idx/author/{}/{id}", object.author_human.scid()),
            b"",
        )?;
        self.backend.put_meta(
            &format!("idx/type/{}/{id}", type_key(&object.object_type)),
            b"",
        )?;
        for link in &object.links {
            self.backend
                .put_meta(&format!("idx/link/{}/{id}", link.target.as_str()), b"")?;
        }
        Ok(())
    }

    /// Fetch an object by id. The parsed object's derived id must equal the
    /// requested id — a backend can never substitute content (content
    /// addressing is enforced on *every* read, not assumed).
    pub fn get(&self, id: &ObjectId) -> Result<Object> {
        match self.backend.get_blob(id.as_str())? {
            Some(bytes) => {
                let obj = Object::from_bytes(&bytes)?;
                if obj.id().as_str() != id.as_str() {
                    return Err(StoreError::Corrupt);
                }
                Ok(obj)
            }
            None => Err(StoreError::NotFound),
        }
    }

    /// Whether an object is present.
    pub fn contains(&self, id: &ObjectId) -> Result<bool> {
        self.backend.has_blob(id.as_str())
    }

    /// All object ids, key-ordered.
    pub fn all_ids(&self) -> Result<Vec<ObjectId>> {
        self.ids_under("idx/id/")
    }

    /// Ids of objects authored by `human`, key-ordered.
    pub fn by_author(&self, human: &Did) -> Result<Vec<ObjectId>> {
        self.ids_under(&format!("idx/author/{}/", human.scid()))
    }

    /// Ids of objects of `object_type`, key-ordered.
    pub fn by_type(&self, object_type: &ObjectType) -> Result<Vec<ObjectId>> {
        self.ids_under(&format!("idx/type/{}/", type_key(object_type)))
    }

    /// Ids of objects that link to `target`, key-ordered (replies, embeds…).
    pub fn linking_to(&self, target: &ObjectId) -> Result<Vec<ObjectId>> {
        self.ids_under(&format!("idx/link/{}/", target.as_str()))
    }

    fn ids_under(&self, prefix: &str) -> Result<Vec<ObjectId>> {
        let mut out = Vec::new();
        for (key, _) in self.backend.list_meta_prefix(prefix)? {
            let id_str = &key[prefix.len()..];
            out.push(ObjectId::parse(id_str)?);
        }
        Ok(out)
    }

    /// Apply a signed head pointer (SPEC-09 §3): a [`ObjectType::HEAD`] object
    /// whose public payload names the subject and whose single `"target"` link
    /// points at the latest version.
    ///
    /// Convergence rule (deterministic on every replica, any arrival order):
    /// highest `sequence` wins; ties break on the lexicographically greatest
    /// head-object id. The head slot is keyed by *the head's own author* — a
    /// third party's head can never move someone else's state.
    pub fn apply_head(&mut self, head: &Object) -> Result<HeadState> {
        if head.object_type != ObjectType::HEAD {
            return Err(StoreError::BadHead);
        }
        let subject_bytes = match &head.payload {
            Payload::Public(b) => b.clone(),
            Payload::Encrypted(_) => return Err(StoreError::BadHead),
        };
        let subject = String::from_utf8(subject_bytes).map_err(|_| StoreError::BadHead)?;
        if subject.is_empty()
            || subject.len() > MAX_SUBJECT_BYTES
            || !subject
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        {
            return Err(StoreError::BadHead);
        }
        if head.links.len() != 1 || head.links[0].rel != "target" {
            return Err(StoreError::BadHead);
        }

        // The head object itself is stored (it syncs like anything else).
        self.insert(head)?;

        let slot = format!("head/{}/{subject}", head.author_human.scid());
        let candidate = (head.sequence, head.id().as_str().to_string());
        if let Some(existing) = self.backend.get_meta(&slot)? {
            let (cur_seq, cur_id) = decode_slot(&existing)?;
            if (candidate.0, candidate.1.as_str()) <= (cur_seq, cur_id.as_str()) {
                return Ok(HeadState::Stale);
            }
        }
        self.backend
            .put_meta(&slot, &encode_slot(candidate.0, &candidate.1))?;
        Ok(HeadState::Applied)
    }

    /// Resolve the latest target for (`author`, `subject`), if any head applied.
    pub fn resolve_head(&self, author: &Did, subject: &str) -> Result<Option<ObjectId>> {
        let slot = format!("head/{}/{subject}", author.scid());
        match self.backend.get_meta(&slot)? {
            None => Ok(None),
            Some(bytes) => {
                let (_, head_id) = decode_slot(&bytes)?;
                let head = self.get(&ObjectId::parse(&head_id)?)?;
                Ok(Some(head.links[0].target.clone()))
            }
        }
    }

    /// Link targets of `id` that are not yet in the store — what to fetch next.
    pub fn missing_links(&self, id: &ObjectId) -> Result<Vec<ObjectId>> {
        let object = self.get(id)?;
        let mut out = Vec::new();
        for link in object.links {
            if !self.contains(&link.target)? {
                out.push(link.target);
            }
        }
        Ok(out)
    }

    /// Every referenced-but-absent object across the store, deduplicated and
    /// ordered — the seed of a sync want-list (E3).
    pub fn want_list(&self) -> Result<Vec<ObjectId>> {
        let mut out: Vec<ObjectId> = Vec::new();
        for (key, _) in self.backend.list_meta_prefix("idx/link/")? {
            let rest = &key["idx/link/".len()..];
            let target = match rest.split('/').next() {
                Some(t) if !t.is_empty() => t,
                _ => continue,
            };
            if !self.backend.has_blob(target)? {
                let id = ObjectId::parse(target)?;
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        Ok(out)
    }
}

fn type_key(t: &ObjectType) -> String {
    match t {
        ObjectType::WellKnown(tag) => format!("w{tag}"),
        // Custom names may contain '/', so hex-encode them into one key segment.
        ObjectType::Custom(name) => {
            let mut s = String::with_capacity(1 + name.len() * 2);
            s.push('c');
            for b in name.as_bytes() {
                s.push_str(&format!("{b:02x}"));
            }
            s
        }
    }
}

fn encode_slot(sequence: u64, id: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(8 + id.len());
    v.extend_from_slice(&sequence.to_be_bytes());
    v.extend_from_slice(id.as_bytes());
    v
}

fn decode_slot(bytes: &[u8]) -> Result<(u64, String)> {
    if bytes.len() < 8 {
        return Err(StoreError::BadHead);
    }
    let mut seq = [0u8; 8];
    seq.copy_from_slice(&bytes[..8]);
    let id = String::from_utf8(bytes[8..].to_vec()).map_err(|_| StoreError::BadHead)?;
    Ok((u64::from_be_bytes(seq), id))
}
