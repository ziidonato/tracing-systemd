//! Combined stdout + journald example: the same formatted lines go to the
//! terminal and to the journal, so `journalctl` shows the span chain too.
//!
//! Run with: `cargo run --example combined --features journald`

use tracing::{error, info, instrument, warn};
use tracing_subscriber::prelude::*;
use tracing_systemd::SystemdLayer;

fn main() {
    // Optional: `None` when not running under systemd.
    let journald_output = tracing_systemd::Output::journald()
        .ok()
        .map(|o| o.with_syslog_identifier("combined-example"));

    let journald_layer = journald_output.map(|o| {
        SystemdLayer::stdout()
            .with_output(o)
            .with_color_mode(tracing_systemd::ColorMode::Never)
    });

    tracing_subscriber::registry()
        .with(SystemdLayer::stdout().with_target(true))
        .with(journald_layer)
        .init();

    work(42);
}

#[instrument]
fn work(seed: u64) {
    info!("starting work");
    if seed % 2 == 0 {
        warn!(seed, "even seed, heuristic only");
    } else {
        error!(seed, "odd seed, would have failed");
    }
}
