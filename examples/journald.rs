//! Direct-to-journald example via the optional `journald` feature.
//!
//! Run with: `cargo run --example journald --features journald`
//! Then watch the journal: `journalctl -t journald-example -f`
//!
//! Replaces 0.1's `test_log_systemd`. Under a real systemd unit you usually
//! don't need this (the unit's stdout is already routed to the journal), but
//! this shows direct emission for processes running outside a unit.
//!
//! The journal receives the *same rendered line* stdout would, plus native
//! `PRIORITY`/`CODE_LINE`/`SPAN_NAME` fields, so `journalctl` shows a readable
//! log rather than a bare message.

use tracing::{debug, error, info, instrument, trace, warn};
use tracing_subscriber::prelude::*;

fn main() {
    let output = tracing_systemd::Output::journald()
        .expect("failed to connect to /run/systemd/journal/socket; are you on systemd?")
        .with_syslog_identifier("journald-example");

    tracing_subscriber::registry()
        .with(tracing_systemd::SystemdLayer::stdout().with_output(output))
        .init();

    root_log_fn(true);
}

#[instrument(fields(outside_instrument_field = true))]
fn root_log_fn(_param: bool) {
    info!("Root log");
    inner_log_1(true);
}

#[instrument(fields(inside_instrument_field = true))]
fn inner_log_1(_inside_parameter_field: bool) {
    trace!("this is a trace");
    debug!(field_in_function = "also works");
    info!("this is an info log");
    warn!("Inner log 1");
    error!("this is an error");
}
