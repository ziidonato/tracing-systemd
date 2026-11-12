//! Render a complete event line: `[ts] LEVEL [tid] target::span(arg=val)::leaf(): message {fields}`.

use crate::format::paint::Painter;
use crate::format::span_chain::{SpanLink, render_chain, render_span_name};
use crate::format::{FormatConfig, TimestampFormat, current_thread_id_int, format_timestamp};
use crate::visit::FieldMap;

/// Inputs to event rendering. Bundled into a struct to keep the function
/// signature manageable.
pub(crate) struct EventInput<'a> {
    pub level: tracing::Level,
    pub target: &'a str,
    /// `file!()` of the event's call site, or `""` when not recorded.
    pub file: &'a str,
    /// Line of the event's call site, or `0` when not recorded.
    pub line: u32,
    pub parents: &'a [SpanLink],
    /// The leaf span, if any (the most recent span enclosing the event).
    pub leaf: Option<&'a SpanLink>,
    pub fields: &'a FieldMap,
}

/// Render the line. Styling comes from `painter`; pass [`Painter::mono`] for
/// plain text.
pub(crate) fn render_event(
    cfg: &FormatConfig,
    input: &EventInput<'_>,
    painter: Painter<'_>,
) -> String {
    let mut out = String::with_capacity(64);

    if cfg.timestamp_format != TimestampFormat::None {
        out.push_str(&format_timestamp(cfg.timestamp_format));
        out.push_str(&cfg.level_separator);
    }

    painter.write_level(&mut out, input.level);
    out.push_str(&cfg.level_separator);

    if cfg.show_thread_id {
        let tid = current_thread_id_int();
        out.push_str(&cfg.thread_id_prefix);
        painter.write_thread_id(&mut out, &tid);
        out.push_str(&cfg.thread_id_suffix);
    }

    out.push_str(&render_chain(cfg, painter, input.target, input.parents));

    if let Some(leaf) = input.leaf {
        out.push_str(&render_span_name(cfg, painter, leaf));
    }

    if let Some(msg) = input.fields.get("message") {
        out.push_str(&cfg.message_separator);
        out.push_str(&msg.to_string());
    }

    // Peeking first keeps the bracket, and the separator before it, absent
    // when the event carries no fields beyond the message.
    let mut others = input
        .fields
        .iter()
        .filter(|(k, _)| **k != "message")
        .peekable();
    if others.peek().is_some() {
        out.push_str(&cfg.message_separator);
        out.push_str(&cfg.field_bracket_left);
        let mut first = true;
        for (name, value) in others {
            if !first {
                out.push_str(&cfg.arguments_separator);
            }
            first = false;
            painter.write_field_name(&mut out, name);
            out.push_str(&cfg.arguments_equality);
            painter.write_field_value(&mut out, value);
        }
        out.push_str(&cfg.field_bracket_right);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visit::FieldValue;
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
        render_event(cfg, input, Painter::mono())
    }

    #[test]
    fn bare_event_with_message() {
        let cfg = FormatConfig::default();
        let fields = map(&[("message", FieldValue::Str("hi".into()))]);
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "t",
            file: "src/f.rs",
            line: 7,
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
            file: "src/f.rs",
            line: 7,
            parents: &parents,
            leaf: Some(&leaf),
            fields: &fields,
        };
        assert_eq!(
            render_plain(&cfg, &input),
            "WARN outer()::inner(flag: true): hi"
        );
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
            file: "src/f.rs",
            line: 7,
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
    fn event_fields_honour_arguments_separators() {
        let cfg = FormatConfig {
            field_bracket_left: "[".into(),
            field_bracket_right: "]".into(),
            arguments_equality: "=".into(),
            arguments_separator: ",".into(),
            ..FormatConfig::default()
        };
        let fields = map(&[
            ("message", FieldValue::Str("m".into())),
            ("a", FieldValue::U64(1)),
            ("b", FieldValue::U64(2)),
        ]);
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "t",
            file: "src/f.rs",
            line: 7,
            parents: &[],
            leaf: None,
            fields: &fields,
        };
        assert_eq!(render_plain(&cfg, &input), "INFO : m: [a=1,b=2]");
    }

    #[test]
    fn no_message_only_fields() {
        let cfg = FormatConfig::default();
        let fields = map(&[("k", FieldValue::I64(-1))]);
        let input = EventInput {
            level: tracing::Level::DEBUG,
            target: "t",
            file: "src/f.rs",
            line: 7,
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
            file: "src/f.rs",
            line: 7,
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
            file: "src/f.rs",
            line: 7,
            parents: &[],
            leaf: None,
            fields: &fields,
        };
        assert_eq!(render_plain(&cfg, &input), r#"INFO : {v: "a\"b\\c"}"#);
    }

    #[test]
    fn timestamp_uses_level_separator() {
        let cfg = FormatConfig {
            timestamp_format: TimestampFormat::Uptime,
            ..FormatConfig::default()
        };
        let fields = map(&[("message", FieldValue::Str("hi".into()))]);
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "t",
            file: "src/f.rs",
            line: 7,
            parents: &[],
            leaf: None,
            fields: &fields,
        };
        let out = render_plain(&cfg, &input);
        let ts = out.split("INFO").next().unwrap();
        assert!(ts.contains('.'), "got {out:?}");
        assert!(ts.ends_with(' '), "got {out:?}");
    }
}
