//! Pretty stdout example. Mirrors the demo from `tracing-systemd` 0.1's
//! `test_log_stdout` but uses the new builder API.
//!
//! Run with: `cargo run --example stdout`
//! Or with `NO_COLOR=1 cargo run --example stdout` to verify auto color
//! detection.

use tracing::{debug, error, info, instrument, trace, warn};
use tracing_subscriber::prelude::*;
use tracing_systemd::SystemdLayer;

fn main() {
    tracing_subscriber::registry()
        .with(
            SystemdLayer::stdout()
                .with_target(true)
                .with_thread_ids(true)
                .with_level_prefix(false),
        )
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
