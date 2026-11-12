//! Custom format templates.
//!
//! Run with: `cargo run --example template`

use tracing::{error, info, info_span, warn};
use tracing_subscriber::prelude::*;
use tracing_systemd::SystemdLayer;

fn main() {
    tracing_subscriber::registry()
        .with(
            SystemdLayer::stdout()
                .with_target(false)
                .template("{level:>5} {leaf:<12} {message} {fields}")
                .expect("template"),
        )
        .init();

    for path in ["/api/users", "/api/orders", "/health"] {
        handle(path);
    }
}

fn handle(path: &'static str) {
    let span = info_span!("request", id = path);
    let _g = span.enter();
    info!(took_ms = 4u64, "served");
    if path == "/health" {
        warn!("slow");
    }
    if path == "/api/orders" {
        error!(code = 500u64, "upstream failed");
    }
}
