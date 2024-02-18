use tracing::error;

use tracing::{debug, info, instrument, trace, warn};
use tracing_subscriber::prelude::*;
use tracing_systemd::SystemdLayer;
fn main() {
    tracing_subscriber::registry()
        .with(SystemdLayer::new())
        .init();

    root_log_fn();
}

#[instrument]
fn root_log_fn() {
    info!("Root log");
    inner_log_1(true);
}

#[instrument(fields(instrument_field = true))]
fn inner_log_1(parameter_field: bool) {
    trace!("this is a trace");
    debug!(field_in_function = "also works");
    info!("this is an info log");
    warn!("Inner log 1");
    error!("this is an error");
}
