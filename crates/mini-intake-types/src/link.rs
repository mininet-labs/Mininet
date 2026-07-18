//! [`IntakeLink`]: how a reviewed intake object connects to the rest of
//! Mininet — "linking reviewed material to Mininet objects, issues,
//! audits, research, profiles, posts, or releases" (research report
//! §3). A closed, typed enum rather than a free-form string, so a link
//! target is always structurally distinguishable from another.

use crate::codec::{Reader, Writer};
use crate::error::{IntakeError, Result};
use crate::ids::{read_multihash, write_multihash};
use mini_crypto::Multihash;

const MAX_SLUG_BYTES: usize = 512;

/// A typed reference from an intake object to something else in
/// Mininet. `#[non_exhaustive]` so future link targets can be added
/// without a breaking change.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IntakeLink {
    /// A GitHub-numbered tracking issue.
    Issue(u32),
    /// A content-addressed Mininet object (e.g. a social post, a
    /// profile section).
    Object(Multihash),
    /// A path or slug under `docs/audits/`.
    Audit(String),
    /// A path or slug under `docs/research/`.
    Research(String),
    /// A content-addressed profile reference.
    Profile(Multihash),
    /// A content-addressed post reference.
    Post(Multihash),
    /// A release version string (e.g. a `mini-forge::release::Version`
    /// display form).
    Release(String),
}

impl IntakeLink {
    fn tag(&self) -> u8 {
        match self {
            IntakeLink::Issue(_) => 1,
            IntakeLink::Object(_) => 2,
            IntakeLink::Audit(_) => 3,
            IntakeLink::Research(_) => 4,
            IntakeLink::Profile(_) => 5,
            IntakeLink::Post(_) => 6,
            IntakeLink::Release(_) => 7,
        }
    }

    pub(crate) fn encode(&self, w: &mut Writer) {
        w.u8(self.tag());
        match self {
            IntakeLink::Issue(n) => w.u32(*n),
            IntakeLink::Object(mh) | IntakeLink::Profile(mh) | IntakeLink::Post(mh) => {
                write_multihash(w, mh)
            }
            IntakeLink::Audit(s) | IntakeLink::Research(s) | IntakeLink::Release(s) => w.str(s),
        }
    }

    pub(crate) fn decode(r: &mut Reader) -> Result<Self> {
        match r.u8()? {
            1 => Ok(IntakeLink::Issue(r.u32()?)),
            2 => Ok(IntakeLink::Object(read_multihash(r)?)),
            3 => Ok(IntakeLink::Audit(r.str_limited(MAX_SLUG_BYTES)?)),
            4 => Ok(IntakeLink::Research(r.str_limited(MAX_SLUG_BYTES)?)),
            5 => Ok(IntakeLink::Profile(read_multihash(r)?)),
            6 => Ok(IntakeLink::Post(read_multihash(r)?)),
            7 => Ok(IntakeLink::Release(r.str_limited(MAX_SLUG_BYTES)?)),
            _ => Err(IntakeError::BadIntakeLink),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_crypto::HashAlgorithm;

    fn round_trip(link: IntakeLink) {
        let mut w = Writer::new();
        link.encode(&mut w);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        let decoded = IntakeLink::decode(&mut r).unwrap();
        assert!(r.finished());
        assert_eq!(decoded, link);
    }

    #[test]
    fn every_link_variant_round_trips() {
        round_trip(IntakeLink::Issue(152));
        round_trip(IntakeLink::Object(Multihash::of(
            HashAlgorithm::Blake3,
            b"obj",
        )));
        round_trip(IntakeLink::Audit("issue-152-intake-review".to_string()));
        round_trip(IntakeLink::Research(
            "MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718".to_string(),
        ));
        round_trip(IntakeLink::Profile(Multihash::of(
            HashAlgorithm::Blake3,
            b"profile",
        )));
        round_trip(IntakeLink::Post(Multihash::of(
            HashAlgorithm::Blake3,
            b"post",
        )));
        round_trip(IntakeLink::Release("0.0.1".to_string()));
    }

    #[test]
    fn an_unrecognized_link_tag_is_rejected() {
        let mut r = Reader::new(&[200u8]);
        assert_eq!(IntakeLink::decode(&mut r), Err(IntakeError::BadIntakeLink));
    }
}
