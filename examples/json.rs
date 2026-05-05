//! JSON output example. One JSON object per line.
//!
//! Run with: `cargo run --example json --features json`
//!
//! Pipe through `jq` to inspect:
//! `cargo run --example json --features json 2>&1 | jq`

use tracing::{error, info, instrument, warn};
use tracing_subscriber::prelude::*;
use tracing_systemd::SystemdLayer;

fn main() {
    tracing_subscriber::registry()
        .with(SystemdLayer::json().with_thread_ids(true))
        .init();

    handle_request("GET", "/api/users");
    handle_request("POST", "/api/orders");
}

#[instrument]
fn handle_request(method: &str, path: &str) {
    info!(method, path, "received");
    work(42);
    if path.contains("orders") {
        warn!(retries = 2u64, "transient db error, retrying");
    }
    error!(error = "io: connection reset", "request failed");
}

#[instrument]
fn work(items: u64) {
    info!(processed = items, "done");
}
