//! Styling seam between the renderers and the `colors` feature.
//!
//! `Painter` exists so `event.rs` / `span_chain.rs` contain no `#[cfg]`
//! branches: without the `colors` feature the type is a zero-cost no-op, so
//! the renderers always take one and always write through it.

use crate::visit::FieldValue;

#[cfg(feature = "colors")]
use crate::format::color::ColorTheme;

/// Whether string-like field values get the surrounding quotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Quoting {
    Quote,
    Raw,
}

impl Quoting {
    pub(crate) fn from_bool(quote: bool) -> Self {
        if quote { Self::Quote } else { Self::Raw }
    }
}

/// Applies [`ColorTheme`] styles to rendered fragments.
#[derive(Clone, Copy)]
pub(crate) struct Painter<'a> {
    #[cfg(feature = "colors")]
    theme: Option<&'a ColorTheme>,
    #[cfg(not(feature = "colors"))]
    _marker: std::marker::PhantomData<&'a ()>,
    quoting: Quoting,
}

// Without the `colors` feature every method ignores `self`; keeping the
// receiver keeps the call sites identical across both builds.
#[cfg_attr(not(feature = "colors"), allow(clippy::unused_self))]
impl Painter<'_> {
    /// No styling. Also the only constructor without the `colors` feature.
    pub(crate) fn mono() -> Painter<'static> {
        Painter {
            #[cfg(feature = "colors")]
            theme: None,
            #[cfg(not(feature = "colors"))]
            _marker: std::marker::PhantomData,
            quoting: Quoting::Quote,
        }
    }

    #[cfg(feature = "colors")]
    pub(crate) fn themed(theme: &ColorTheme) -> Painter<'_> {
        Painter {
            theme: Some(theme),
            quoting: Quoting::Quote,
        }
    }

    pub(crate) fn write_level(self, out: &mut String, level: tracing::Level) {
        self.write_level_text(out, level, level.as_str());
    }

    /// `{level_short}` shares the level's style but not its text: the theme
    /// colours `INFO`, so it must colour `INF` too or one line shows both.
    pub(crate) fn write_level_short(self, out: &mut String, level: tracing::Level) {
        self.write_level_text(out, level, crate::format::template::short_level(level));
    }

    fn write_level_text(self, out: &mut String, level: tracing::Level, text: &str) {
        #[cfg(feature = "colors")]
        if let Some(theme) = self.theme {
            use std::fmt::Write as _;
            let _ = write!(out, "{}", theme.level_style(level).paint(text));
            return;
        }
        out.push_str(text);
    }

    pub(crate) fn write_thread_id(self, out: &mut String, tid: &str) {
        #[cfg(feature = "colors")]
        if let Some(theme) = self.theme {
            use std::fmt::Write as _;
            let _ = write!(out, "{}", theme.thread_id.paint(tid));
            return;
        }
        out.push_str(tid);
    }

    pub(crate) fn write_target(self, out: &mut String, target: &str) {
        #[cfg(feature = "colors")]
        if let Some(theme) = self.theme {
            use std::fmt::Write as _;
            let _ = write!(out, "{}", theme.target.paint(target));
            return;
        }
        out.push_str(target);
    }

    pub(crate) fn write_span_name(self, out: &mut String, name: &str) {
        #[cfg(feature = "colors")]
        if let Some(theme) = self.theme {
            use std::fmt::Write as _;
            let _ = write!(out, "{}", theme.span_name.paint(name));
            return;
        }
        out.push_str(name);
    }

    pub(crate) fn write_timestamp(self, out: &mut String, stamp: &str) {
        #[cfg(feature = "colors")]
        if let Some(theme) = self.theme {
            use std::fmt::Write as _;
            let _ = write!(out, "{}", theme.timestamp.paint(stamp));
            return;
        }
        out.push_str(stamp);
    }

    pub(crate) fn write_file(self, out: &mut String, path: &str) {
        #[cfg(feature = "colors")]
        if let Some(theme) = self.theme {
            use std::fmt::Write as _;
            let _ = write!(out, "{}", theme.file.paint(path));
            return;
        }
        out.push_str(path);
    }

    pub(crate) fn write_line(self, out: &mut String, line: u32) {
        #[cfg(feature = "colors")]
        if let Some(theme) = self.theme {
            use std::fmt::Write as _;
            let _ = write!(out, "{}", theme.line.paint(line.to_string()));
            return;
        }
        out.push_str(&line.to_string());
    }

    pub(crate) fn write_field_name(self, out: &mut String, name: &str) {
        #[cfg(feature = "colors")]
        if let Some(theme) = self.theme {
            use std::fmt::Write as _;
            let _ = write!(out, "{}", theme.field_name.paint(name));
            return;
        }
        out.push_str(name);
    }

    pub(crate) fn write_field_value(self, out: &mut String, value: &FieldValue) {
        self.write_field_value_with(out, value, self.quoting);
    }

    /// Same as [`Self::write_field_value`] with quoting chosen per call, for
    /// the message slot: quotes make a tail unambiguous, but they also land in
    /// the journal's `MESSAGE`, so a user may want none there.
    pub(crate) fn write_field_value_with(
        self,
        out: &mut String,
        value: &FieldValue,
        quoting: Quoting,
    ) {
        #[cfg(feature = "colors")]
        if let Some(theme) = self.theme {
            use std::fmt::Write as _;
            let style = theme.field_value;
            return match (value, quoting) {
                (
                    FieldValue::Str(s) | FieldValue::Debug(s) | FieldValue::Error(s),
                    Quoting::Quote,
                ) => {
                    let _ = write!(out, "{}", style.paint(format!("{}", Quoted(s))));
                }
                (
                    FieldValue::Str(s) | FieldValue::Debug(s) | FieldValue::Error(s),
                    Quoting::Raw,
                ) => {
                    let _ = write!(out, "{}", style.paint(s.as_str()));
                }
                (other, _) => {
                    let _ = write!(out, "{}", style.paint(other.to_string()));
                }
            };
        }
        match (value, quoting) {
            (FieldValue::Str(s) | FieldValue::Debug(s) | FieldValue::Error(s), Quoting::Quote) => {
                use std::fmt::Write as _;
                let _ = write!(out, "{}", Quoted(s));
            }
            (FieldValue::Str(s) | FieldValue::Debug(s) | FieldValue::Error(s), Quoting::Raw) => {
                out.push_str(s);
            }
            (other, _) => out.push_str(&other.to_string()),
        }
    }

    /// This painter with per-element colour switched off.
    #[must_use]
    pub(crate) fn plain(self) -> Self {
        Self {
            #[cfg(feature = "colors")]
            theme: None,
            quoting: self.quoting,
        }
    }

    /// This painter with the default quoting for field values.
    #[must_use]
    pub(crate) fn with_quoting(self, quoting: Quoting) -> Self {
        Self {
            #[cfg(feature = "colors")]
            theme: self.theme,
            quoting,
        }
    }
}

/// A string-like field value rendered with surrounding quotes and escaped
/// inner quotes/backslashes, so values containing `, ` or `}` stay unambiguous.
pub(crate) struct Quoted<'a>(pub(crate) &'a str);

impl std::fmt::Display for Quoted<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("\"")?;
        for ch in self.0.chars() {
            if ch == '"' || ch == '\\' {
                f.write_str("\\")?;
            }
            write!(f, "{ch}")?;
        }
        f.write_str("\"")
    }
}

/// Render `name<equality>value<separator>...` for a field map.
pub(crate) fn write_fields(
    out: &mut String,
    cfg: &crate::format::FormatConfig,
    painter: Painter<'_>,
    fields: &crate::visit::FieldMap,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_escapes_quotes_and_backslashes() {
        assert_eq!(Quoted(r#"a"b\c"#).to_string(), r#""a\"b\\c""#);
    }

    #[test]
    fn mono_painter_writes_plain_text() {
        let p = Painter::mono();
        let mut out = String::new();
        p.write_level(&mut out, tracing::Level::WARN);
        p.write_target(&mut out, "t");
        p.write_span_name(&mut out, "s");
        p.write_field_name(&mut out, "k");
        p.write_field_value(&mut out, &FieldValue::Str("v".into()));
        assert_eq!(out, "WARNtsk\"v\"");
    }
}
