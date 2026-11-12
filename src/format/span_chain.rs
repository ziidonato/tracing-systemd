//! Render the parent-span chain that appears between the level and the
//! event message: `parent(arg=val)::child(...)::leaf`.

use crate::format::FormatConfig;
use crate::format::paint::{Painter, write_fields};
use crate::visit::FieldMap;

/// One link in the chain, produced from a single `Span`.
#[derive(Debug, Clone)]
pub(crate) struct SpanLink {
    pub name: &'static str,
    pub fields: FieldMap,
}

/// Build a span name with its arguments: `name(field1: value1, field2: value2)`.
pub(crate) fn render_span_name(
    cfg: &FormatConfig,
    painter: Painter<'_>,
    link: &SpanLink,
) -> String {
    let mut out = String::with_capacity(link.name.len() + 2);
    painter.write_span_name(&mut out, link.name);
    out.push_str(&cfg.function_bracket_left);
    write_fields(&mut out, cfg, painter, &link.fields);
    out.push_str(&cfg.function_bracket_right);
    out
}

/// Build the chain of all parent spans (everything except the leaf, which
/// the event renderer handles separately) plus an optional `target::`
/// prefix.
pub(crate) fn render_chain(
    cfg: &FormatConfig,
    painter: Painter<'_>,
    target: &str,
    parents: &[SpanLink],
) -> String {
    let mut chain = String::new();

    if cfg.show_target {
        painter.write_target(&mut chain, target);
        chain.push_str(&cfg.span_separator);
    }

    for link in parents {
        chain.push_str(&render_span_name(cfg, painter, link));
        chain.push_str(&cfg.span_separator);
    }

    chain
}

/// Render only the parent spans, with no `target::` prefix.
///
/// [`render_chain`] prepends the target when `show_target` is set, which a
/// template must not inherit: `{spans}` names the spans, and silently emitting
/// a leading `::` there would be a formatting bug the user cannot configure away.
pub(crate) fn render_parents(
    cfg: &FormatConfig,
    painter: Painter<'_>,
    parents: &[SpanLink],
) -> String {
    let mut chain = String::new();
    for link in parents {
        chain.push_str(&render_span_name(cfg, painter, link));
        chain.push_str(&cfg.span_separator);
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visit::FieldValue;
    use std::collections::BTreeMap;

    fn link(name: &'static str, kvs: &[(&'static str, FieldValue)]) -> SpanLink {
        let mut m = BTreeMap::new();
        for (k, v) in kvs {
            m.insert(*k, v.clone());
        }
        SpanLink { name, fields: m }
    }

    fn mono() -> Painter<'static> {
        Painter::mono()
    }

    #[test]
    fn span_name_without_fields() {
        let cfg = FormatConfig::default();
        assert_eq!(
            render_span_name(&cfg, Painter::mono(), &link("foo", &[])),
            "foo()"
        );
    }

    #[test]
    fn span_name_with_fields_uses_separators() {
        let cfg = FormatConfig::default();
        let p = mono();
        let l = link(
            "foo",
            &[
                ("a", FieldValue::I64(1)),
                ("b", FieldValue::Str("two".into())),
            ],
        );
        assert_eq!(render_span_name(&cfg, p, &l), "foo(a: 1, b: \"two\")");
    }

    #[test]
    fn string_args_are_quoted_so_separators_stay_unambiguous() {
        let cfg = FormatConfig::default();
        let p = mono();
        let l = link("f", &[("s", FieldValue::Str("a, b".into()))]);
        assert_eq!(render_span_name(&cfg, p, &l), "f(s: \"a, b\")");
    }

    #[test]
    fn chain_joins_parents_with_separator() {
        let cfg = FormatConfig::default();
        let p = mono();
        let chain = render_chain(
            &cfg,
            p,
            "irrelevant",
            &[
                link("outer", &[]),
                link("middle", &[("x", FieldValue::Bool(true))]),
            ],
        );
        assert_eq!(chain, "outer()::middle(x: true)::");
    }

    #[test]
    fn chain_includes_target_when_enabled() {
        let cfg = FormatConfig {
            show_target: true,
            ..FormatConfig::default()
        };
        let p = mono();
        let chain = render_chain(&cfg, p, "my_target", &[link("outer", &[])]);
        assert_eq!(chain, "my_target::outer()::");
    }

    #[test]
    fn chain_with_no_parents_and_no_target_is_empty() {
        let cfg = FormatConfig::default();
        assert_eq!(render_chain(&cfg, Painter::mono(), "ignored", &[]), "");
    }

    #[test]
    fn custom_brackets_and_separator() {
        let cfg = FormatConfig {
            function_bracket_left: "[".into(),
            function_bracket_right: "]".into(),
            arguments_equality: "=".into(),
            ..FormatConfig::default()
        };
        let p = mono();
        let l = link("fn", &[("k", FieldValue::U64(7))]);
        assert_eq!(render_span_name(&cfg, p, &l), "fn[k=7]");
    }
}
