use tracing::error;

use tracing::{debug, info, instrument, trace, warn};
use tracing_subscriber::prelude::*;
use tracing_systemd::SystemdLayer;
fn main() {
    tracing_subscriber::registry()
        .with(
            SystemdLayer::new()
                .with_target(true)
                .use_level_prefix(false)
                .use_color(true),
        )
        .init();

    root_log_fn(true);
}

#[instrument(fields(outside_instrument_field = true))]
fn root_log_fn(outside_instrument_field: bool) {
    info!("Root log");
    inner_log_1(true);
}

#[instrument(fields(inside_instrument_field = true))]
fn inner_log_1(inside_parameter_field: bool) {
    trace!("this is a trace");
    debug!(field_in_function = "also works");
    info!("this is an info log");
    warn!("Inner log 1");
    error!("this is an error");
}
