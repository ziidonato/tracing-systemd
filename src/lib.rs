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
//! To control the line shape exactly, pass a template instead of tuning
//! separators:
//!
//! ```no_run
//! use tracing_subscriber::prelude::*;
//! use tracing_systemd::SystemdLayer;
//!
//! // INFO app::net: served took_ms=4
//! tracing_subscriber::registry()
//!     .with(
//!         SystemdLayer::stdout()
//!             .template("{level} {target}: {message} took_ms={took_ms}")
//!             .expect("template"),
//!     )
//!     .init();
//! ```
//!
//! # Features
//!
//! | Feature    | Default | Effect                                                                  |
//! |------------|---------|-------------------------------------------------------------------------|
//! | `colors`   | yes     | Enables ANSI color output via [`nu-ansi-term`](https://docs.rs/nu-ansi-term). |
//! | `journald` | no      | Adds [`Output::journald`], which writes the *same* formatted line to the journal socket. Pure Rust; no `libsystemd-dev`. |
//! | `json`     | no      | Adds [`SystemdLayer::json`] for one JSON object per event (uses [`serde_json`](https://docs.rs/serde_json)). |
//!
//! # Migrating from 0.1
//!
//! See [the README](https://github.com/ziidonato/tracing-systemd/blob/main/README.md#migrating-from-01)
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
pub(crate) mod journald;

pub use format::TimestampFormat;
pub use layer::SystemdLayer;
pub use output::Output;

#[cfg(feature = "colors")]
#[cfg_attr(docsrs, doc(cfg(feature = "colors")))]
pub use format::{ColorMode, ColorTheme};

pub use format::Error as TemplateError;
