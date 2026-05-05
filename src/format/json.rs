//! Render an event as a single-line JSON object.
//!
//! Schema:
//! ```text
//! {
//!   "timestamp": "<rfc3339 or unix>",   // omitted if show_timestamp is false
//!   "level":     "INFO" | ...,
//!   "message":   "<the event's message field, or "" if none>",
//!   "target":    "<module path>",       // omitted if show_target is false
//!   "thread_id": <number|string>,       // present only when show_thread_id is true
//!   "span_chain": [{"name": "...", "fields": {...}}, ...],
//!   "fields":    { ...event fields minus "message"... }
//! }
//! ```

use serde_json::{Map, Value};

use crate::format::event::EventInput;
use crate::format::span_chain::SpanLink;
use crate::format::{FormatConfig, TimestampFormat, current_thread_id_int, format_timestamp};
use crate::visit::FieldValue;

pub(crate) fn render_event_json(cfg: &FormatConfig, input: &EventInput<'_>) -> String {
    let mut obj = Map::new();

    if cfg.show_timestamp && cfg.timestamp_format != TimestampFormat::None {
        obj.insert(
            "timestamp".into(),
            Value::String(format_timestamp(cfg.timestamp_format)),
        );
    }

    obj.insert(
        "level".into(),
        Value::String(input.level.as_str().to_owned()),
    );

    let message = input
        .fields
        .get("message")
        .map(ToString::to_string)
        .unwrap_or_default();
    obj.insert("message".into(), Value::String(message));

    if cfg.show_target {
        obj.insert("target".into(), Value::String(input.target.to_owned()));
    }

    if cfg.show_thread_id {
        let tid = current_thread_id_int();
        let v = tid
            .parse::<u64>()
            .map_or_else(|_| Value::String(tid.clone()), Value::from);
        obj.insert("thread_id".into(), v);
    }

    let mut chain: Vec<Value> = Vec::with_capacity(input.parents.len() + usize::from(input.leaf.is_some()));
    for link in input.parents {
        chain.push(span_link_to_json(link));
    }
    if let Some(leaf) = input.leaf {
        chain.push(span_link_to_json(leaf));
    }
    obj.insert("span_chain".into(), Value::Array(chain));

    let mut fields = Map::new();
    for (name, value) in input.fields.iter().filter(|(k, _)| **k != "message") {
        fields.insert((*name).to_owned(), field_value_to_json(value));
    }
    obj.insert("fields".into(), Value::Object(fields));

    // `serde_json::to_string` on a `Map<String, Value>` cannot fail for the
    // values we construct here (no maps with non-string keys, no custom
    // Serialize impls). Fall back to a minimal valid object for paranoia.
    serde_json::to_string(&Value::Object(obj)).unwrap_or_else(|_| String::from("{}"))
}

fn span_link_to_json(link: &SpanLink) -> Value {
    let mut obj = Map::new();
    obj.insert("name".into(), Value::String(link.name.to_owned()));
    let mut fields = Map::new();
    for (name, value) in &link.fields {
        fields.insert((*name).to_owned(), field_value_to_json(value));
    }
    obj.insert("fields".into(), Value::Object(fields));
    Value::Object(obj)
}

fn field_value_to_json(v: &FieldValue) -> Value {
    match v {
        FieldValue::Bool(b) => Value::from(*b),
        FieldValue::I64(i) => Value::from(*i),
        FieldValue::U64(u) => Value::from(*u),
        // `Value::from(f64)` returns Null for NaN/±Infinity, which matches
        // tracing-subscriber and JS's JSON.stringify.
        FieldValue::F64(f) => Value::from(*f),
        FieldValue::Str(s) | FieldValue::Debug(s) | FieldValue::Error(s) => {
            Value::String(s.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::RenderMode;
    use crate::visit::FieldMap;
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

    fn json_cfg() -> FormatConfig {
        FormatConfig {
            mode: RenderMode::Json,
            show_target: true,
            show_timestamp: false, // off in tests so output is deterministic
            timestamp_format: TimestampFormat::None,
            use_level_prefix: false,
            ..FormatConfig::default()
        }
    }

    #[test]
    fn bare_event_basic_shape() {
        let cfg = json_cfg();
        let fields = map(&[("message", FieldValue::Str("hi".into()))]);
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "my::tgt",
            parents: &[],
            leaf: None,
            fields: &fields,
        };
        let s = render_event_json(&cfg, &input);
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["level"], "INFO");
        assert_eq!(v["message"], "hi");
        assert_eq!(v["target"], "my::tgt");
        assert_eq!(v["span_chain"], serde_json::json!([]));
        assert_eq!(v["fields"], serde_json::json!({}));
        assert!(v.get("thread_id").is_none());
        assert!(v.get("timestamp").is_none());
    }

    #[test]
    fn span_chain_root_to_leaf_order() {
        let cfg = json_cfg();
        let outer = link("outer", &[("seed", FieldValue::U64(7))]);
        let middle = link("middle", &[]);
        let leaf = link("leaf", &[("flag", FieldValue::Bool(true))]);
        let parents = vec![outer, middle];
        let fields = map(&[("message", FieldValue::Str("ok".into()))]);
        let input = EventInput {
            level: tracing::Level::WARN,
            target: "t",
            parents: &parents,
            leaf: Some(&leaf),
            fields: &fields,
        };
        let v: Value = serde_json::from_str(&render_event_json(&cfg, &input)).unwrap();
        let chain = v["span_chain"].as_array().unwrap();
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0]["name"], "outer");
        assert_eq!(chain[0]["fields"]["seed"], 7);
        assert_eq!(chain[1]["name"], "middle");
        assert_eq!(chain[1]["fields"], serde_json::json!({}));
        assert_eq!(chain[2]["name"], "leaf");
        assert_eq!(chain[2]["fields"]["flag"], true);
    }

    #[test]
    fn types_are_preserved() {
        let cfg = json_cfg();
        let fields = map(&[
            ("b", FieldValue::Bool(false)),
            ("i", FieldValue::I64(-3)),
            ("u", FieldValue::U64(42)),
            ("f", FieldValue::F64(1.5)),
            ("s", FieldValue::Str("hi".into())),
            ("d", FieldValue::Debug("Foo { x: 1 }".into())),
            ("e", FieldValue::Error("oops".into())),
        ]);
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "t",
            parents: &[],
            leaf: None,
            fields: &fields,
        };
        let v: Value = serde_json::from_str(&render_event_json(&cfg, &input)).unwrap();
        let f = &v["fields"];
        assert_eq!(f["b"], false);
        assert_eq!(f["i"], -3);
        assert_eq!(f["u"], 42);
        assert_eq!(f["f"], 1.5);
        assert_eq!(f["s"], "hi");
        assert_eq!(f["d"], "Foo { x: 1 }");
        assert_eq!(f["e"], "oops");
    }

    #[test]
    fn non_finite_f64_becomes_null() {
        let cfg = json_cfg();
        let fields = map(&[
            ("nan", FieldValue::F64(f64::NAN)),
            ("inf", FieldValue::F64(f64::INFINITY)),
            ("ninf", FieldValue::F64(f64::NEG_INFINITY)),
        ]);
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "t",
            parents: &[],
            leaf: None,
            fields: &fields,
        };
        let v: Value = serde_json::from_str(&render_event_json(&cfg, &input)).unwrap();
        assert!(v["fields"]["nan"].is_null());
        assert!(v["fields"]["inf"].is_null());
        assert!(v["fields"]["ninf"].is_null());
    }

    #[test]
    fn target_omitted_when_disabled() {
        let cfg = FormatConfig {
            show_target: false,
            ..json_cfg()
        };
        let fields = map(&[("message", FieldValue::Str("ok".into()))]);
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "t",
            parents: &[],
            leaf: None,
            fields: &fields,
        };
        let v: Value = serde_json::from_str(&render_event_json(&cfg, &input)).unwrap();
        assert!(v.get("target").is_none());
    }

    #[test]
    fn thread_id_present_when_enabled() {
        let cfg = FormatConfig {
            show_thread_id: true,
            ..json_cfg()
        };
        let fields = map(&[("message", FieldValue::Str("ok".into()))]);
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "t",
            parents: &[],
            leaf: None,
            fields: &fields,
        };
        let v: Value = serde_json::from_str(&render_event_json(&cfg, &input)).unwrap();
        assert!(v.get("thread_id").is_some(), "thread_id missing: {v}");
    }

    #[test]
    fn missing_message_renders_empty_string() {
        let cfg = json_cfg();
        let fields = map(&[("k", FieldValue::I64(1))]);
        let input = EventInput {
            level: tracing::Level::INFO,
            target: "t",
            parents: &[],
            leaf: None,
            fields: &fields,
        };
        let v: Value = serde_json::from_str(&render_event_json(&cfg, &input)).unwrap();
        assert_eq!(v["message"], "");
        assert_eq!(v["fields"]["k"], 1);
    }
}
