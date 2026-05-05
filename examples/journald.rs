//! Direct-to-journald example via the optional `journald` feature.
//!
//! Run with: `cargo run --example journald --features journald`
//! Then watch the journal: `journalctl -t journald-example -f`
//!
//! Replaces 0.1's `test_log_systemd`. Note that under a real systemd unit
//! you usually don't need this — stdout from the unit is already routed
//! to the journal — but this example shows direct emission for processes
//! running outside a unit.

use tracing::{debug, error, info, instrument, trace, warn};
use tracing_subscriber::prelude::*;

fn main() {
    let journald = tracing_systemd::journald::layer_with_identifier("journald-example")
        .expect("failed to connect to /run/systemd/journal/socket — are you on systemd?");

    tracing_subscriber::registry().with(journald).init();

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
