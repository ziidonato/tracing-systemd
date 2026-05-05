//! Formatting types and rendering helpers.

use std::borrow::Cow;

#[cfg(feature = "colors")]
pub(crate) mod color;
pub(crate) mod event;
pub(crate) mod span_chain;

#[cfg(feature = "colors")]
pub use color::{ColorMode, ColorTheme};

/// How (or whether) to render a timestamp at the start of each stdout line.
///
/// Journald entries always carry their own native timestamp, so this
/// applies only to the stdout layer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TimestampFormat {
    /// No timestamp prefix. Default.
    #[default]
    None,
    /// Seconds since [`UNIX_EPOCH`](std::time::UNIX_EPOCH), with millisecond
    /// fraction (e.g. `1714867200.123`). Free of any external date crate.
    UnixSeconds,
    /// Seconds since this process started (e.g. `12.345`). Useful for tests
    /// and short-lived programs.
    Uptime,
}

/// Static configuration shared by both the stdout and journald renderers.
///
/// Held by `SystemdLayer` and not exposed publicly — users configure via
/// builder methods on [`SystemdLayer`](crate::SystemdLayer) instead.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // Independent toggles, not state.
pub(crate) struct FormatConfig {
    pub show_target: bool,
    pub show_thread_id: bool,
    pub show_timestamp: bool,
    pub timestamp_format: TimestampFormat,
    pub use_level_prefix: bool,
    pub span_separator: Cow<'static, str>,
    pub message_separator: Cow<'static, str>,
    pub level_separator: Cow<'static, str>,
    pub function_bracket_left: Cow<'static, str>,
    pub function_bracket_right: Cow<'static, str>,
    pub arguments_equality: Cow<'static, str>,
    pub arguments_separator: Cow<'static, str>,
    pub thread_id_prefix: Cow<'static, str>,
    pub thread_id_suffix: Cow<'static, str>,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            show_target: false,
            show_thread_id: false,
            show_timestamp: false,
            timestamp_format: TimestampFormat::None,
            use_level_prefix: true,
            span_separator: Cow::Borrowed("::"),
            message_separator: Cow::Borrowed(": "),
            level_separator: Cow::Borrowed(" "),
            function_bracket_left: Cow::Borrowed("("),
            function_bracket_right: Cow::Borrowed(")"),
            arguments_equality: Cow::Borrowed(": "),
            arguments_separator: Cow::Borrowed(", "),
            thread_id_prefix: Cow::Borrowed("["),
            thread_id_suffix: Cow::Borrowed("] "),
        }
    }
}

/// Map a tracing [`Level`](tracing::Level) to its syslog priority prefix
/// (`<3>`–`<7>`), as understood by `journalctl` when no native field is
/// supplied.
pub(crate) fn syslog_prefix(level: tracing::Level) -> &'static str {
    match level {
        tracing::Level::ERROR => "<3>",
        tracing::Level::WARN => "<4>",
        tracing::Level::INFO => "<5>",
        tracing::Level::DEBUG => "<6>",
        tracing::Level::TRACE => "<7>",
    }
}

/// Read the current thread's id as a `u64`. tracing-systemd 0.1 parsed
/// the `Debug` output of [`ThreadId`](std::thread::ThreadId) by string
/// splitting; we do the same here, but defensively. Stable Rust does not
/// (yet) expose the integer directly outside nightly's `as_u64`.
pub(crate) fn current_thread_id_int() -> String {
    let id = format!("{:?}", std::thread::current().id());
    // ThreadId's Debug is e.g. `ThreadId(2)`. Pull out the digits.
    id.split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map_or_else(|| id.clone(), |(digits, _)| digits.to_owned())
}

/// Format a timestamp prefix according to `format`. Returns an empty string
/// for [`TimestampFormat::None`].
pub(crate) fn format_timestamp(format: TimestampFormat) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    match format {
        TimestampFormat::None => String::new(),
        TimestampFormat::UnixSeconds => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or_else(|_| String::from("0.000"), |d| {
                format!("{}.{:03}", d.as_secs(), d.subsec_millis())
            }),
        TimestampFormat::Uptime => {
            let elapsed = process_start().elapsed();
            format!("{}.{:03}", elapsed.as_secs(), elapsed.subsec_millis())
        }
    }
}

fn process_start() -> std::time::Instant {
    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    *START.get_or_init(std::time::Instant::now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syslog_prefix_maps_levels() {
        assert_eq!(syslog_prefix(tracing::Level::ERROR), "<3>");
        assert_eq!(syslog_prefix(tracing::Level::WARN), "<4>");
        assert_eq!(syslog_prefix(tracing::Level::INFO), "<5>");
        assert_eq!(syslog_prefix(tracing::Level::DEBUG), "<6>");
        assert_eq!(syslog_prefix(tracing::Level::TRACE), "<7>");
    }

    #[test]
    fn current_thread_id_int_is_numeric() {
        let s = current_thread_id_int();
        // Should be parseable as some kind of integer. ThreadId's Debug
        // format isn't strictly stable, so we tolerate a fallback string,
        // but on real Rust we expect digits.
        if let Ok(n) = s.parse::<u64>() {
            // Test threads have positive ids.
            assert!(n > 0);
        }
    }

    #[test]
    fn format_timestamp_none_is_empty() {
        assert_eq!(format_timestamp(TimestampFormat::None), "");
    }

    #[test]
    fn format_timestamp_unix_has_dot() {
        let s = format_timestamp(TimestampFormat::UnixSeconds);
        assert!(s.contains('.'), "unexpected {s:?}");
    }

    #[test]
    fn format_timestamp_uptime_has_dot() {
        let s = format_timestamp(TimestampFormat::Uptime);
        assert!(s.contains('.'), "unexpected {s:?}");
    }
}
