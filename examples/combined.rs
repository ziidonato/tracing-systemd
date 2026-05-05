//! Combined stdout + journald example. The pretty span-chain output goes
//! to stdout for human reading; the journal receives structured events
//! via `tracing-journald` so they are queryable with `journalctl`.
//!
//! Run with: `cargo run --example combined --features journald`

use tracing::{error, info, instrument, warn};
use tracing_subscriber::prelude::*;
use tracing_systemd::SystemdLayer;

fn main() {
    // Stdout layer: pretty for humans.
    let stdout_layer = SystemdLayer::stdout()
        .with_target(true)
        .with_level_prefix(false);

    // Journald layer: optional. None if we're not running under systemd.
    let journald_layer = tracing_systemd::journald::layer_with_identifier("combined-example").ok();

    tracing_subscriber::registry()
        .with(stdout_layer)
        .with(journald_layer)
        .init();

    work(42);
}

#[instrument]
fn work(seed: u64) {
    info!("starting work");
    if seed % 2 == 0 {
        warn!(seed, "even seed — heuristic only");
    } else {
        error!(seed, "odd seed — would have failed");
    }
}
