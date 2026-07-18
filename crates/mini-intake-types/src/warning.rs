//! [`IntakeWarning`]: a structured note about something an extractor
//! found suspicious or malformed in a source, surfaced to review rather
//! than silently swallowed or silently upgraded into a rejection.

use crate::codec::{Reader, Writer};
use crate::error::Result;

const MAX_CODE_BYTES: usize = 128;
const MAX_MESSAGE_BYTES: usize = 2048;

/// A machine-readable code plus a human-readable message — e.g.
/// `("malformed-pdf-xref", "cross-reference table failed strict
/// validation; recovered via linear scan")`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IntakeWarning {
    pub code: String,
    pub message: String,
}

impl IntakeWarning {
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.str(&self.code);
        w.str(&self.message);
    }

    pub(crate) fn decode(r: &mut Reader) -> Result<Self> {
        let code = r.str_limited(MAX_CODE_BYTES)?;
        let message = r.str_limited(MAX_MESSAGE_BYTES)?;
        Ok(IntakeWarning { code, message })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_warning_round_trips() {
        let warning = IntakeWarning {
            code: "malformed-pdf-xref".to_string(),
            message: "cross-reference table failed strict validation".to_string(),
        };
        let mut w = Writer::new();
        warning.encode(&mut w);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        let decoded = IntakeWarning::decode(&mut r).unwrap();
        assert!(r.finished());
        assert_eq!(decoded, warning);
    }
}
