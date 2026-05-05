# tracing-systemd

[![crates.io](https://img.shields.io/crates/v/tracing-systemd.svg)](https://crates.io/crates/tracing-systemd)
[![docs.rs](https://img.shields.io/docsrs/tracing-systemd)](https://docs.rs/tracing-systemd)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue)](#msrv)

A [`tracing-subscriber`](https://docs.rs/tracing-subscriber) `Layer` that pretty-prints
span chains to stdout and (optionally) the systemd journal.

```text
INFO [1] my_app::request(method: "GET", path: "/api")::handler(): served in 4ms
WARN [1] my_app::request(method: "POST", path: "/api")::handler(): retrying: {attempt: 2}
```

## What's new in 0.2

- **No more `libsystemd-dev` build dependency.** Journald support now goes through the
  official [`tracing-journald`](https://docs.rs/tracing-journald) layer (pure Rust,
  speaks the native journal socket protocol).
- **Robustness**: every `.unwrap()` removed; the layer never panics on degenerate
  span/field states.
- **More customizable**: `Cow<'static, str>` separators (so you can pass owned `String`s),
  pluggable [`ColorTheme`], optional timestamps, custom [`Output`] writers (great for tests).
- **Better defaults**: `ColorMode::Auto` respects `NO_COLOR` and TTY status out of the box.
- **Documented**: `#![deny(missing_docs)]`, working doctests, docs.rs metadata, full migration table below.
- **Modern toolchain**: edition 2024, MSRV 1.85, `#![forbid(unsafe_code)]`, clippy::pedantic clean.

## Quick start

```toml
[dependencies]
tracing            = "0.1"
tracing-subscriber = "0.3"
tracing-systemd    = "0.2"
```

```rust
use tracing::info;
use tracing_subscriber::prelude::*;
use tracing_systemd::SystemdLayer;

fn main() {
    tracing_subscriber::registry()
        .with(SystemdLayer::stdout().with_target(true).with_thread_ids(true))
        .init();

    info!("hello, world");
}
```

## Features

| Feature    | Default | Effect                                                                                        |
|------------|---------|-----------------------------------------------------------------------------------------------|
| `colors`   | yes     | ANSI color output via [`nu-ansi-term`](https://docs.rs/nu-ansi-term).                         |
| `journald` | no      | Re-exports [`tracing-journald`](https://docs.rs/tracing-journald) under `tracing_systemd::journald`. Pure Rust; no `libsystemd-dev` needed. |
| `json`     | no      | Pulls in [`serde_json`](https://docs.rs/serde_json) (reserved for a future JSON output mode). |

Disable color output with `default-features = false`:

```toml
tracing-systemd = { version = "0.2", default-features = false }
```

## Logging to the journal

Two patterns, depending on how your binary is invoked.

**Under a systemd unit** (most common): the unit's stdout/stderr is already routed to the
journal. Just use `SystemdLayer::stdout()` with the level prefix on (it's the default), and
`journalctl` will pick up the syslog priority from the `<3>`–`<7>` markers.

```rust
# use tracing_subscriber::prelude::*;
# use tracing_systemd::SystemdLayer;
tracing_subscriber::registry()
    .with(SystemdLayer::stdout())   // level prefix `<5>` etc. is emitted by default
    .init();
```

**Outside a unit**, e.g. running locally for development: enable the `journald` feature and
attach the dedicated layer.

```rust,ignore
use tracing_subscriber::prelude::*;
use tracing_systemd::SystemdLayer;

let journald = tracing_systemd::journald::layer_with_identifier("my-app").ok();

tracing_subscriber::registry()
    .with(SystemdLayer::stdout())   // pretty for humans
    .with(journald)                 // structured fields into the journal
    .init();
```

`Option<Layer>` implements `Layer<S>`, so passing `None` (when journald isn't reachable)
cleanly disables that arm without an `if let`.

Filter your entries with `journalctl -t my-app`.

## Customization

Every separator and bracket is overridable via the builder; they accept anything that
implements `Into<Cow<'static, str>>`, so `&'static str` *and* `String` both work.

```rust,ignore
use tracing_systemd::{SystemdLayer, ColorMode, ColorTheme, TimestampFormat};
use nu_ansi_term::{Color, Style};

let layer = SystemdLayer::stdout()
    .with_target(true)
    .with_thread_ids(true)
    .with_timestamp_format(TimestampFormat::UnixSeconds)
    .with_function_bracket_left("[")
    .with_function_bracket_right("]")
    .with_arguments_equality("=")
    .with_color_mode(ColorMode::Auto)
    .with_color_theme(ColorTheme {
        info: Style::new().fg(Color::Cyan).bold(),
        ..ColorTheme::default()
    });
```

For tests, write to any `io::Write`:

```rust,ignore
use std::sync::{Arc, Mutex};
use tracing_systemd::{Output, SystemdLayer};

let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
let layer = SystemdLayer::stdout()
    .with_output(Output::writer(MyShared(buf.clone())));
```

## Migration from 0.1

The crate has 2,300+ downloads on 0.1 — here's the explicit method-name mapping.

| 0.1 method                         | 0.2 equivalent                                        |
|------------------------------------|-------------------------------------------------------|
| `SystemdLayer::new()`              | `SystemdLayer::stdout()`                              |
| `with_target(b)`                   | `with_target(b)` *(unchanged)*                        |
| `with_thread_ids(b)`               | `with_thread_ids(b)` *(unchanged)*                    |
| `separate_spans_with(s)`           | `with_span_separator(s)`                              |
| `separate_message_with(s)`         | `with_message_separator(s)`                           |
| `level_separator(s)`               | `with_level_separator(s)`                             |
| `function_bracket_left(s)`         | `with_function_bracket_left(s)`                       |
| `function_bracket_right(s)`        | `with_function_bracket_right(s)`                      |
| `arguments_equality(s)`            | `with_arguments_equality(s)`                          |
| `arguments_separator(s)`           | `with_arguments_separator(s)`                         |
| `thread_id_prefix(s)`              | `with_thread_id_prefix(s)`                            |
| `thread_id_suffix(s)`              | `with_thread_id_suffix(s)`                            |
| `use_level_prefix(b)`              | `with_level_prefix(b)`                                |
| `use_color(true)` / `use_color(false)` | `with_color_mode(ColorMode::Always / Never)`     |
| `use_sd_journal(true)`             | Add `tracing_systemd::journald::layer()?` as a separate layer (requires `journald` feature). |
| *(N/A)*                            | `with_timestamps`, `with_timestamp_format`, `with_color_theme`, `with_output` *(new)* |

**Other behavior changes:**

- The default for color output is now `ColorMode::Auto` (was: always on under the
  `colored` feature). Pass `with_color_mode(ColorMode::Always)` for the old behavior.
- `colored` feature renamed to `colors`.
- `sd-journal` feature **removed**; use `journald` (which pulls in `tracing-journald` instead).
- The old runtime `use_sd_journal(false)` toggle is gone — pick the right layer at
  construction time.

## MSRV

Tracing-systemd 0.2 requires Rust **1.85** (edition 2024). Bumping the MSRV is a
**minor** version bump going forward.

## License

[MIT](LICENSE)
