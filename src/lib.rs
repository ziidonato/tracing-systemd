//! A [`tracing_subscriber::Layer`] that pretty-prints span chains to stdout
//! and (optionally) the systemd journal.
//!
//! # Quick start
//!
//! ```no_run
//! use tracing::info;
//! use tracing_subscriber::prelude::*;
//! use tracing_systemd::SystemdLayer;
//!
//! tracing_subscriber::registry()
//!     .with(SystemdLayer::stdout().with_target(true).with_thread_ids(true))
//!     .init();
//!
//! info!("hello");
//! ```
//!
//! # Features
//!
//! | Feature    | Default | Effect                                                                  |
//! |------------|---------|-------------------------------------------------------------------------|
//! | `colors`   | yes     | Enables ANSI color output via [`nu-ansi-term`](https://docs.rs/nu-ansi-term). |
//! | `journald` | no      | Re-exports the official [`tracing-journald`](https://docs.rs/tracing-journald) layer through [`mod@journald`]. Pure-Rust; no `libsystemd-dev` build dependency. |
//! | `json`     | no      | Pulls in [`serde_json`](https://docs.rs/serde_json) (reserved for a future JSON output mode). |
//!
//! # Migrating from 0.1
//!
//! See [the README](https://github.com/ziidonato/tracing-systemd/blob/main/README.md#migration-from-01)
//! for a method-by-method mapping.

#![deny(missing_docs)]
#![forbid(unsafe_code)]
#![warn(
    clippy::pedantic,
    clippy::all,
    rust_2018_idioms,
    missing_debug_implementations
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod format;
mod layer;
mod output;
mod visit;

#[cfg(all(unix, feature = "journald"))]
#[cfg_attr(docsrs, doc(cfg(all(unix, feature = "journald"))))]
pub mod journald;

pub use format::TimestampFormat;
pub use layer::SystemdLayer;
pub use output::Output;

#[cfg(feature = "colors")]
#[cfg_attr(docsrs, doc(cfg(feature = "colors")))]
pub use format::{ColorMode, ColorTheme};
