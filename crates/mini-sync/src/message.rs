//! MINI/SYNC1 wire messages: a tiny tagged codec, bounded before allocation.

use crate::{Result, SyncError};

/// AAD binding every sync frame to this protocol.
pub(crate) const SYNC_AAD: &[u8] = b"MINI/SYNC1";

const MAX_IDS_PER_MSG: usize = 4096;
const MAX_OBJECTS_PER_MSG: usize = 64;
const MAX_ID_BYTES: usize = 128;
const MAX_OBJECT_BYTES: usize = 9 * 1024 * 1024; // envelope cap + headroom
const MAX_BUCKETS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Msg {
    /// Digest of the client's whole id set.
    RootDigest([u8; 32]),
    /// Per-bucket digests of the server's set.
    BucketDigests(Vec<(u8, [u8; 32])>),
    /// Buckets the client needs listed.
    NeedBuckets(Vec<u8>),
    /// Ids in the requested buckets.
    Ids(Vec<String>),
    /// Ids the client wants transferred.
    Want(Vec<String>),
    /// A batch of full object bytes.
    Objects(Vec<Vec<u8>>),
    /// End of this pull.
    Done,
}

const T_ROOT: u8 = 1;
const T_BUCKETS: u8 = 2;
const T_NEED: u8 = 3;
const T_IDS: u8 = 4;
const T_WANT: u8 = 5;
const T_OBJECTS: u8 = 6;
const T_DONE: u8 = 7;

impl Msg {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut w: Vec<u8> = Vec::new();
        match self {
            Msg::RootDigest(d) => {
                w.push(T_ROOT);
                w.extend_from_slice(d);
            }
            Msg::BucketDigests(list) => {
                w.push(T_BUCKETS);
                w.extend_from_slice(&(list.len() as u32).to_be_bytes());
                for (b, d) in list {
                    w.push(*b);
                    w.extend_from_slice(d);
                }
            }
            Msg::NeedBuckets(list) => {
                w.push(T_NEED);
                w.extend_from_slice(&(list.len() as u32).to_be_bytes());
                w.extend_from_slice(list);
            }
            Msg::Ids(ids) => encode_strings(&mut w, T_IDS, ids),
            Msg::Want(ids) => encode_strings(&mut w, T_WANT, ids),
            Msg::Objects(objs) => {
                w.push(T_OBJECTS);
                w.extend_from_slice(&(objs.len() as u32).to_be_bytes());
                for o in objs {
                    w.extend_from_slice(&(o.len() as u32).to_be_bytes());
                    w.extend_from_slice(o);
                }
            }
            Msg::Done => w.push(T_DONE),
        }
        w
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Msg> {
        let mut r = Cursor { b: bytes, p: 0 };
        let tag = r.u8()?;
        let msg = match tag {
            T_ROOT => Msg::RootDigest(r.arr32()?),
            T_BUCKETS => {
                let n = r.u32()? as usize;
                if n > MAX_BUCKETS {
                    return Err(SyncError::LimitExceeded);
                }
                let mut list = Vec::with_capacity(n);
                for _ in 0..n {
                    let b = r.u8()?;
                    list.push((b, r.arr32()?));
                }
                Msg::BucketDigests(list)
            }
            T_NEED => {
                let n = r.u32()? as usize;
                if n > MAX_BUCKETS {
                    return Err(SyncError::LimitExceeded);
                }
                Msg::NeedBuckets(r.take(n)?.to_vec())
            }
            T_IDS => Msg::Ids(decode_strings(&mut r)?),
            T_WANT => Msg::Want(decode_strings(&mut r)?),
            T_OBJECTS => {
                let n = r.u32()? as usize;
                if n > MAX_OBJECTS_PER_MSG {
                    return Err(SyncError::LimitExceeded);
                }
                let mut objs = Vec::with_capacity(n);
                for _ in 0..n {
                    let len = r.u32()? as usize;
                    if len > MAX_OBJECT_BYTES {
                        return Err(SyncError::LimitExceeded);
                    }
                    objs.push(r.take(len)?.to_vec());
                }
                Msg::Objects(objs)
            }
            T_DONE => Msg::Done,
            _ => return Err(SyncError::Protocol),
        };
        if !r.finished() {
            return Err(SyncError::Protocol);
        }
        Ok(msg)
    }
}

fn encode_strings(w: &mut Vec<u8>, tag: u8, ids: &[String]) {
    w.push(tag);
    w.extend_from_slice(&(ids.len() as u32).to_be_bytes());
    for id in ids {
        w.extend_from_slice(&(id.len() as u32).to_be_bytes());
        w.extend_from_slice(id.as_bytes());
    }
}

fn decode_strings(r: &mut Cursor<'_>) -> Result<Vec<String>> {
    let n = r.u32()? as usize;
    if n > MAX_IDS_PER_MSG {
        return Err(SyncError::LimitExceeded);
    }
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let len = r.u32()? as usize;
        if len > MAX_ID_BYTES {
            return Err(SyncError::LimitExceeded);
        }
        let s = String::from_utf8(r.take(len)?.to_vec()).map_err(|_| SyncError::Protocol)?;
        out.push(s);
    }
    Ok(out)
}

struct Cursor<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.p + n > self.b.len() {
            return Err(SyncError::Protocol);
        }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn arr32(&mut self) -> Result<[u8; 32]> {
        let b = self.take(32)?;
        let mut a = [0u8; 32];
        a.copy_from_slice(b);
        Ok(a)
    }
    fn finished(&self) -> bool {
        self.p == self.b.len()
    }
}
