//! Native journald transport: writes the journal's export format straight to
//! `/run/systemd/journal/socket`.
//!
//! `tracing-journald` builds `MESSAGE` from the event's `message` field alone,
//! so a span chain and every formatting option would never reach the journal.
//! Here the transport is a sink behind the normal renderer instead: the journal
//! receives the exact line stdout receives, plus the structured fields that
//! make it queryable.
//!
//! Large payloads are not routed through a sealed memfd, which is what forces
//! `libc` and `unsafe` in `tracing-journald`. A formatted log line is far below
//! the socket's datagram limit, so a single `send_to` suffices and the whole
//! module stays safe Rust.

use std::io;
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};

use crate::output::EntryMeta;

/// Where systemd listens for native journal entries.
pub(crate) const JOURNALD_PATH: &str = "/run/systemd/journal/socket";

/// Upper bound on one datagram. This is a safety margin, not the real limit:
/// the kernel's `EMSGSIZE` threshold depends on the socket buffer and can be
/// lower, which is why `send_lossy` backs off on that error.
const MAX_ENTRY_BYTES: usize = 128 * 1024;

/// `EMSGSIZE`. Hardcoded because pulling in `libc` for one constant is not
/// worth it, and the stable `ErrorKind` for it varies by target.
const EMSGSIZE: i32 = 90;

pub(crate) struct JournaldSink {
    socket: UnixDatagram,
    identifier: String,
}

impl JournaldSink {
    /// Connect and verify the socket is actually there; `send_to` on an
    /// unbound datagram succeeds for a missing path only at call time, so
    /// probing now turns "no systemd here" into a clean `Err` at construction.
    pub(crate) fn connect(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let socket = UnixDatagram::unbound()?;
        socket.connect(&path)?;
        Ok(Self {
            socket,
            identifier: default_identifier(),
        })
    }

    pub(crate) fn set_identifier(&mut self, identifier: impl Into<String>) {
        self.identifier = identifier.into();
    }

    /// Send one entry unchanged. The layer always goes through `send_lossy`,
    /// but tests use this to assert on the encoded payload.
    #[cfg(test)]
    pub(crate) fn send(&self, line: &str, meta: &EntryMeta<'_>) -> io::Result<()> {
        self.socket.send(&self.encode(line, meta)).map(|_| ())
    }

    /// Serialise one entry into the journal's native wire format.
    fn encode(&self, line: &str, meta: &EntryMeta<'_>) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256 + line.len());
        put_field(&mut buf, "MESSAGE", line);
        put_field(&mut buf, "PRIORITY", priority(meta.level));
        put_field(&mut buf, "SYSLOG_IDENTIFIER", &self.identifier);
        if !meta.target.is_empty() {
            put_field(&mut buf, "TARGET", meta.target);
        }
        if !meta.file.is_empty() {
            put_field(&mut buf, "CODE_FILE", meta.file);
            put_field(&mut buf, "CODE_LINE", &meta.line.to_string());
        }
        for name in meta.spans {
            put_field(&mut buf, "SPAN_NAME", name);
        }

        buf
    }

    /// Same as [`Self::send`] but truncating an oversized entry instead of
    /// failing, so a pathological message cannot silently drop the log line.
    ///
    /// `MESSAGE` shares the datagram with `PRIORITY`, `SYSLOG_IDENTIFIER`,
    /// `TARGET`, `CODE_FILE`, `CODE_LINE` and one `SPAN_NAME` per span, and a
    /// multi-line value carries a length header on top. Budgeting the whole
    /// `MAX_ENTRY_BYTES` for the line therefore overshoots and lands on the
    /// `EMSGSIZE` path this function exists to avoid, so the overhead is
    /// measured with an empty message first.
    pub(crate) fn send_lossy(&self, line: &str, meta: &EntryMeta<'_>) {
        let overhead = self.encode("", meta).len();
        let mut budget = MAX_ENTRY_BYTES.saturating_sub(overhead);
        loop {
            let buf = self.encode(&truncate(line, budget), meta);
            if buf.len() <= MAX_ENTRY_BYTES {
                // The kernel's limit can sit below MAX_ENTRY_BYTES, so back off
                // and retry smaller rather than drop the line that truncation
                // was meant to keep.
                let retry = match self.socket.send(&buf) {
                    Ok(_) => return,
                    Err(e) => e.raw_os_error() == Some(EMSGSIZE) && budget > 1024,
                };
                if !retry {
                    return;
                }
                budget /= 2;
            } else if budget <= 1024 {
                return;
            } else {
                // The `[truncated N bytes]` suffix is itself part of the entry,
                // so a pass can overshoot by the length of its own marker.
                budget = budget.saturating_sub(buf.len() - MAX_ENTRY_BYTES + 1);
            }
        }
    }
}

/// Truncate to at most `budget` bytes, on a char boundary, noting what was cut.
fn truncate(line: &str, budget: usize) -> String {
    if line.len() <= budget {
        return line.to_owned();
    }
    // Slicing mid-character panics, so walk to the last boundary at or below
    // the budget instead of indexing.
    let end = line
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|i| *i <= budget)
        .last()
        .unwrap_or(0);
    format!("{} [truncated {} bytes]", &line[..end], line.len() - end)
}

/// journald discards fields whose names violate its conventions, so mirror what
/// `tracing-journald` does: uppercase, non-alphanumerics to `_`, no leading `_`.
fn put_field(buf: &mut Vec<u8>, name: &str, value: &str) {
    let name = sanitize_name(name);
    if value.contains('\n') {
        // Length-encoded form is mandatory once the value spans lines.
        buf.extend_from_slice(name.as_bytes());
        buf.push(b'\n');
        buf.extend_from_slice(value.len().to_string().as_bytes());
        buf.push(b'\n');
        buf.extend_from_slice(value.as_bytes());
        buf.push(b'\n');
    } else {
        buf.extend_from_slice(name.as_bytes());
        buf.push(b'=');
        buf.extend_from_slice(value.as_bytes());
        buf.push(b'\n');
    }
}

fn sanitize_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
        } else {
            out.push('_');
        }
    }
    // A name of only underscores would otherwise sanitise to the empty string,
    // which `put_field` would write as `=value`, corrupting the entry.
    let name = out.trim_start_matches('_');
    if name.is_empty() {
        "FIELD".to_owned()
    } else {
        name.to_owned()
    }
}

fn priority(level: tracing::Level) -> &'static str {
    match level {
        tracing::Level::ERROR => "3",
        tracing::Level::WARN => "4",
        tracing::Level::INFO => "5",
        tracing::Level::DEBUG => "6",
        tracing::Level::TRACE => "7",
    }
}

fn default_identifier() -> String {
    std::env::args_os()
        .next()
        .as_ref()
        .and_then(|p| Path::new(p).file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::Level;

    fn meta() -> EntryMeta<'static> {
        EntryMeta {
            level: Level::WARN,
            target: "my_app::net",
            file: "src/net.rs",
            line: 42,
            spans: &[],
        }
    }

    #[test]
    fn payload_is_length_free_when_single_line() {
        let mut buf = Vec::new();
        put_field(&mut buf, "MESSAGE", "hello");
        assert_eq!(buf, b"MESSAGE=hello\n");
    }

    #[test]
    fn multiline_value_uses_length_encoding() {
        let mut buf = Vec::new();
        put_field(&mut buf, "MESSAGE", "a\nb");
        assert_eq!(buf, b"MESSAGE\n3\na\nb\n");
    }

    #[test]
    fn field_names_are_sanitized() {
        assert_eq!(sanitize_name("_hidden"), "HIDDEN");
        assert_eq!(sanitize_name("my.field-name"), "MY_FIELD_NAME");
    }

    /// An all-underscore name sanitises to empty, and an empty name would be
    /// written as `=value`, which journald rejects as malformed.
    #[test]
    fn all_underscore_name_still_yields_a_valid_field() {
        for name in ["_", "___", ".-."] {
            let mut buf = Vec::new();
            put_field(&mut buf, name, "v");
            let written = std::str::from_utf8(&buf).unwrap();
            assert!(!written.starts_with('='), "{name:?} produced {written:?}");
            assert_eq!(written, "FIELD=v\n");
        }
    }

    #[test]
    fn priorities_follow_syslog() {
        assert_eq!(priority(Level::ERROR), "3");
        assert_eq!(priority(Level::INFO), "5");
        assert_eq!(priority(Level::TRACE), "7");
    }

    /// The point of the module: `MESSAGE` carries the rendered line verbatim.
    #[test]
    fn message_field_is_the_formatted_line() {
        let dir = std::env::temp_dir().join(format!("ts-jd-{}", std::process::id()));
        let _ = std::fs::remove_file(&dir);
        let listener = UnixDatagram::bind(&dir).unwrap();
        let sink = JournaldSink::connect(&dir).unwrap();

        let rendered = "WARN [1] my_app::net::send(): slow: {ms: 900}";
        sink.send(rendered, &meta()).unwrap();

        let mut buf = [0u8; 4096];
        let n = listener.recv(&mut buf).unwrap();
        let payload = std::str::from_utf8(&buf[..n]).unwrap();
        assert!(
            payload.starts_with(&format!("MESSAGE={rendered}\n")),
            "got {payload:?}"
        );
        assert!(payload.contains("PRIORITY=4\n"), "got {payload:?}");
        assert!(payload.contains("CODE_LINE=42\n"), "got {payload:?}");

        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn oversized_line_is_truncated_not_dropped() {
        let dir = std::env::temp_dir().join(format!("ts-jd-big-{}", std::process::id()));
        let _ = std::fs::remove_file(&dir);
        let listener = UnixDatagram::bind(&dir).unwrap();
        let sink = JournaldSink::connect(&dir).unwrap();

        let huge = "x".repeat(MAX_ENTRY_BYTES + 10);
        sink.send_lossy(&huge, &meta());

        let mut buf = vec![0u8; MAX_ENTRY_BYTES + 1024];
        let n = listener.recv(&mut buf).unwrap();
        let payload = std::str::from_utf8(&buf[..n]).unwrap();
        assert!(
            payload.contains("[truncated"),
            "got {} bytes",
            payload.len()
        );

        let _ = std::fs::remove_file(&dir);
    }

    /// The whole entry, metadata included, must fit the datagram budget. A line
    /// sized to `MAX_ENTRY_BYTES` used to overshoot by its own field overhead and
    /// hit EMSGSIZE, which `send_lossy` then swallowed.
    #[test]
    fn truncated_entry_fits_the_budget_including_metadata() {
        let dir = std::env::temp_dir().join(format!("ts-jd-fit-{}", std::process::id()));
        let _ = std::fs::remove_file(&dir);
        let listener = UnixDatagram::bind(&dir).unwrap();
        let sink = JournaldSink::connect(&dir).unwrap();

        let spans = ["request", "handler", "db"];
        let mut meta = meta();
        meta.spans = &spans;

        let huge = "y".repeat(MAX_ENTRY_BYTES);
        sink.send_lossy(&huge, &meta);

        let mut buf = vec![0u8; MAX_ENTRY_BYTES * 2];
        let n = listener.recv(&mut buf).unwrap();
        assert!(
            n <= MAX_ENTRY_BYTES,
            "entry {n} bytes exceeds budget {MAX_ENTRY_BYTES}"
        );
        let payload = std::str::from_utf8(&buf[..n]).unwrap();
        assert!(payload.starts_with("MESSAGE="), "{payload:?}");
        // Metadata must survive truncation, not be crowded out by the message.
        for field in ["PRIORITY=4\n", "CODE_LINE=42\n", "SPAN_NAME=db\n"] {
            assert!(payload.contains(field), "missing {field:?} in entry");
        }

        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn truncation_lands_on_a_char_boundary() {
        let line = "é".repeat(2000);
        let out = truncate(&line, 101);
        assert!(out.starts_with(&line[..100]), "{out:?}");
        assert!(out.contains("[truncated 3900 bytes]"), "{out:?}");
    }
}
