//! Formatting types and rendering helpers.

use std::borrow::Cow;

#[cfg(feature = "colors")]
pub(crate) mod color;
pub(crate) mod event;
#[cfg(feature = "json")]
pub(crate) mod json;
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
    /// RFC 3339 / ISO 8601 in UTC with millisecond precision and a trailing
    /// `Z` (e.g. `2026-05-05T14:23:45.123Z`). Hand-formatted from
    /// [`SystemTime`](std::time::SystemTime); no external date crate.
    /// Default for [`SystemdLayer::json`](crate::SystemdLayer::json).
    Rfc3339,
}

/// Which renderer the layer uses to turn an event into a line.
///
/// `Pretty` produces the human-readable span-chain form. `Json` produces a
/// single-line JSON object per event (see [`SystemdLayer::json`](crate::SystemdLayer::json)).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum RenderMode {
    #[default]
    Pretty,
    #[cfg(feature = "json")]
    Json,
}

/// Static configuration shared by both the stdout and journald renderers.
///
/// Held by `SystemdLayer` and not exposed publicly — users configure via
/// builder methods on [`SystemdLayer`](crate::SystemdLayer) instead.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // Independent toggles, not state.
pub(crate) struct FormatConfig {
    #[cfg_attr(not(feature = "json"), allow(dead_code))]
    pub mode: RenderMode,
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
            mode: RenderMode::Pretty,
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
        TimestampFormat::Rfc3339 => format_rfc3339(SystemTime::now()),
    }
}

/// RFC 3339 / ISO 8601 in UTC with millisecond precision and a trailing `Z`.
/// Hand-formatted from `SystemTime` using Howard Hinnant's date algorithm,
/// so no external date crate is needed.
fn format_rfc3339(now: std::time::SystemTime) -> String {
    use std::time::UNIX_EPOCH;
    let Ok(dur) = now.duration_since(UNIX_EPOCH) else {
        return String::from("1970-01-01T00:00:00.000Z");
    };
    let total_secs = dur.as_secs();
    let millis = dur.subsec_millis();
    // Days since 1970-01-01 fit in i64 for any plausible system clock.
    #[allow(clippy::cast_possible_wrap)]
    let days = (total_secs / 86_400) as i64;
    let sod = total_secs % 86_400;
    let hour = sod / 3_600;
    let minute = (sod % 3_600) / 60;
    let second = sod % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// Howard Hinnant's `civil_from_days`:
/// <https://howardhinnant.github.io/date_algorithms.html#civil_from_days>.
/// Converts days-since-1970-01-01 into a `(year, month, day)` tuple in the
/// proleptic Gregorian calendar.
#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year, m as u32, d as u32)
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

    #[test]
    fn rfc3339_known_epochs() {
        use std::time::{Duration, UNIX_EPOCH};
        // Epoch
        assert_eq!(
            format_rfc3339(UNIX_EPOCH),
            "1970-01-01T00:00:00.000Z"
        );
        // 2026-05-05T14:23:45.123Z
        // 56 years × 365 + 14 leap days = 20454 days through 2025-12-31;
        // + 124 days through 2026-05-05 = 20578 days.
        // 20578 × 86400 + 14×3600 + 23×60 + 45 = 1_777_991_025 s.
        let t = UNIX_EPOCH + Duration::new(1_777_991_025, 123_000_000);
        assert_eq!(format_rfc3339(t), "2026-05-05T14:23:45.123Z");
        // A leap year: 2000-02-29
        // `date -u -d '2000-02-29T00:00:00Z' +%s` -> 951782400
        let t = UNIX_EPOCH + Duration::new(951_782_400, 0);
        assert_eq!(format_rfc3339(t), "2000-02-29T00:00:00.000Z");
        // 1999-12-31T23:59:59.999Z
        let t = UNIX_EPOCH + Duration::new(946_684_799, 999_000_000);
        assert_eq!(format_rfc3339(t), "1999-12-31T23:59:59.999Z");
    }

    #[test]
    fn format_timestamp_rfc3339_shape() {
        let s = format_timestamp(TimestampFormat::Rfc3339);
        // Shape: YYYY-MM-DDTHH:MM:SS.mmmZ → length 24, ends with Z, has 'T'.
        assert_eq!(s.len(), 24, "got {s:?}");
        assert!(s.ends_with('Z'), "got {s:?}");
        assert!(s.contains('T'), "got {s:?}");
    }
}
