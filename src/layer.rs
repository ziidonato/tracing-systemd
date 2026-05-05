//! [`SystemdLayer`]: a `tracing-subscriber` Layer that pretty-prints
//! span chains, with optional ANSI colors and timestamps.

use std::borrow::Cow;
use std::fmt;

use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

use crate::format::event::{EventInput, render_event};
use crate::format::span_chain::SpanLink;
use crate::format::{FormatConfig, TimestampFormat, syslog_prefix};
use crate::output::Output;
use crate::visit::{FieldMap, FieldStorage, FieldVisitor};

#[cfg(feature = "colors")]
use crate::format::color::{ColorMode, ColorTheme};

#[cfg(feature = "json")]
use crate::format::RenderMode;
#[cfg(feature = "json")]
use crate::format::json::render_event_json;

/// A `tracing-subscriber` Layer that emits a pretty span-chain line per event.
///
/// Construct with [`SystemdLayer::stdout`], then chain `with_*` methods to
/// configure formatting, and pass it to a `tracing-subscriber::registry()`.
///
/// ```no_run
/// use tracing_subscriber::prelude::*;
/// use tracing_systemd::SystemdLayer;
///
/// tracing_subscriber::registry()
///     .with(SystemdLayer::stdout().with_target(true))
///     .init();
/// ```
pub struct SystemdLayer {
    config: FormatConfig,
    output: Output,
    #[cfg(feature = "colors")]
    color_mode: ColorMode,
    #[cfg(feature = "colors")]
    color_theme: ColorTheme,
}

impl fmt::Debug for SystemdLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("SystemdLayer");
        d.field("config", &self.config).field("output", &self.output);
        #[cfg(feature = "colors")]
        d.field("color_mode", &self.color_mode)
            .field("color_theme", &self.color_theme);
        d.finish()
    }
}

impl Default for SystemdLayer {
    fn default() -> Self {
        Self::stdout()
    }
}

// ---------- Constructors ----------

impl SystemdLayer {
    /// Construct a layer that writes formatted lines to standard output.
    /// All formatting options have sensible defaults; chain `with_*`
    /// methods to override them.
    #[must_use]
    pub fn stdout() -> Self {
        Self {
            config: FormatConfig::default(),
            output: Output::stdout(),
            #[cfg(feature = "colors")]
            color_mode: ColorMode::default(),
            #[cfg(feature = "colors")]
            color_theme: ColorTheme::default(),
        }
    }

    /// Construct a layer that writes formatted lines to standard error.
    #[must_use]
    pub fn stderr() -> Self {
        Self {
            config: FormatConfig::default(),
            output: Output::stderr(),
            #[cfg(feature = "colors")]
            color_mode: ColorMode::default(),
            #[cfg(feature = "colors")]
            color_theme: ColorTheme::default(),
        }
    }

    /// Construct a layer that writes formatted lines to a journald-friendly
    /// stream — i.e. **stdout with syslog-priority prefixes** (`<3>`–`<7>`).
    /// `journalctl` understands these prefixes when no native journal field
    /// is supplied, which is what happens for processes managed by a
    /// systemd unit (their stdout is already routed to the journal).
    ///
    /// For direct journal-protocol writes (without going through stdout),
    /// enable the `journald` feature and use [`crate::journald::layer`].
    #[must_use]
    pub fn unit_stdout() -> Self {
        Self {
            config: FormatConfig {
                use_level_prefix: true,
                ..FormatConfig::default()
            },
            output: Output::stdout(),
            #[cfg(feature = "colors")]
            color_mode: ColorMode::Never,
            #[cfg(feature = "colors")]
            color_theme: ColorTheme::monochrome(),
        }
    }

    /// Construct a layer that emits a single-line JSON object per event to
    /// standard output.
    ///
    /// Defaults differ from the pretty-mode constructors: `target` is on,
    /// timestamps are on with [`TimestampFormat::Rfc3339`], and the syslog
    /// level prefix is off (so each line is a valid standalone JSON object).
    /// Pretty-only builders (`with_color_*`, separators, brackets, thread-id
    /// prefix/suffix) compile but have no effect in JSON mode.
    ///
    /// Schema (per line):
    ///
    /// ```json
    /// {
    ///   "timestamp": "2026-05-05T14:23:45.123Z",
    ///   "level": "INFO",
    ///   "message": "served request",
    ///   "target": "my_app::handlers",
    ///   "span_chain": [
    ///     {"name": "request", "fields": {"method": "GET"}},
    ///     {"name": "handler", "fields": {}}
    ///   ],
    ///   "fields": {"latency_ms": 4}
    /// }
    /// ```
    ///
    /// `thread_id` is added at the top level when [`Self::with_thread_ids`]
    /// is `true`. Non-finite `f64` values become JSON `null`, matching
    /// `tracing-subscriber`'s JSON formatter.
    #[cfg(feature = "json")]
    #[cfg_attr(docsrs, doc(cfg(feature = "json")))]
    #[must_use]
    pub fn json() -> Self {
        Self {
            config: FormatConfig {
                mode: RenderMode::Json,
                show_target: true,
                show_timestamp: true,
                timestamp_format: TimestampFormat::Rfc3339,
                use_level_prefix: false,
                ..FormatConfig::default()
            },
            output: Output::stdout(),
            #[cfg(feature = "colors")]
            color_mode: ColorMode::Never,
            #[cfg(feature = "colors")]
            color_theme: ColorTheme::monochrome(),
        }
    }
}

// ---------- Builder methods ----------

impl SystemdLayer {
    /// Override the destination. Useful for `Output::stderr()` or
    /// `Output::writer(buf)` (test capture).
    #[must_use]
    pub fn with_output(mut self, output: Output) -> Self {
        self.output = output;
        self
    }

    /// Show the event's `target` (typically the module path) before the span chain.
    /// Default: `false`.
    #[must_use]
    pub fn with_target(mut self, show: bool) -> Self {
        self.config.show_target = show;
        self
    }

    /// Include the OS thread id in each line. Default: `false`.
    #[must_use]
    pub fn with_thread_ids(mut self, show: bool) -> Self {
        self.config.show_thread_id = show;
        self
    }

    /// Show a timestamp at the start of each line. Default: `false`.
    /// See also [`SystemdLayer::with_timestamp_format`].
    #[must_use]
    pub fn with_timestamps(mut self, show: bool) -> Self {
        self.config.show_timestamp = show;
        self
    }

    /// Choose the timestamp format. Implies `with_timestamps(true)` for any
    /// non-`None` value. Default: [`TimestampFormat::None`].
    #[must_use]
    pub fn with_timestamp_format(mut self, format: TimestampFormat) -> Self {
        self.config.timestamp_format = format;
        if format != TimestampFormat::None {
            self.config.show_timestamp = true;
        }
        self
    }

    /// Emit a syslog-priority prefix (`<3>` – `<7>`) before each line. This
    /// is what `journalctl` uses to assign a `PRIORITY` when ingesting plain
    /// stdout from a systemd unit. Default: `true`.
    #[must_use]
    pub fn with_level_prefix(mut self, use_prefix: bool) -> Self {
        self.config.use_level_prefix = use_prefix;
        self
    }

    /// Separator between spans in the chain. Default: `"::"`.
    #[must_use]
    pub fn with_span_separator(mut self, sep: impl Into<Cow<'static, str>>) -> Self {
        self.config.span_separator = sep.into();
        self
    }

    /// Separator between the level/span chain and the event message. Default: `": "`.
    #[must_use]
    pub fn with_message_separator(mut self, sep: impl Into<Cow<'static, str>>) -> Self {
        self.config.message_separator = sep.into();
        self
    }

    /// Separator after the level. Default: `" "`.
    #[must_use]
    pub fn with_level_separator(mut self, sep: impl Into<Cow<'static, str>>) -> Self {
        self.config.level_separator = sep.into();
        self
    }

    /// String wrapping the *opening* of a span argument list. Default: `"("`.
    #[must_use]
    pub fn with_function_bracket_left(mut self, s: impl Into<Cow<'static, str>>) -> Self {
        self.config.function_bracket_left = s.into();
        self
    }

    /// String wrapping the *closing* of a span argument list. Default: `")"`.
    #[must_use]
    pub fn with_function_bracket_right(mut self, s: impl Into<Cow<'static, str>>) -> Self {
        self.config.function_bracket_right = s.into();
        self
    }

    /// String between an argument name and its value. Default: `": "`.
    #[must_use]
    pub fn with_arguments_equality(mut self, s: impl Into<Cow<'static, str>>) -> Self {
        self.config.arguments_equality = s.into();
        self
    }

    /// String between consecutive arguments. Default: `", "`.
    #[must_use]
    pub fn with_arguments_separator(mut self, s: impl Into<Cow<'static, str>>) -> Self {
        self.config.arguments_separator = s.into();
        self
    }

    /// Prefix for the thread id when `with_thread_ids(true)`. Default: `"["`.
    #[must_use]
    pub fn with_thread_id_prefix(mut self, s: impl Into<Cow<'static, str>>) -> Self {
        self.config.thread_id_prefix = s.into();
        self
    }

    /// Suffix for the thread id when `with_thread_ids(true)`. Default: `"] "`.
    #[must_use]
    pub fn with_thread_id_suffix(mut self, s: impl Into<Cow<'static, str>>) -> Self {
        self.config.thread_id_suffix = s.into();
        self
    }

    /// Set the [`ColorMode`]. Default: [`ColorMode::Auto`] (color iff TTY
    /// and `NO_COLOR` is unset).
    #[cfg(feature = "colors")]
    #[cfg_attr(docsrs, doc(cfg(feature = "colors")))]
    #[must_use]
    pub fn with_color_mode(mut self, mode: ColorMode) -> Self {
        self.color_mode = mode;
        self
    }

    /// Set the [`ColorTheme`]. Default: [`ColorTheme::default`] (matches 0.1).
    #[cfg(feature = "colors")]
    #[cfg_attr(docsrs, doc(cfg(feature = "colors")))]
    #[must_use]
    pub fn with_color_theme(mut self, theme: ColorTheme) -> Self {
        self.color_theme = theme;
        self
    }
}

// ---------- Layer impl ----------

impl<S> Layer<S> for SystemdLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: Context<'_, S>,
    ) {
        let mut fields = FieldMap::new();
        attrs.record(&mut FieldVisitor::new(&mut fields));

        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(FieldStorage(fields));
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        // Walk parent spans and copy their fields out.
        // The leaf span (if any) is the *last* element; everything before
        // it is "parents" in the chain.
        let mut chain: Vec<SpanLink> = Vec::new();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                let exts = span.extensions();
                let fields = exts
                    .get::<FieldStorage>()
                    .map(|s| s.0.clone())
                    .unwrap_or_default();
                chain.push(SpanLink {
                    name: span.name(),
                    fields,
                });
            }
        }
        let leaf = chain.last().cloned();
        let parents: &[SpanLink] = if chain.is_empty() {
            &[]
        } else {
            &chain[..chain.len() - 1]
        };

        // Visit the event's own fields.
        let mut event_fields = FieldMap::new();
        event.record(&mut FieldVisitor::new(&mut event_fields));

        let metadata = event.metadata();
        let level = *metadata.level();

        let input = EventInput {
            level,
            target: metadata.target(),
            parents,
            leaf: leaf.as_ref(),
            fields: &event_fields,
        };

        let line = self.render(&input);

        if self.config.use_level_prefix {
            self.output.write_line(&format!("{}{}", syslog_prefix(level), line));
        } else {
            self.output.write_line(&line);
        }
    }
}

impl SystemdLayer {
    fn render(&self, input: &EventInput<'_>) -> String {
        #[cfg(feature = "json")]
        if matches!(self.config.mode, RenderMode::Json) {
            return render_event_json(&self.config, input);
        }

        #[cfg(feature = "colors")]
        {
            let use_color = self.color_mode.resolve_now(self.output.is_terminal());
            let theme = if use_color { Some(&self.color_theme) } else { None };
            render_event(&self.config, input, theme)
        }
        #[cfg(not(feature = "colors"))]
        {
            render_event(&self.config, input)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tracing::{Level, info, info_span, warn};
    use tracing_subscriber::prelude::*;

    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);
    impl Write for Buf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn capture<F: FnOnce()>(layer: SystemdLayer, body: F) -> String {
        let buf = Buf::default();
        let captured = buf.0.clone();
        let layer = layer.with_output(Output::writer(buf));
        tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), body);
        let bytes = captured.lock().unwrap().clone();
        String::from_utf8(bytes).expect("utf-8 output")
    }

    #[test]
    fn bare_info_event() {
        let layer = SystemdLayer::stdout().with_level_prefix(false);
        let out = capture(layer, || {
            info!("hello");
        });
        // Target depends on test crate name; just check the suffix.
        assert!(out.ends_with(": hello\n"), "got {out:?}");
        assert!(out.starts_with("INFO "), "got {out:?}");
        let _ = Level::INFO; // silence unused
    }

    #[test]
    fn span_arguments_appear_in_output() {
        let layer = SystemdLayer::stdout().with_level_prefix(false);
        let out = capture(layer, || {
            let span = info_span!("worker", id = 7u64);
            let _g = span.enter();
            warn!("done");
        });
        assert!(out.contains("worker(id: 7)"), "got {out:?}");
        assert!(out.contains("done"), "got {out:?}");
    }

    #[test]
    fn level_prefix_emits_syslog_marker() {
        let layer = SystemdLayer::stdout().with_level_prefix(true);
        let out = capture(layer, || {
            info!("p");
        });
        assert!(out.starts_with("<5>INFO"), "got {out:?}");
    }
}
