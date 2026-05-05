//! Integration tests against the public crate surface only.
//!
//! These tests intentionally do not use any `pub(crate)` item — they
//! verify that the API documented in the README is enough to drive the
//! layer through realistic scenarios.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use tracing::{Level, error, info, info_span, instrument, warn};
use tracing_subscriber::prelude::*;
use tracing_systemd::{Output, SystemdLayer, TimestampFormat};

#[derive(Clone, Default)]
struct Buf(Arc<Mutex<Vec<u8>>>);

impl Write for Buf {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn capture<F: FnOnce()>(layer: SystemdLayer, body: F) -> String {
    let buf = Buf::default();
    let captured = buf.0.clone();
    let layer = layer.with_output(Output::writer(buf));
    tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), body);
    String::from_utf8(captured.lock().unwrap().clone()).expect("utf-8 output")
}

#[test]
fn instrument_attribute_renders_span_chain() {
    #[instrument(fields(seed = 7))]
    fn outer() {
        inner();
    }

    #[instrument]
    fn inner() {
        info!("hi");
    }

    let layer = SystemdLayer::stdout().with_level_prefix(false);
    let out = capture(layer, outer);
    assert!(out.contains("outer(seed: 7)"), "got: {out}");
    assert!(out.contains("::inner()"), "got: {out}");
    assert!(out.contains(": hi"), "got: {out}");
}

#[test]
fn level_prefix_is_per_event_priority() {
    let layer = SystemdLayer::stdout().with_level_prefix(true);
    let out = capture(layer, || {
        info!("a");
        warn!("b");
        error!("c");
    });
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "got: {out:?}");
    assert!(lines[0].starts_with("<5>INFO"));
    assert!(lines[1].starts_with("<4>WARN"));
    assert!(lines[2].starts_with("<3>ERROR"));
}

#[test]
fn target_appears_when_enabled() {
    let layer = SystemdLayer::stdout()
        .with_target(true)
        .with_level_prefix(false);
    let out = capture(layer, || {
        info!("ok");
    });
    // The target for events emitted from this test crate is the test crate name.
    assert!(out.contains("integration"), "got: {out}");
}

#[test]
fn timestamp_uptime_prefixes_each_line() {
    let layer = SystemdLayer::stdout()
        .with_timestamp_format(TimestampFormat::Uptime)
        .with_level_prefix(false);
    let out = capture(layer, || {
        info!("first");
        info!("second");
    });
    for line in out.lines() {
        // Uptime format is "<secs>.<millis> ", so the first token must
        // contain a dot before the level keyword.
        let first_token = line.split_whitespace().next().unwrap_or("");
        assert!(first_token.contains('.'), "no timestamp in: {line}");
    }
}

#[test]
fn custom_brackets_and_arg_format_round_trip() {
    let layer = SystemdLayer::stdout()
        .with_level_prefix(false)
        .with_function_bracket_left("[")
        .with_function_bracket_right("]")
        .with_arguments_equality("=")
        .with_arguments_separator(",");
    let out = capture(layer, || {
        let span = info_span!("worker", id = 1u64, name = "alice");
        let _g = span.enter();
        info!("hi");
    });
    assert!(out.contains("worker[id=1,name=alice]"), "got: {out}");
}

#[test]
fn level_levels_compile() {
    // Confirms public Level re-exports / accepted types haven't drifted.
    fn _accepts(_l: Level) {}
    _accepts(Level::INFO);
}

#[cfg(feature = "json")]
mod json {
    use super::Buf;
    use serde_json::Value;
    use tracing::{Level, debug, error, info, info_span, instrument, warn};
    use tracing_subscriber::prelude::*;
    use tracing_systemd::{Output, SystemdLayer};

    fn capture_json<F: FnOnce()>(layer: SystemdLayer, body: F) -> Vec<Value> {
        let buf = Buf::default();
        let captured = buf.0.clone();
        let layer = layer.with_output(Output::writer(buf));
        tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), body);
        let bytes = captured.lock().unwrap().clone();
        let s = String::from_utf8(bytes).expect("utf-8");
        s.lines()
            .map(|l| {
                serde_json::from_str(l).unwrap_or_else(|e| panic!("bad json: {l:?}: {e}"))
            })
            .collect()
    }

    #[test]
    fn round_trip_basic_event() {
        let lines = capture_json(SystemdLayer::json(), || {
            info!("hello");
        });
        assert_eq!(lines.len(), 1);
        let v = &lines[0];
        assert_eq!(v["level"], "INFO");
        assert_eq!(v["message"], "hello");
        // target is on by default in JSON mode; for tests this is the test crate.
        assert!(v["target"].is_string());
        assert_eq!(v["span_chain"], serde_json::json!([]));
        assert_eq!(v["fields"], serde_json::json!({}));
        // RFC 3339 default: "YYYY-MM-DDTHH:MM:SS.mmmZ" → 24 chars, ends with Z.
        let ts = v["timestamp"].as_str().unwrap();
        assert_eq!(ts.len(), 24);
        assert!(ts.ends_with('Z'));
    }

    #[test]
    fn span_chain_is_root_to_leaf() {
        #[instrument(fields(seed = 7))]
        fn outer() {
            inner();
        }
        #[instrument]
        fn inner() {
            info!("hi");
        }

        let lines = capture_json(SystemdLayer::json(), outer);
        assert_eq!(lines.len(), 1);
        let chain = lines[0]["span_chain"].as_array().unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0]["name"], "outer");
        assert_eq!(chain[0]["fields"]["seed"], 7);
        assert_eq!(chain[1]["name"], "inner");
        assert_eq!(chain[1]["fields"], serde_json::json!({}));
    }

    #[test]
    fn levels_map_to_strings() {
        let lines = capture_json(SystemdLayer::json(), || {
            error!("e");
            warn!("w");
            info!("i");
            debug!("d");
        });
        // debug events are filtered by default unless the registry has a max
        // level filter installed; tracing's default is TRACE, so all four
        // should appear here.
        let levels: Vec<&str> = lines.iter().map(|v| v["level"].as_str().unwrap()).collect();
        assert!(levels.contains(&"ERROR"));
        assert!(levels.contains(&"WARN"));
        assert!(levels.contains(&"INFO"));
        assert!(levels.contains(&"DEBUG"));
        let _ = Level::TRACE;
    }

    #[test]
    fn thread_id_off_by_default_on_when_enabled() {
        let lines = capture_json(SystemdLayer::json(), || info!("a"));
        assert!(lines[0].get("thread_id").is_none());

        let lines = capture_json(SystemdLayer::json().with_thread_ids(true), || info!("a"));
        assert!(lines[0].get("thread_id").is_some());
    }

    #[test]
    fn target_can_be_disabled() {
        let lines = capture_json(SystemdLayer::json().with_target(false), || info!("a"));
        assert!(lines[0].get("target").is_none());
    }

    #[test]
    fn level_prefix_still_works_in_json_mode() {
        // Validate that with_level_prefix(true) honors the request even
        // though json() defaults it off. The line is no longer pure JSON,
        // but the priority byte is preserved for unit-stdout-style use.
        let buf = Buf::default();
        let captured = buf.0.clone();
        let layer = SystemdLayer::json()
            .with_level_prefix(true)
            .with_output(Output::writer(buf));
        tracing::subscriber::with_default(
            tracing_subscriber::registry().with(layer),
            || warn!("p"),
        );
        let s = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
        assert!(s.starts_with("<4>"), "got {s:?}");
        // Strip the prefix and parse the remainder as JSON.
        let json_part = s.trim_end().trim_start_matches("<4>");
        let v: Value = serde_json::from_str(json_part).unwrap();
        assert_eq!(v["level"], "WARN");
    }

    #[test]
    fn field_types_preserved_round_trip() {
        let lines = capture_json(SystemdLayer::json(), || {
            let span = info_span!(
                "s",
                b = true,
                i = -3i64,
                u = 42u64,
                f = 1.5f64,
                s = "hi",
            );
            let _g = span.enter();
            info!(extra = 9u64, "msg");
        });
        let v = &lines[0];
        let span_fields = &v["span_chain"][0]["fields"];
        assert_eq!(span_fields["b"], true);
        assert_eq!(span_fields["i"], -3);
        assert_eq!(span_fields["u"], 42);
        assert_eq!(span_fields["f"], 1.5);
        assert_eq!(span_fields["s"], "hi");
        assert_eq!(v["fields"]["extra"], 9);
    }

    #[test]
    fn non_finite_f64_in_event_field_becomes_null() {
        let lines = capture_json(SystemdLayer::json(), || {
            info!(bad = f64::NAN, "msg");
        });
        assert!(lines[0]["fields"]["bad"].is_null());
    }

}
