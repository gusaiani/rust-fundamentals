//! RESP — the Redis Serialization Protocol (the wire format).
//!
//! The [`Resp`] enum and the small constructors are **given**; the **parser**
//! ([`Resp::parse`], step 1) and the **encoder** ([`Resp::encode`], step 2) are
//! yours. They are the heart of the module: framing a byte stream into messages
//! and back (Pills 5 & 6).
//!
//! RESP is a simple, prefix-tagged, CRLF-framed protocol. The first byte of
//! every value says what it is; `\r\n` ("CRLF") terminates the framing lines:
//!
//! ```text
//! +OK\r\n                      Simple String   (status replies)
//! -ERR unknown command\r\n     Error           (error replies)
//! :1000\r\n                    Integer
//! $5\r\nhello\r\n              Bulk String      ($<len>CRLF<bytes>CRLF)
//! $-1\r\n                      Null Bulk        (the "nil" reply)
//! *2\r\n$3\r\nGET\r\n$3\r\nfoo\r\n   Array       (*<count>CRLF then <count> values)
//! ```
//!
//! A client *request* is always an Array of Bulk Strings — e.g. `SET foo bar`
//! arrives as `*3\r\n$3\r\nSET\r\n$3\r\nfoo\r\n$3\r\nbar\r\n`. Replies use the
//! whole vocabulary above.

use crate::{Error, Result};

/// A single RESP value — one request or one reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resp {
    /// `+OK\r\n` — a short status line, no length prefix.
    Simple(String),
    /// `-ERR ...\r\n` — an error status line.
    Error(String),
    /// `:42\r\n` — a signed 64-bit integer.
    Integer(i64),
    /// `$5\r\nhello\r\n` — a length-prefixed binary-safe string.
    Bulk(Vec<u8>),
    /// `$-1\r\n` — the null / "nil" reply (a key that doesn't exist).
    Null,
    /// `*N\r\n...` — an array of `N` values.
    Array(Vec<Resp>),
}

impl Resp {
    /// `+OK\r\n`.
    pub fn ok() -> Resp {
        Resp::Simple("OK".to_string())
    }

    /// An error reply, `-<msg>\r\n`.
    pub fn error(msg: impl Into<String>) -> Resp {
        Resp::Error(msg.into())
    }

    /// A bulk string from any byte source.
    pub fn bulk(bytes: impl Into<Vec<u8>>) -> Resp {
        Resp::Bulk(bytes.into())
    }

    fn find_crlf(buf: &[u8]) -> Option<usize> {
        buf.windows(2).position(|pair| pair == b"\r\n")
    }

    /// Try to parse **one** RESP value from the front of `buf`.
    ///
    /// This is the framing primitive that makes buffered, non-blocking reads
    /// work (Pill 6). Three outcomes:
    ///   - `Ok(Some((value, consumed)))` — a complete value was parsed;
    ///     `consumed` bytes should be removed from the caller's buffer.
    ///   - `Ok(None)` — the buffer holds a *partial* value; the caller should
    ///     read more bytes and try again. (Not an error — this is normal.)
    ///   - `Err(_)` — the bytes are not valid RESP.
    pub fn parse(buf: &[u8]) -> Result<Option<(Resp, usize)>> {
        if buf.is_empty() {
            return Ok(None);
        }
        let line_end = match Self::find_crlf(buf) {
            Some(i) => i,
            None => return Ok(None), // framing line incomplete - need more bytes
        };
        let line = &buf[1..line_end];
        let after_line = line_end + 2;

        match buf[0] {
            b'+' => {
                let text = std::str::from_utf8(line)
                    .map_err(|_| Error::protocol("invalid UTF-8 in simple string"))?;
                Ok(Some((Resp::Simple(text.to_string()), after_line)))
            }
            b'-' => {
                let text = std::str::from_utf8(line)
                    .map_err(|_| Error::protocol("invalid UTF-8 in error"))?;
                Ok(Some((Resp::Error(text.to_string()), after_line)))
            }
            b':' => {
                // parse the line bytes as ASCII, then as an i64
                let n: i64 = std::str::from_utf8(line)
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| Error::protocol("invalid integer"))?;
                Ok(Some((Resp::Integer(n), after_line)))
            }
            b'$' => {
                let n: i64 = std::str::from_utf8(line)
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| Error::protocol("invalid bulk length"))?;
                if n < 0 {
                    return Ok(Some((Resp::Null, after_line))); // $-1\r\n is the nil reply
                }
                let n = n as usize;
                let value_end = after_line + n;
                if buf.len() < value_end + 2 {
                    return Ok(None);
                }
                let value = buf[after_line..value_end].to_vec();
                Ok(Some((Resp::Bulk(value), value_end + 2)))
            }
            b'*' => {
                let count: i64 = std::str::from_utf8(line)
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| Error::protocol("invalid array count"))?;
                if count < 0 {
                    return Ok(Some((Resp::Null, after_line)));
                }
                let mut consumed = after_line;
                let mut items = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    match Resp::parse(&buf[consumed..])? {
                        Some((value, used)) => {
                            items.push(value);
                            consumed += used;
                        }
                        None => return Ok(None),
                    }
                }
                Ok(Some((Resp::Array(items), consumed)))
            }
            other => Err(Error::protocol(format!("bad type byte: {other:#x}"))),
        }
    }

    /// Append the wire encoding of `self` to `out`.
    ///
    /// The inverse of [`Resp::parse`]. Writing into a caller-owned buffer (not
    /// returning a `Vec`) is deliberate: replies accumulate in the connection's
    /// write buffer (Pill 7), so we append rather than allocate per value.
    pub fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Resp::Simple(s) => {
                out.push(b'+');
                out.extend_from_slice(s.as_bytes());
                out.extend_from_slice(b"\r\n");
            }
            Resp::Error(s) => {
                out.push(b'-');
                out.extend_from_slice(s.as_bytes());
                out.extend_from_slice(b"\r\n");
            }
            Resp::Integer(i) => {
                out.push(b':');
                out.extend_from_slice(i.to_string().as_bytes());
                out.extend_from_slice(b"\r\n");
            }
            Resp::Bulk(b) => {
                out.push(b'$');
                out.extend_from_slice(b.len().to_string().as_bytes());
                out.extend_from_slice(b"\r\n");
                out.extend_from_slice(b);
                out.extend_from_slice(b"\r\n");
            }
            Resp::Null => {
                out.extend_from_slice(b"$-1\r\n");
            }
            Resp::Array(v) => {
                out.push(b'*');
                out.extend_from_slice(v.len().to_string().as_bytes());
                out.extend_from_slice(b"\r\n");
                for item in v {
                    item.encode(out);
                }
            }
        }
    }
}
