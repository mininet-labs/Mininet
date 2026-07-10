//! Length-delimited, size-bounded framing for the coordinator/runner IPC
//! channel (stdin/stdout or a local socket, per D-0069). Every message is
//! `[u32 big-endian length][payload]`; a declared length over the
//! caller's bound is refused before any allocation, so a hostile or
//! buggy peer can never make the reader allocate an unbounded buffer.

use std::io::{Read, Write};

use crate::error::{ProtocolError, Result};

/// Write one length-delimited frame.
pub fn write_framed<W: Write>(w: &mut W, payload: &[u8]) -> Result<()> {
    let len = u32::try_from(payload.len()).map_err(|_| ProtocolError::MessageTooLarge {
        declared: u32::MAX,
        max: u32::MAX as usize,
    })?;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(payload)?;
    w.flush()?;
    Ok(())
}

/// Read one length-delimited frame, refusing anything over `max_len`
/// bytes. Returns `Ok(None)` on a clean EOF at a message boundary (the
/// peer closed the channel between messages, not mid-message).
pub fn read_framed<R: Read>(r: &mut R, max_len: usize) -> Result<Option<Vec<u8>>> {
    let mut len_bytes = [0u8; 4];
    if !read_exact_or_eof(r, &mut len_bytes)? {
        return Ok(None);
    }
    let len = u32::from_be_bytes(len_bytes) as usize;
    if len > max_len {
        return Err(ProtocolError::MessageTooLarge {
            declared: len as u32,
            max: max_len,
        });
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(Some(buf))
}

/// Like `Read::read_exact`, but returns `Ok(false)` instead of erroring
/// when zero bytes were read before EOF (a clean boundary), and still
/// errors on a *partial* read followed by EOF (a truncated message).
fn read_exact_or_eof<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<bool> {
    let mut filled = 0usize;
    while filled < buf.len() {
        let n = r.read(&mut buf[filled..])?;
        if n == 0 {
            if filled == 0 {
                return Ok(false);
            }
            return Err(ProtocolError::Io("unexpected EOF mid-frame".to_string()));
        }
        filled += n;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn a_message_round_trips() {
        let mut buf = Vec::new();
        write_framed(&mut buf, b"hello").unwrap();
        let mut cursor = Cursor::new(buf);
        let got = read_framed(&mut cursor, 1024).unwrap().unwrap();
        assert_eq!(got, b"hello");
    }

    #[test]
    fn multiple_messages_round_trip_in_order() {
        let mut buf = Vec::new();
        write_framed(&mut buf, b"first").unwrap();
        write_framed(&mut buf, b"second").unwrap();
        let mut cursor = Cursor::new(buf);
        assert_eq!(read_framed(&mut cursor, 1024).unwrap().unwrap(), b"first");
        assert_eq!(read_framed(&mut cursor, 1024).unwrap().unwrap(), b"second");
        assert_eq!(read_framed(&mut cursor, 1024).unwrap(), None);
    }

    #[test]
    fn clean_eof_between_messages_is_none_not_an_error() {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        assert_eq!(read_framed(&mut cursor, 1024).unwrap(), None);
    }

    #[test]
    fn truncated_mid_frame_is_an_error() {
        let mut buf = Vec::new();
        write_framed(&mut buf, b"hello world").unwrap();
        buf.truncate(6); // cut off partway through the payload
        let mut cursor = Cursor::new(buf);
        assert!(read_framed(&mut cursor, 1024).is_err());
    }

    #[test]
    fn a_declared_length_over_the_bound_is_refused_before_reading_payload() {
        let mut buf = Vec::new();
        write_framed(&mut buf, &vec![0u8; 2000]).unwrap();
        let mut cursor = Cursor::new(buf);
        assert!(matches!(
            read_framed(&mut cursor, 1024),
            Err(ProtocolError::MessageTooLarge {
                declared: 2000,
                max: 1024
            })
        ));
    }
}
