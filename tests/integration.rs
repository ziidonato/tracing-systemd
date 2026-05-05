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
