//! User-supplied format templates.
//!
//! A template is parsed once when the layer is built and rendered per event,
//! so a typo is a startup error rather than a silently broken log line.
//!
//! ```text
//! {timestamp} {level:>5} {target}: {message} {fields}
//! ```
//!
//! Placeholders name either a built-in variable or an event field: anything
//! that is not a built-in is looked up in the event's own fields, so
//! `info!(rows = 3, "done")` can be rendered with `{rows}`.

use std::fmt;

use crate::format::event::EventInput;
use crate::format::paint::Painter;
use crate::format::span_chain::{render_parents, render_span_name};
use crate::format::{FormatConfig, TimestampFormat, current_thread_id_int, format_timestamp};

/// A compiled format template.
#[derive(Debug, Clone)]
pub(crate) struct Template {
    segments: Vec<Segment>,
    /// Captured so an error message can show what the user actually wrote.
    source: String,
}

#[derive(Debug, Clone)]
enum Segment {
    Literal(String),
    Placeholder(Placeholder),
}

#[derive(Debug, Clone)]
struct Placeholder {
    name: String,
    align: Align,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Align {
    #[default]
    None,
    Left(usize),
    Right(usize),
    Center(usize),
}

/// Guards against a template that is one long run of placeholders: each
/// placeholder allocates a `String`, and compile-time is the only cheap place
/// to refuse. Chosen far above any realistic template.
const MAX_PLACEHOLDERS: usize = 1024;

/// Templates quote themselves in every error, and a 1 MB template would make
/// the message worse than the mistake.
const MAX_QUOTED: usize = 200;

/// Refuse absurd padding widths at compile time. `{level:>999999999}` is a
/// single keystroke away from a gigabyte allocation on every event.
const MAX_WIDTH: usize = 1024;

/// Why a template could not be compiled.
///
/// Every variant carries the whole template: a brace error three placeholders
/// deep is not findable from a column number alone once the string is long.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// A `{` was never closed.
    Unclosed(String),
    /// A `}` appeared with no matching `{`. Write `}}` for a literal brace.
    UnmatchedClose(String),
    /// A placeholder had an empty name, e.g. `{:>5}`.
    EmptyName(String),
    /// The part after `:` was not a recognised alignment spec.
    BadSpec(String),
    /// More than `MAX_PLACEHOLDERS` (1024) placeholders in one template.
    TooManyPlaceholders,
    /// An alignment width above `MAX_WIDTH` (1024).
    WidthTooLarge,
}

/// Quote a template for an error message, truncating rather than dumping a
/// whole config file into a log line.
fn quote(t: &str) -> String {
    if t.chars().count() <= MAX_QUOTED {
        return format!("{t:?}");
    }
    let head: String = t.chars().take(MAX_QUOTED).collect();
    format!("{head:?}... ({n} chars)", n = t.chars().count())
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unclosed(t) => write!(
                f,
                "unclosed `{{` in template {} (write `{{{{` for a literal brace)",
                quote(t)
            ),
            Self::UnmatchedClose(t) => write!(
                f,
                "unmatched `}}` in template {} (write `}}}}` for a literal brace)",
                quote(t)
            ),
            Self::EmptyName(t) => write!(f, "empty placeholder name in template {}", quote(t)),
            Self::BadSpec(t) => write!(
                f,
                "unrecognised alignment spec in template {}; use `{{name:>5}}`, `{{name:<5}}` or `{{name:^5}}`",
                quote(t)
            ),
            Self::TooManyPlaceholders => {
                write!(f, "template has more than {MAX_PLACEHOLDERS} placeholders")
            }
            Self::WidthTooLarge => write!(f, "alignment width above {MAX_WIDTH}"),
        }
    }
}

impl std::error::Error for TemplateError {}

impl Template {
    /// Compile `src`.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] when the braces do not balance or a
    /// placeholder is malformed.
    pub(crate) fn parse(src: impl Into<String>) -> Result<Self, TemplateError> {
        let source = src.into();
        let mut segments: Vec<Segment> = Vec::new();
        let mut placeholders = 0usize;
        let mut literal = String::new();
        let mut chars = source.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    literal.push('{');
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    literal.push('}');
                }
                '{' => {
                    if !literal.is_empty() {
                        segments.push(Segment::Literal(std::mem::take(&mut literal)));
                    }
                    let mut body = String::new();
                    let mut closed = false;
                    for c in chars.by_ref() {
                        if c == '}' {
                            closed = true;
                            break;
                        }
                        body.push(c);
                    }
                    if !closed {
                        return Err(TemplateError::Unclosed(source));
                    }
                    placeholders += 1;
                    if placeholders > MAX_PLACEHOLDERS {
                        return Err(TemplateError::TooManyPlaceholders);
                    }
                    segments.push(Segment::Placeholder(Placeholder::parse(&body, &source)?));
                }
                '}' => return Err(TemplateError::UnmatchedClose(source)),
                _ => literal.push(c),
            }
        }

        if !literal.is_empty() {
            segments.push(Segment::Literal(literal));
        }

        Ok(Self { segments, source })
    }

    /// Render one event.
    pub(crate) fn render(
        &self,
        cfg: &FormatConfig,
        input: &EventInput<'_>,
        painter: Painter<'_>,
    ) -> String {
        let mut out = String::with_capacity(self.source.len() + 64);

        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => out.push_str(text),
                Segment::Placeholder(ph) => {
                    let mut piece = String::new();
                    ph.write_value(&mut piece, cfg, input, painter);
                    // Padding is measured on the painted string so ANSI
                    // escapes do not count toward the column width.
                    ph.align.apply(&mut out, &piece);
                }
            }
        }

        out
    }
}

impl Placeholder {
    fn parse(body: &str, source: &str) -> Result<Self, TemplateError> {
        let (name, spec) = match body.split_once(':') {
            Some((n, s)) => (n, Some(s)),
            None => (body, None),
        };

        if name.is_empty() {
            return Err(TemplateError::EmptyName(source.to_owned()));
        }

        let align = match spec {
            None | Some("") => Align::None,
            Some(spec) => match parse_align(spec) {
                Ok(align) => align,
                Err(SpecError::TooWide) => return Err(TemplateError::WidthTooLarge),
                Err(SpecError::Malformed) => {
                    return Err(TemplateError::BadSpec(source.to_owned()));
                }
            },
        };

        Ok(Self {
            name: name.to_owned(),
            align,
        })
    }

    fn write_value(
        &self,
        out: &mut String,
        cfg: &FormatConfig,
        input: &EventInput<'_>,
        painter: Painter<'_>,
    ) {
        let name = self.name.as_str();

        match name {
            "level" => painter.write_level(out, input.level),
            "level_short" => out.push_str(short_level(input.level)),
            "target" => painter.write_target(out, input.target),
            "message" => {
                if let Some(msg) = input.fields.get("message") {
                    out.push_str(&msg.to_string());
                }
            }
            "thread_id" => {
                let tid = current_thread_id_int();
                painter.write_thread_id(out, &tid);
            }
            "timestamp" => {
                let fmt = match cfg.timestamp_format {
                    // `{timestamp}` states the intent, so a `None` config still
                    // renders one rather than producing a silently blank column.
                    TimestampFormat::None => TimestampFormat::Rfc3339,
                    other => other,
                };
                out.push_str(&format_timestamp(fmt));
            }
            "file" => out.push_str(input.file),
            "line" => out.push_str(&input.line.to_string()),
            "spans" => out.push_str(&render_parents(cfg, painter, input.parents)),
            "leaf" => {
                if let Some(leaf) = input.leaf {
                    painter.write_span_name(out, leaf.name);
                }
            }
            "span" => {
                out.push_str(&render_parents(cfg, painter, input.parents));
                if let Some(leaf) = input.leaf {
                    out.push_str(&render_span_name(cfg, painter, leaf));
                }
            }
            "fields" => {
                let rest = strip_message(input.fields);
                let mut first = true;
                for (fname, value) in &rest {
                    if !first {
                        out.push_str(&cfg.arguments_separator);
                    }
                    first = false;
                    painter.write_field_name(out, fname);
                    out.push_str(&cfg.arguments_equality);
                    painter.write_field_value(out, value);
                }
            }
            // Anything else is an event field; a missing one renders empty so
            // one template can serve events that carry different fields.
            other => {
                if let Some(value) = input.fields.get(other) {
                    painter.write_field_value(out, value);
                }
            }
        }
    }
}

/// Borrow the event fields minus `message`, without cloning.
fn strip_message(
    fields: &crate::visit::FieldMap,
) -> Vec<(&'static str, &crate::visit::FieldValue)> {
    fields
        .iter()
        .filter(|(k, _)| **k != "message")
        .map(|(k, v)| (*k, v))
        .collect()
}

fn short_level(level: tracing::Level) -> &'static str {
    match level {
        tracing::Level::TRACE => "TRC",
        tracing::Level::DEBUG => "DBG",
        tracing::Level::INFO => "INF",
        tracing::Level::WARN => "WRN",
        tracing::Level::ERROR => "ERR",
    }
}

enum SpecError {
    Malformed,
    TooWide,
}

fn parse_align(spec: &str) -> Result<Align, SpecError> {
    let (c, rest) = spec.split_at_checked(1).ok_or(SpecError::Malformed)?;
    let width: usize = rest.parse().map_err(|_| SpecError::Malformed)?;
    if width > MAX_WIDTH {
        return Err(SpecError::TooWide);
    }
    match c {
        ">" => Ok(Align::Right(width)),
        "<" => Ok(Align::Left(width)),
        "^" => Ok(Align::Center(width)),
        _ => Err(SpecError::Malformed),
    }
}

impl Align {
    fn apply(self, out: &mut String, piece: &str) {
        let width = match self {
            Align::None => {
                out.push_str(piece);
                return;
            }
            Align::Left(w) | Align::Right(w) | Align::Center(w) => w,
        };

        let visible = visible_width(piece);
        let pad = width.saturating_sub(visible);
        if pad == 0 {
            out.push_str(piece);
            return;
        }

        match self {
            Align::Left(_) => {
                out.push_str(piece);
                out.extend(std::iter::repeat_n(' ', pad));
            }
            Align::Right(_) => {
                out.extend(std::iter::repeat_n(' ', pad));
                out.push_str(piece);
            }
            Align::Center(_) => {
                let left = pad / 2;
                out.extend(std::iter::repeat_n(' ', left));
                out.push_str(piece);
                out.extend(std::iter::repeat_n(' ', pad - left));
            }
            Align::None => unreachable!("handled above"),
        }
    }
}

/// Width of `s` ignoring ANSI SGR sequences.
///
/// Counts `chars`, not display columns: a wide CJK glyph or an emoji still
/// counts as one. Prefer over-buying that than pulling in a Unicode width
/// table for log alignment.
///
/// Only a well-formed CSI sequence is skipped. A stray `ESC`, which an event
/// field can carry since its value is arbitrary user data, counts as one
/// column and consumes nothing else. Scanning blindly for the next `m` would
/// swallow the remainder of the line whenever a value happened to contain a
/// bare escape, undercounting every element after it.
fn visible_width(s: &str) -> usize {
    let mut width = 0;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            width += 1;
            continue;
        }
        // CSI: ESC '[' <params> <final byte in @..=~>. Anything else is not a
        // sequence we understand, so treat the ESC as a printable column.
        if chars.peek() != Some(&'[') {
            width += 1;
            continue;
        }
        chars.next();
        let mut terminated = false;
        for c in chars.by_ref() {
            if ('@'..='~').contains(&c) {
                // 'm' is what nu-ansi-term emits; other finals (cursor moves,
                // clears) still occupy no columns, so skip them too.
                terminated = true;
                break;
            }
            // Parameter bytes: digits, ';', '?', ':' are all consumed.
        }
        if !terminated {
            // Truncated sequence mid-string: nothing more to measure.
            break;
        }
    }
    width
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visit::{FieldMap, FieldValue};
    use std::collections::BTreeMap;

    fn fields(kvs: &[(&'static str, FieldValue)]) -> FieldMap {
        let mut m: FieldMap = BTreeMap::new();
        for (k, v) in kvs {
            m.insert(*k, v.clone());
        }
        m
    }

    fn cfg() -> FormatConfig {
        FormatConfig::default()
    }

    fn render(src: &str, level: tracing::Level, fs: &FieldMap) -> String {
        let t = Template::parse(src).expect("template parses");
        let input = EventInput {
            level,
            target: "app::net",
            file: "src/net.rs",
            line: 42,
            parents: &[],
            leaf: None,
            fields: fs,
        };
        t.render(&cfg(), &input, Painter::mono())
    }

    #[test]
    fn literals_and_builtins() {
        let f = fields(&[("message", FieldValue::Str("served".into()))]);
        assert_eq!(
            render("{level} {target}: {message}", tracing::Level::INFO, &f),
            "INFO app::net: served"
        );
    }

    #[test]
    fn unknown_name_is_an_event_field() {
        let f = fields(&[
            ("message", FieldValue::Str("done".into())),
            ("rows", FieldValue::U64(3)),
        ]);
        assert_eq!(render("rows={rows}", tracing::Level::INFO, &f), "rows=3");
    }

    #[test]
    fn missing_field_renders_empty() {
        let f = fields(&[("message", FieldValue::Str("x".into()))]);
        assert_eq!(render("[{absent}]", tracing::Level::INFO, &f), "[]");
    }

    #[test]
    fn brace_escapes() {
        let f = fields(&[]);
        assert_eq!(render("{{literal}}", tracing::Level::INFO, &f), "{literal}");
    }

    #[test]
    fn alignment_pads_to_width() {
        let f = fields(&[]);
        assert_eq!(render("[{level:>5}]", tracing::Level::INFO, &f), "[ INFO]");
        assert_eq!(render("[{level:<5}]", tracing::Level::INFO, &f), "[INFO ]");
        assert_eq!(render("[{level:^6}]", tracing::Level::INFO, &f), "[ INFO ]");
    }

    #[test]
    fn alignment_ignores_ansi_width() {
        // A styled piece padded as if it were two columns wide.
        let painted = "\u{1b}[32mINFO\u{1b}[0m";
        assert_eq!(visible_width(painted), 4);
        let mut out = String::new();
        Align::Right(6).apply(&mut out, painted);
        assert_eq!(out, format!("  {painted}"));
    }

    #[test]
    fn level_short() {
        let f = fields(&[]);
        assert_eq!(render("{level_short}", tracing::Level::ERROR, &f), "ERR");
    }

    #[test]
    fn file_and_line() {
        let f = fields(&[]);
        assert_eq!(
            render("{file}:{line}", tracing::Level::INFO, &f),
            "src/net.rs:42"
        );
    }

    #[test]
    fn fields_renders_everything_but_message() {
        let f = fields(&[
            ("message", FieldValue::Str("done".into())),
            ("a", FieldValue::U64(1)),
            ("b", FieldValue::Bool(true)),
        ]);
        assert_eq!(
            render("{message} {fields}", tracing::Level::INFO, &f),
            "done a: 1, b: true"
        );
    }

    #[test]
    fn unclosed_brace_is_an_error() {
        assert_eq!(
            Template::parse("{message").unwrap_err(),
            TemplateError::Unclosed("{message".into())
        );
    }

    #[test]
    fn unmatched_close_is_an_error() {
        assert_eq!(
            Template::parse("a} b").unwrap_err(),
            TemplateError::UnmatchedClose("a} b".into())
        );
    }

    #[test]
    fn empty_name_is_an_error() {
        assert!(matches!(
            Template::parse("{:>5}").unwrap_err(),
            TemplateError::EmptyName(_)
        ));
    }

    #[test]
    fn bad_spec_is_an_error() {
        assert!(matches!(
            Template::parse("{level:wat}").unwrap_err(),
            TemplateError::BadSpec(_)
        ));
    }
}
