//! Render a complete event line: `[ts] LEVEL [tid] target::span(arg=val)::leaf(): message {fields}`.

use std::fmt::Write as _;

use crate::format::span_chain::{SpanLink, render_chain, render_span_name};
use crate::format::{FormatConfig, TimestampFormat, current_thread_id_int, format_timestamp};
use crate::visit::{FieldMap, FieldValue};

#[cfg(feature = "colors")]
use crate::format::color::ColorTheme;

/// Inputs to event rendering. Bundled into a struct to keep the function
/// signature manageable.
pub(crate) struct EventInput<'a> {
    pub level: tracing::Level,
    pub target: &'a str,
    pub parents: &'a [SpanLink],
    /// The leaf span, if any (the most recent span enclosing the event).
    pub leaf: Option<&'a SpanLink>,
    pub fields: &'a FieldMap,
}

/// Render the line, with optional ANSI styling. Pass `theme = None` for
/// no styling regardless of feature.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_event(
    cfg: &FormatConfig,
    input: &EventInput<'_>,
    #[cfg(feature = "colors")] theme: Option<&ColorTheme>,
) -> String {
    let mut out = String::with_capacity(64);

    // 1. Optional timestamp prefix.
    if cfg.show_timestamp && cfg.timestamp_format != TimestampFormat::None {
        out.push_str(&format_timestamp(cfg.timestamp_format));
        out.push(' ');
    }

    // 2. Level (styled or plain).
    let level_str = input.level.as_str();
    #[cfg(feature = "colors")]
    {
        if let Some(t) = theme {
            let _ = write!(out, "{}", t.level_style(input.level).paint(level_str));
        } else {
            out.push_str(level_str);
        }
    }
    #[cfg(not(feature = "colors"))]
    {
        out.push_str(level_str);
    }

    out.push_str(&cfg.level_separator);

    // 3. Optional thread id.
    if cfg.show_thread_id {
        let tid = current_thread_id_int();
        out.push_str(&cfg.thread_id_prefix);
        #[cfg(feature = "colors")]
        {
            if let Some(t) = theme {
                let _ = write!(out, "{}", t.thread_id.paint(&tid));
            } else {
                out.push_str(&tid);
            }
        }
        #[cfg(not(feature = "colors"))]
        {
            out.push_str(&tid);
        }
        out.push_str(&cfg.thread_id_suffix);
    }

    // 4. Span chain (parents) + leaf span name.
    let chain = render_chain(cfg, input.target, input.parents);
    out.push_str(&chain);

    if let Some(leaf) = input.leaf {
        out.push_str(&render_span_name(cfg, leaf));
    }

    // 5. Message + non-message fields.
    let message = input.fields.get("message");
    let other_fields_count = input.fields.iter().filter(|(k, _)| **k != "message").count();

    if let Some(msg) = message {
        out.push_str(&cfg.message_separator);
        out.push_str(&msg.to_string());
    }

    if other_fields_count > 0 {
        out.push_str(&cfg.message_separator);
        out.push('{');
        let mut first = true;
        for (name, value) in input.fields.iter().filter(|(k, _)| **k != "message") {
            if !first {
                out.push_str(", ");
            }
            first = false;
            let _ = write!(out, "{name}: ");
            render_field_value(&mut out, value);
        }
        out.push('}');
    }

    out
}

fn render_field_value(out: &mut String, v: &FieldValue) {
    match v {
        FieldValue::Str(s) | FieldValue::Debug(s) | FieldValue::Error(s) => {
            // Quote string-like values for readability and to disambiguate
            // when a field contains separator characters like `, ` or `}`.
            out.push('"');
            for ch in s.chars() {
                if ch == '"' || ch == '\\' {
                    out.push('\\');
                }
                out.push(ch);
            }
            out.push('"');
        }
        other => {
            let _ = write!(out, "{other}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn map(kvs: &[(&'static str, FieldValue)]) -> FieldMap {
        let mut m = BTreeMap::new();
        for (k, v) in kvs {
            m.insert(*k, v.clone());
        }
        m
    }

    fn link(name: &'static str, kvs: &[(&'static str, FieldValue)]) -> SpanLink {
        SpanLink {
            name,
            fields: map(kvs),
        }
    }

    fn render_plain(cfg: &FormatConfig, input: &EventInput<'_>) -> String {
        #[cfg(feature = "colors")]
        return render_event(cfg, input, None);
        #[cfg(not(feature = "colors"))]
        return render_event(cfg, input);
    }

    #[test]
    fn bare_event_with_message() {
        let cfg = FormatConfig::default();
        let fields = map(&[("message", FieldValue::Str("hi".into()))]);
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "t",
            parents: &[],
            leaf: None,
            fields: &fields,
        };
        assert_eq!(render_plain(&cfg, &input), "INFO : hi");
    }

    #[test]
    fn event_inside_span() {
        let cfg = FormatConfig::default();
        let leaf = link("inner", &[("flag", FieldValue::Bool(true))]);
        let parents = vec![link("outer", &[])];
        let fields = map(&[("message", FieldValue::Str("hi".into()))]);
        let input = EventInput {
            level: tracing::Level::WARN,
            target: "t",
            parents: &parents,
            leaf: Some(&leaf),
            fields: &fields,
        };
        assert_eq!(render_plain(&cfg, &input), "WARN outer()::inner(flag: true): hi");
    }

    #[test]
    fn event_with_extra_fields() {
        let cfg = FormatConfig::default();
        let fields = map(&[
            ("message", FieldValue::Str("see".into())),
            ("count", FieldValue::U64(3)),
            ("name", FieldValue::Str("alice".into())),
        ]);
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "t",
            parents: &[],
            leaf: None,
            fields: &fields,
        };
        // Extra fields rendered alphabetically (BTreeMap order).
        assert_eq!(
            render_plain(&cfg, &input),
            "INFO : see: {count: 3, name: \"alice\"}"
        );
    }

    #[test]
    fn no_message_only_fields() {
        let cfg = FormatConfig::default();
        let fields = map(&[("k", FieldValue::I64(-1))]);
        let input = EventInput {
            level: tracing::Level::DEBUG,
            target: "t",
            parents: &[],
            leaf: None,
            fields: &fields,
        };
        assert_eq!(render_plain(&cfg, &input), "DEBUG : {k: -1}");
    }

    #[test]
    fn target_inclusion() {
        let cfg = FormatConfig {
            show_target: true,
            ..FormatConfig::default()
        };
        let leaf = link("inner", &[]);
        let fields = map(&[("message", FieldValue::Str("ok".into()))]);
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "my::tgt",
            parents: &[],
            leaf: Some(&leaf),
            fields: &fields,
        };
        assert_eq!(render_plain(&cfg, &input), "INFO my::tgt::inner(): ok");
    }

    #[test]
    fn quoted_strings_escape_quotes_and_backslashes() {
        let cfg = FormatConfig::default();
        let fields = map(&[("v", FieldValue::Str(r#"a"b\c"#.into()))]);
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "t",
            parents: &[],
            leaf: None,
            fields: &fields,
        };
        assert_eq!(render_plain(&cfg, &input), r#"INFO : {v: "a\"b\\c"}"#);
    }
}
