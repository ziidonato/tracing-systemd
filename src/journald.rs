//! Optional journald layer: a thin convenience wrapper around the official
//! [`tracing-journald`](https://docs.rs/tracing-journald) crate.
//!
//! `tracing-journald` speaks the systemd journal native socket protocol in
//! pure Rust, so enabling this feature does **not** add a build-time
//! dependency on `libsystemd-dev`.

#![cfg(all(unix, feature = "journald"))]

use std::io;

/// Build the official [`tracing_journald::Layer`] with default settings.
///
/// Returns `Err` if the journal socket cannot be opened (typically
/// `/run/systemd/journal/socket`), e.g. when running outside of systemd.
///
/// # Composition
///
/// ```no_run
/// use tracing_subscriber::prelude::*;
/// use tracing_systemd::SystemdLayer;
///
/// let journald = tracing_systemd::journald::layer().ok();
/// tracing_subscriber::registry()
///     .with(SystemdLayer::stdout())
///     .with(journald)
///     .init();
/// ```
///
/// `Option<Layer>` implements `Layer<S>`, so passing `None` cleanly disables
/// the journald layer when journald isn't available.
///
/// # Errors
///
/// Returns the underlying [`io::Error`] from [`tracing_journald::layer`]
/// when the journal socket can't be opened.
pub fn layer() -> io::Result<tracing_journald::Layer> {
    tracing_journald::layer()
}

/// Build a [`tracing_journald::Layer`] with a specific
/// `SYSLOG_IDENTIFIER` (filterable via `journalctl -t <identifier>`).
///
/// # Errors
///
/// Returns the underlying [`io::Error`] from [`tracing_journald::layer`]
/// when the journal socket can't be opened.
pub fn layer_with_identifier(identifier: impl AsRef<str>) -> io::Result<tracing_journald::Layer> {
    tracing_journald::layer().map(|l| l.with_syslog_identifier(identifier.as_ref().to_owned()))
}
