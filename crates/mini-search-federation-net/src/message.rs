//! The tiny MINI/SEARCHFED-ADV1 advertisement exchange: one bounded
//! request/response pair, tagged and length-prefixed like `mini-sync`'s own
//! wire messages. This is deliberately the only thing that crosses the wire
//! before the generic `mini_sync::request_retrieval`/`serve_retrieval`
//! exchange takes over -- no query terms, no ranking profile, no free text
//! of any kind. A peer states which object ids it is willing to serve;
//! that's the entire vocabulary.

use crate::error::{NetError, Result};

/// Hard ceiling on how many ids either side will encode in one advertisement
/// message. Mirrors `mini-sync`'s own `MAX_RETRIEVAL_OBJECTS` order of
/// magnitude -- an advertisement is a prelude to a retrieval, so it should
/// never offer more than a retrieval could ever pull in one exchange.
pub const MAX_ADVERTISE_IDS: usize = 4096;
const MAX_ID_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Msg {
    /// Client asks for up to `max_ids` object ids the peer is willing to
    /// serve from its F1/F2 candidate set.
    AdvertiseRequest { max_ids: u32 },
    /// Server's bounded answer.
    AdvertiseResponse { ids: Vec<String> },
}

const T_REQUEST: u8 = 1;
const T_RESPONSE: u8 = 2;

impl Msg {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut w = Vec::new();
        match self {
            Msg::AdvertiseRequest { max_ids } => {
                w.push(T_REQUEST);
                w.extend_from_slice(&max_ids.to_be_bytes());
            }
            Msg::AdvertiseResponse { ids } => {
                w.push(T_RESPONSE);
                w.extend_from_slice(&(ids.len() as u32).to_be_bytes());
                for id in ids {
                    w.extend_from_slice(&(id.len() as u32).to_be_bytes());
                    w.extend_from_slice(id.as_bytes());
                }
            }
        }
        w
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Msg> {
        let mut c = Cursor { b: bytes, p: 0 };
        let tag = c.u8()?;
        let msg = match tag {
            T_REQUEST => Msg::AdvertiseRequest { max_ids: c.u32()? },
            T_RESPONSE => {
                let n = c.u32()? as usize;
                if n > MAX_ADVERTISE_IDS {
                    return Err(NetError::LimitExceeded);
                }
                let mut ids = Vec::with_capacity(n);
                for _ in 0..n {
                    let len = c.u32()? as usize;
                    if len > MAX_ID_BYTES {
                        return Err(NetError::LimitExceeded);
                    }
                    let s =
                        String::from_utf8(c.take(len)?.to_vec()).map_err(|_| NetError::Protocol)?;
                    ids.push(s);
                }
                Msg::AdvertiseResponse { ids }
            }
            _ => return Err(NetError::Protocol),
        };
        if !c.finished() {
            return Err(NetError::Protocol);
        }
        Ok(msg)
    }
}

struct Cursor<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.p + n > self.b.len() {
            return Err(NetError::Protocol);
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
    fn finished(&self) -> bool {
        self.p == self.b.len()
    }
}
