//! Render the parent-span chain that appears between the level and the
//! event message: `parent(arg=val)::child(...)::leaf`.

use std::fmt::Write as _;

use crate::format::FormatConfig;
use crate::visit::FieldMap;

/// One link in the chain — produced from a single `Span`.
#[derive(Debug, Clone)]
pub(crate) struct SpanLink {
    pub name: &'static str,
    pub fields: FieldMap,
}

/// Build a span name with its arguments: `name(field1: value1, field2: value2)`.
pub(crate) fn render_span_name(cfg: &FormatConfig, link: &SpanLink) -> String {
    let mut out = String::with_capacity(link.name.len() + 2);
    out.push_str(link.name);
    out.push_str(&cfg.function_bracket_left);

    let mut first = true;
    for (name, value) in &link.fields {
        if !first {
            out.push_str(&cfg.arguments_separator);
        }
        first = false;
        // ignore push_str result — String::write_* is infallible
        let _ = write!(out, "{}{}{}", name, cfg.arguments_equality, value);
    }

    out.push_str(&cfg.function_bracket_right);
    out
}

/// Build the chain of all parent spans (everything except the leaf, which
/// the event renderer handles separately) plus an optional `target::`
/// prefix.
pub(crate) fn render_chain(
    cfg: &FormatConfig,
    target: &str,
    parents: &[SpanLink],
) -> String {
    let mut chain = String::new();

    if cfg.show_target {
        chain.push_str(target);
        chain.push_str(&cfg.span_separator);
    }

    for link in parents {
        chain.push_str(&render_span_name(cfg, link));
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
        SpanLink {
            name,
            fields: m,
        }
    }

    #[test]
    fn span_name_without_fields() {
        let cfg = FormatConfig::default();
        assert_eq!(render_span_name(&cfg, &link("foo", &[])), "foo()");
    }

    #[test]
    fn span_name_with_fields_uses_separators() {
        let cfg = FormatConfig::default();
        let l = link(
            "foo",
            &[
                ("a", FieldValue::I64(1)),
                ("b", FieldValue::Str("two".into())),
            ],
        );
        assert_eq!(render_span_name(&cfg, &l), "foo(a: 1, b: two)");
    }

    #[test]
    fn chain_joins_parents_with_separator() {
        let cfg = FormatConfig::default();
        let chain = render_chain(
            &cfg,
            "irrelevant",
            &[link("outer", &[]), link("middle", &[("x", FieldValue::Bool(true))])],
        );
        assert_eq!(chain, "outer()::middle(x: true)::");
    }

    #[test]
    fn chain_includes_target_when_enabled() {
        let cfg = FormatConfig {
            show_target: true,
            ..FormatConfig::default()
        };
        let chain = render_chain(&cfg, "my_target", &[link("outer", &[])]);
        assert_eq!(chain, "my_target::outer()::");
    }

    #[test]
    fn chain_with_no_parents_and_no_target_is_empty() {
        let cfg = FormatConfig::default();
        assert_eq!(render_chain(&cfg, "ignored", &[]), "");
    }

    #[test]
    fn custom_brackets_and_separator() {
        let cfg = FormatConfig {
            function_bracket_left: "[".into(),
            function_bracket_right: "]".into(),
            arguments_equality: "=".into(),
            ..FormatConfig::default()
        };
        let l = link("fn", &[("k", FieldValue::U64(7))]);
        assert_eq!(render_span_name(&cfg, &l), "fn[k=7]");
    }
}
