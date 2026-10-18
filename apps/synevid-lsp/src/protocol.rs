//! LSP JSON-RPC framing (Tier 3.2).
//!
//! The Language Server Protocol runs JSON-RPC 2.0 messages framed with an HTTP-ish
//! header block: `Content-Length: <n>\r\n\r\n<n bytes of JSON>`. This module is the
//! whole transport — reading, writing, and the three message shapes (request,
//! notification, response) — so the rest of the server never touches bytes.
//!
//! Deliberate choices, mirroring the engine's honesty rules:
//!
//! - **Tolerant reader, strict writer.** Incoming headers accept `\r\n` and bare `\n`
//!   terminators and any additional headers (clients send `Content-Type`); outgoing
//!   frames are byte-canonical `Content-Length: n\r\n\r\n` + body.
//! - **Unknown methods are answered, not ignored.** A request for a method this
//!   server does not implement gets a `-32601 MethodNotFound` error response; a
//!   *notification* for an unknown method is ignored, exactly as the spec requires.
//! - **Malformed JSON is a parse error response when it is attributable** (we read a
//!   complete frame but its body is not JSON), and a hard I/O error when the stream
//!   itself is corrupt — a server that cannot frame its stream cannot answer at all.

use serde_json::{Value, json};
use std::io::{BufRead, Write};

/// JSON-RPC error codes used by this server.
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

/// One decoded incoming message.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    /// A request: carries an `id` the response must echo.
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    /// A notification: no `id`, no response.
    Notification { method: String, params: Value },
}

impl Incoming {
    pub fn method(&self) -> &str {
        match self {
            Incoming::Request { method, .. } | Incoming::Notification { method, .. } => method,
        }
    }
}

/// Reads one `Content-Length`-framed message.
///
/// Returns `Ok(None)` on a clean EOF before any header byte — the normal end of an
/// editor session. Header block and body are read as bytes; the body is UTF-8
/// decoded for JSON parsing (a body that is not UTF-8 is a framing error).
pub fn read_message<R: BufRead>(reader: &mut R) -> std::io::Result<Option<Incoming>> {
    let mut content_length: Option<usize> = None;
    let mut saw_header = false;

    loop {
        let mut line = Vec::new();
        let n = reader.read_until(b'\n', &mut line)?;
        if n == 0 {
            return if saw_header {
                Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "stream ended inside an LSP header block",
                ))
            } else {
                Ok(None)
            };
        }
        let text = String::from_utf8_lossy(&line);
        let trimmed = text.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break; // end of headers
        }
        saw_header = true;
        if let Some((name, value)) = trimmed.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse::<usize>().ok();
        }
        // Other headers (Content-Type etc.) are tolerated and ignored.
    }

    let Some(length) = content_length else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "LSP header block carried no Content-Length",
        ));
    };

    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    let text = String::from_utf8(body).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "LSP body is not UTF-8")
    })?;
    let message: Value = serde_json::from_str(&text).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("LSP body is not JSON: {e}"),
        )
    })?;

    let method = message
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_string);
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    Ok(Some(match (message.get("id").cloned(), method) {
        (Some(id), Some(method)) => Incoming::Request { id, method, params },
        (None, Some(method)) => Incoming::Notification { method, params },
        // A message with neither an id nor a method is not LSP; treat as a
        // notification-shaped no-op so one stray frame cannot kill the session.
        (_, None) => Incoming::Notification {
            method: String::new(),
            params,
        },
    }))
}

/// Writes one framed JSON message with a canonical header block.
pub fn write_message<W: Write>(writer: &mut W, message: &Value) -> std::io::Result<()> {
    let body = serde_json::to_string(message)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    writer.flush()
}

/// A success response echoing the request id.
pub fn response(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// An error response echoing the request id.
pub fn error_response(id: &Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

/// A server-to-client notification.
pub fn notification(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn frame(body: &str) -> String {
        format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
    }

    #[test]
    fn round_trips_request_and_notification() {
        let stream = frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)
            + &frame(r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#);
        let mut cursor = Cursor::new(stream.into_bytes());

        let first = read_message(&mut cursor).unwrap().unwrap();
        assert!(matches!(first, Incoming::Request { .. }));
        assert_eq!(first.method(), "initialize");

        let second = read_message(&mut cursor).unwrap().unwrap();
        assert!(matches!(second, Incoming::Notification { .. }));
        assert_eq!(second.method(), "initialized");

        assert!(read_message(&mut cursor).unwrap().is_none(), "clean EOF");
    }

    #[test]
    fn tolerates_extra_headers_and_bare_lf() {
        let body = r#"{"jsonrpc":"2.0","id":"a-1","method":"x","params":null}"#;
        let stream = format!(
            "Content-Type: application/vscode-jsonrpc; charset=utf-8\nContent-Length: {}\n\n{}",
            body.len(),
            body
        );
        let mut cursor = Cursor::new(stream.into_bytes());
        let msg = read_message(&mut cursor).unwrap().unwrap();
        assert_eq!(msg.method(), "x");
        match msg {
            Incoming::Request { id, .. } => assert_eq!(id, json!("a-1")),
            _ => panic!("expected request"),
        }
    }

    #[test]
    fn refuses_missing_length_header() {
        let stream = "Content-Type: text/plain\r\n\r\n".to_string();
        let mut cursor = Cursor::new(stream.into_bytes());
        assert!(read_message(&mut cursor).is_err());
    }

    #[test]
    fn write_message_is_byte_canonical() {
        let mut out = Vec::new();
        write_message(&mut out, &response(&json!(1), json!({"ok": true}))).unwrap();
        let text = String::from_utf8(out).unwrap();
        let (headers, body) = text.split_once("\r\n\r\n").unwrap();
        assert_eq!(headers, format!("Content-Length: {}", body.len()));
        let parsed: Value = serde_json::from_str(body).unwrap();
        assert_eq!(parsed["result"]["ok"], json!(true));
    }
}
