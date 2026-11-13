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
use crate::format::paint::{Painter, Quoting};
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
    content: Content,
    quoting: Quoting,
}

/// What a placeholder renders, decided at compile time so the render path is a
/// flat dispatch and the modifier set can differ per kind.
///
/// Plain covers the metadata slots (level, target, timestamp, call site) that
/// carry no string values, so they take `:plain` but not `:raw`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Content {
    Plain,
    Theme,
    Span,
    Fields,
}

fn parse_align_checked(spec: &str, source: &str) -> Result<Align, TemplateError> {
    match parse_align(spec) {
        Ok(align) => Ok(align),
        Err(SpecError::TooWide) => Err(TemplateError::WidthTooLarge(source.to_owned())),
        Err(SpecError::Malformed) => Err(TemplateError::BadSpec(source.to_owned())),
    }
}

/// The content kind for a built-in name, or `None` for an event field.
fn content_of(name: &str) -> Option<Content> {
    Some(match name {
        "spans" | "leaf" | "span" => Content::Span,
        "fields" => Content::Fields,
        "message" | "level" | "level_short" | "target" | "thread_id" | "timestamp" | "file"
        | "line" => Content::Theme,
        _ => return None,
    })
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
    TooManyPlaceholders(String),
    /// An alignment width above `MAX_WIDTH` (1024).
    WidthTooLarge(String),
    /// A `{` inside a placeholder body, e.g. `{level{>5}}`.
    NestedBrace(String),
    /// A placeholder name that is not an identifier, e.g. `{level!}`.
    BadName(String),
    /// A modifier that is not recognised, or not available on that placeholder.
    /// e.g. `{level:x}` or `{fields:raw-something}`.
    BadModifier(String, String),
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
            Self::TooManyPlaceholders(t) => write!(
                f,
                "template has more than {MAX_PLACEHOLDERS} placeholders: {}",
                quote(t)
            ),
            Self::WidthTooLarge(t) => {
                write!(
                    f,
                    "alignment width above {MAX_WIDTH} in template {}",
                    quote(t)
                )
            }
            Self::NestedBrace(t) => write!(
                f,
                "nested `{{` inside a placeholder in template {} (placeholders cannot contain braces)",
                quote(t)
            ),
            Self::BadName(t) => write!(
                f,
                "placeholder name is not an identifier in template {} (letters, digits and `_` only, as in `{{{{level:>5}}}}`)",
                quote(t)
            ),
            Self::BadModifier(m, t) => write!(
                f,
                "unrecognised placeholder modifier `{m}` in template {}; known: `plain`, `nocolor`, `raw`, `unquoted` (raw/unquoted only on `{{message}}`, `{{fields}}` and event fields)",
                quote(t)
            ),
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
                        return Err(TemplateError::TooManyPlaceholders(source));
                    }
                    // The body runs to the first `}`, so a second `{` inside it
                    // would silently become part of the name and render empty.
                    // Checked here, before the name/spec split, so the whole
                    // class gets one variant instead of name-only detection.
                    if body.contains('{') {
                        return Err(TemplateError::NestedBrace(source));
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
        // Reused across placeholders: alignment measures the painted piece
        // before padding, so it needs a buffer of its own, but not a fresh
        // allocation per placeholder.
        let mut piece = String::new();

        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => out.push_str(text),
                Segment::Placeholder(ph) => {
                    piece.clear();
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

/// Whether a `:`-segment is one of the per-placeholder switches.
fn is_modifier(seg: &str) -> bool {
    matches!(seg, "plain" | "nocolor" | "no_color" | "raw" | "unquoted")
}

/// Split a `:`-suffix into its modifier part and its alignment part.
fn split_spec(spec: &str) -> (&str, &str) {
    // An alignment spec starts with `<`, `>` or `^`; every leading `.`-segment
    // that does not is a modifier. Decided by shape rather than by a separator
    // so the common `{name:>5}` keeps Rust's familiar spelling.
    if let Some(at) = spec.find(['<', '>', '^']) {
        if at == 0 {
            return ("", spec);
        }
        let prev = spec.as_bytes()[at - 1];
        if prev == b'.' || prev == b':' {
            return (&spec[..at - 1], &spec[at..]);
        }
    }
    (spec, "")
}

impl Placeholder {
    fn parse(body: &str, source: &str) -> Result<Self, TemplateError> {
        let (name, spec) = body.split_once(':').map_or((body, ""), |(n, s)| (n, s));

        if name.is_empty() {
            return Err(TemplateError::EmptyName(source.to_owned()));
        }
        // A typo inside the braces is the one class the brace check cannot see:
        // `{level }` and `{lev el}` would otherwise parse and render empty.
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(TemplateError::BadName(source.to_owned()));
        }

        // An alignment spec starts with `<`, `>` or `^`; every leading segment
        // that does not is a modifier. Decided by shape rather than by a
        // separator so the common `{name:>5}` keeps Rust's familiar spelling.
        let (mods, align_spec) = split_spec(spec);

        // A colon-suffix that names no alignment and no known modifier is the
        // old "bad spec" spelling; report it before the loop so `{level:wat}`
        // keeps its historical variant.
        // A colon word that is neither a modifier nor an alignment is a typo:
        // `{level:wat}` and `{level:>wat}` both fail at startup. The former is
        // reported as a bad modifier (the word was the mistake), the latter as
        // a bad spec (the width was).
        if !spec.is_empty()
            && !spec.contains(['<', '>', '^'])
            && !spec
                .split(['.', ':'])
                .filter(|m| !m.is_empty())
                .all(is_modifier)
        {
            return Err(TemplateError::BadSpec(source.to_owned()));
        }

        let mut quoting = Quoting::Quote;
        let mut plain = false;
        for m in mods.split(['.', ':']).filter(|m| !m.is_empty()) {
            match m {
                "plain" | "nocolor" | "no_color" => plain = true,
                "raw" | "unquoted" => quoting = Quoting::Raw,
                other => {
                    return Err(TemplateError::BadModifier(
                        other.to_owned(),
                        source.to_owned(),
                    ));
                }
            }
        }

        let mut content = content_of(name).unwrap_or(Content::Theme);
        if plain {
            content = Content::Plain;
        }

        let align = if align_spec.is_empty() {
            Align::None
        } else {
            parse_align_checked(align_spec, source)?
        };

        Ok(Self {
            name: name.to_owned(),
            align,
            content,
            quoting,
        })
    }

    fn write_value(
        &self,
        out: &mut String,
        cfg: &FormatConfig,
        input: &EventInput<'_>,
        painter: Painter<'_>,
    ) {
        // Per-element switches land on the painter, so a placeholder can be
        // rendered without colour without every other element following it.
        let p = match self.content {
            Content::Plain => painter.plain(),
            Content::Theme | Content::Span => painter.with_quoting(self.quoting),
            // `{fields}` follows the `quote_strings` knob but never takes
            // `:raw`: the tail's job is to stay unambiguous against the
            // separators, and one placeholder must not turn that off.
            Content::Fields => painter.with_quoting(Quoting::from_bool(cfg.quote_strings)),
        };

        match self.content {
            Content::Fields => {
                let rest = strip_message(input.fields);
                // Brackets match the default renderer, so `{fields}` and the
                // default tail look the same in a line that mixes both.
                if !rest.is_empty() {
                    out.push_str(&cfg.field_bracket_left);
                }
                write_pairs(out, cfg, p, &rest);
                if !rest.is_empty() {
                    out.push_str(&cfg.field_bracket_right);
                }
            }
            Content::Span => {
                out.push_str(&render_parents(cfg, p, input.parents));
                if let Some(leaf) = input.leaf {
                    out.push_str(&render_span_name(cfg, p, leaf));
                }
            }
            Content::Plain | Content::Theme => {
                self.write_named(out, cfg, input, p);
            }
        }
    }

    fn write_named(
        &self,
        out: &mut String,
        cfg: &FormatConfig,
        input: &EventInput<'_>,
        painter: Painter<'_>,
    ) {
        let name = self.name.as_str();

        match name {
            "level" => painter.write_level(out, input.level),
            "level_short" => painter.write_level_short(out, input.level),
            "target" => painter.write_target(out, input.target),
            // Quoting follows `with_quotes`: quoted makes the tail
            // unambiguous, raw keeps the journal's `MESSAGE` free of escapes.
            "message" => {
                if let Some(msg) = input.fields.get("message") {
                    painter.write_field_value(out, msg);
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
                painter.write_timestamp(out, &format_timestamp(fmt));
            }
            "file" => painter.write_file(out, input.file),
            "line" => painter.write_line(out, input.line),
            "spans" => out.push_str(&render_parents(cfg, painter, input.parents)),
            "leaf" => {
                if let Some(leaf) = input.leaf {
                    out.push_str(&render_span_name(cfg, painter, leaf));
                }
            }
            "span" => {
                out.push_str(&render_parents(cfg, painter, input.parents));
                if let Some(leaf) = input.leaf {
                    out.push_str(&render_span_name(cfg, painter, leaf));
                }
            }
            // Unreachable: `Content::Fields` dispatches before the name is
            // looked at.
            "fields" => {}
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

/// Same as [`write_fields`] for the `(name, value)` pairs `strip_message`
/// yields, so the `{fields}` path does not rebuild a map per event.
fn write_pairs(
    out: &mut String,
    cfg: &FormatConfig,
    painter: Painter<'_>,
    fields: &[(&str, &crate::visit::FieldValue)],
) {
    let mut first = true;
    for (name, value) in fields {
        if !first {
            out.push_str(&cfg.arguments_separator);
        }
        first = false;
        painter.write_field_name(out, name);
        out.push_str(&cfg.arguments_equality);
        painter.write_field_value(out, value);
    }
}

pub(crate) fn short_level(level: tracing::Level) -> &'static str {
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
            "INFO app::net: \"served\""
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
            "\"done\" {a: 1, b: true}"
        );
    }

    #[test]
    fn fields_without_extras_prints_no_bracket() {
        let f = fields(&[("message", FieldValue::Str("done".into()))]);
        assert_eq!(
            render("{message}{fields}", tracing::Level::INFO, &f),
            "\"done\""
        );
    }

    #[test]
    fn fields_honours_bracket_knobs() {
        let mut c = cfg();
        c.field_bracket_left = "[".into();
        c.field_bracket_right = "]".into();
        c.arguments_separator = "; ".into();
        c.arguments_equality = "=".into();
        let f = fields(&[("a", FieldValue::U64(1)), ("b", FieldValue::U64(2))]);
        let t = Template::parse("{fields}").expect("template parses");
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "app::net",
            file: "src/net.rs",
            line: 42,
            parents: &[],
            leaf: None,
            fields: &f,
        };
        assert_eq!(t.render(&c, &input, Painter::mono()), "[a=1; b=2]");
    }

    #[test]
    fn nested_brace_is_an_error() {
        assert!(matches!(
            Template::parse("{level{>5}}").unwrap_err(),
            TemplateError::NestedBrace(_)
        ));
    }

    #[test]
    fn placeholder_name_must_be_an_identifier() {
        // A body ending in `}` can hide a stray name `{level }` would accept;
        // punctuation inside the name is the same typo class as a nested brace.
        assert!(matches!(
            Template::parse("{level!}").unwrap_err(),
            TemplateError::BadName(_)
        ));
        assert!(matches!(
            Template::parse("{a b}").unwrap_err(),
            TemplateError::BadName(_)
        ));
    }

    #[test]
    fn width_and_count_errors_carry_the_template() {
        let wide = "{level:>999999}";
        match Template::parse(wide).unwrap_err() {
            TemplateError::WidthTooLarge(src) => assert_eq!(src, wide),
            other => panic!("expected WidthTooLarge, got {other:?}"),
        }
        let many = "{level}".repeat(2_000);
        match Template::parse(many.as_str()).unwrap_err() {
            TemplateError::TooManyPlaceholders(src) => assert_eq!(src, many),
            other => panic!("expected TooManyPlaceholders, got {other:?}"),
        }
    }

    #[test]
    fn quote_modifier_controls_string_values() {
        let cfg = FormatConfig::default();
        let mut f = crate::visit::FieldMap::new();
        f.insert("message", crate::visit::FieldValue::Str("done".into()));
        f.insert("path", crate::visit::FieldValue::Str("/api".into()));
        f.insert("rows", crate::visit::FieldValue::U64(3));
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "app",
            file: "",
            line: 0,
            parents: &[],
            leaf: None,
            fields: &f,
        };

        let quoted = Template::parse("{message} {fields}").unwrap();
        assert_eq!(
            quoted.render(&cfg, &input, Painter::mono()),
            "\"done\" {path: \"/api\", rows: 3}"
        );

        // `{fields}` follows `quote_strings`; only `{message}` takes `:raw`
        // on its own, so this test isolates the knob by using both together.
        let mut no_quotes = cfg.clone();
        no_quotes.quote_strings = false;
        let raw = Template::parse("{message:raw} {fields}").unwrap();
        assert_eq!(
            raw.render(&no_quotes, &input, Painter::mono()),
            "done {path: /api, rows: 3}"
        );
        // Without the knob the tail keeps its quotes even next to a raw message.
        let t = Template::parse("{message:raw} {fields}").unwrap();
        assert_eq!(
            t.render(&cfg, &input, Painter::mono()),
            "done {path: \"/api\", rows: 3}"
        );

        // Numbers are never quoted, so the modifier changes nothing for them.
        assert_eq!(
            Template::parse("{rows:raw}")
                .unwrap()
                .render(&cfg, &input, Painter::mono()),
            "3"
        );
    }

    #[test]
    fn layer_quotes_knob_reaches_the_default_renderer() {
        let cfg = FormatConfig {
            quote_strings: false,
            ..FormatConfig::default()
        };
        let mut f = crate::visit::FieldMap::new();
        f.insert("message", crate::visit::FieldValue::Str("hi".into()));
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "app",
            file: "",
            line: 0,
            parents: &[],
            leaf: None,
            fields: &f,
        };
        assert!(
            crate::format::event::render_event(&cfg, &input, Painter::mono()).contains(" hi"),
            "{}",
            crate::format::event::render_event(&cfg, &input, Painter::mono())
        );
    }

    #[test]
    fn plain_modifier_moves_only_that_element() {
        let cfg = FormatConfig::default();
        let mut f = crate::visit::FieldMap::new();
        f.insert("message", crate::visit::FieldValue::Str("msg".into()));
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "app",
            file: "",
            line: 0,
            parents: &[],
            leaf: None,
            fields: &f,
        };

        // `Painter::mono` emits no escapes either way; the point is that the
        // modifier parses, dispatches and leaves the other placeholders alone.
        let t = Template::parse("{level:plain} {level} {message:nocolor}").unwrap();
        assert_eq!(t.render(&cfg, &input, Painter::mono()), "INFO INFO \"msg\"");
        assert!(Template::parse("{message:plain}").is_ok());

        // `raw` parses anywhere; on a metadata slot it simply has no value to
        // act on, so the line is unchanged rather than an error at startup.
        assert!(Template::parse("{level:raw}").is_ok());
        assert!(matches!(
            Template::parse("{message:shiny:>6}").unwrap_err(),
            TemplateError::BadModifier(name, _) if name == "shiny"
        ));
    }

    #[test]
    fn modifier_and_width_compose() {
        let cfg = FormatConfig::default();
        let mut f = crate::visit::FieldMap::new();
        f.insert("message", crate::visit::FieldValue::Str("hi".into()));
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "app",
            file: "",
            line: 0,
            parents: &[],
            leaf: None,
            fields: &f,
        };
        // Alignment is measured on the rendered piece, so `raw` shortens it
        // by two and the padding follows.
        let t = Template::parse("{message:raw:>6}|").unwrap();
        assert_eq!(t.render(&cfg, &input, Painter::mono()), "    hi|");
        let q = Template::parse("{message:>6}|").unwrap();
        assert_eq!(q.render(&cfg, &input, Painter::mono()), "  \"hi\"|");
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
