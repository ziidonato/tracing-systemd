//! The same template, sent to the journal.
//!
//! Run with: `cargo run --example template_journald --features journald`
//! Then: `journalctl -t template-demo -n 5 -o json-pretty`

use tracing::{info, info_span, warn};
use tracing_subscriber::prelude::*;
use tracing_systemd::{Output, SystemdLayer};

fn main() {
    let output = Output::journald()
        .expect("journal socket")
        .with_syslog_identifier("template-demo");

    tracing_subscriber::registry()
        .with(
            SystemdLayer::stdout()
                .with_output(output)
                .template("{level_short} {leaf}: {message} {fields}")
                .expect("template"),
        )
        .init();

    let span = info_span!("handle", id = 7u64);
    let _g = span.enter();
    info!(rows = 3u64, "done");
    warn!("careful");
}
