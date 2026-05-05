# tracing-systemd

[![crates.io](https://img.shields.io/crates/v/tracing-systemd.svg)](https://crates.io/crates/tracing-systemd)
[![docs.rs](https://img.shields.io/docsrs/tracing-systemd)](https://docs.rs/tracing-systemd)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue)](#msrv)

A [`tracing-subscriber`](https://docs.rs/tracing-subscriber) layer that prints span chains
to stdout in a format that's easy to read locally and easy to ingest into the systemd
journal when run under a unit.

```text
INFO [1] my_app::request(method: "GET", path: "/api")::handler(): served in 4ms
WARN [1] my_app::request(method: "POST", path: "/api")::handler(): retrying: {attempt: 2}
```

## Usage

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

See `examples/` for more.

## Features

| Feature    | Default | Effect |
|------------|---------|--------|
| `colors`   | yes     | ANSI color output via [`nu-ansi-term`](https://docs.rs/nu-ansi-term). |
| `journald` | no      | Re-exports [`tracing-journald`](https://docs.rs/tracing-journald) under `tracing_systemd::journald`. |
| `json`     | no      | Adds `SystemdLayer::json()` for one JSON object per event (uses [`serde_json`](https://docs.rs/serde_json)). |

To turn off color output:

```toml
tracing-systemd = { version = "0.2", default-features = false }
```

## Logging to the journal

There are two ways to get logs into journald, depending on how the binary is started.

If your binary runs under a systemd unit, its stdout/stderr is already piped to the
journal. The default `SystemdLayer::stdout()` emits the `<3>`–`<7>` syslog priority
prefix that `journalctl` uses to assign levels, so you don't need anything else:

```rust
# use tracing_subscriber::prelude::*;
# use tracing_systemd::SystemdLayer;
tracing_subscriber::registry()
    .with(SystemdLayer::stdout())
    .init();
```

If you want structured fields in the journal, or you're running outside a unit, enable
the `journald` feature and add the dedicated layer alongside the stdout one:

```rust,ignore
use tracing_subscriber::prelude::*;
use tracing_systemd::SystemdLayer;

let journald = tracing_systemd::journald::layer_with_identifier("my-app").ok();

tracing_subscriber::registry()
    .with(SystemdLayer::stdout())
    .with(journald)
    .init();
```

`Option<Layer>` implements `Layer<S>`, so a `None` (when journald isn't reachable)
just becomes a no-op without an `if let`.

Filter entries with `journalctl -t my-app`.

## JSON output

Enable the `json` feature and use `SystemdLayer::json()` to emit one JSON object
per event:

```toml
tracing-systemd = { version = "0.2", features = ["json"] }
```

```rust,ignore
use tracing_subscriber::prelude::*;
use tracing_systemd::SystemdLayer;

tracing_subscriber::registry()
    .with(SystemdLayer::json().with_thread_ids(true))
    .init();
```

Sample line (formatted across lines for the README; in practice it's one line):

```json
{
  "timestamp": "2026-05-05T14:23:45.123Z",
  "level": "INFO",
  "message": "served request",
  "target": "my_app::handlers",
  "span_chain": [
    {"name": "request", "fields": {"method": "GET", "path": "/api"}},
    {"name": "handler", "fields": {}}
  ],
  "fields": {"latency_ms": 4}
}
```

Defaults differ from the pretty-mode constructors: `target` is on, timestamps
default to RFC 3339 in UTC, and the `<3>`–`<7>` syslog prefix is off so each
line is a valid standalone JSON object. Field types are preserved (`bool`,
integer, float, string); non-finite floats (`NaN`, `±Infinity`) become `null`,
matching `tracing-subscriber`'s JSON formatter and `JSON.stringify`.

Pretty-only builders (`with_color_*`, separators, brackets, thread-id
prefix/suffix) compile but have no effect in JSON mode.

## Customization

All separators, brackets, and prefixes are overridable on the builder. They take
anything that's `Into<Cow<'static, str>>`, so both `&'static str` and `String` work.

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

For tests, redirect output to any `io::Write`:

```rust,ignore
use std::sync::{Arc, Mutex};
use tracing_systemd::{Output, SystemdLayer};

let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
let layer = SystemdLayer::stdout()
    .with_output(Output::writer(MyShared(buf.clone())));
```

## Migrating from 0.1

| 0.1                                | 0.2                                                   |
|------------------------------------|-------------------------------------------------------|
| `SystemdLayer::new()`              | `SystemdLayer::stdout()`                              |
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
| `use_color(true/false)`            | `with_color_mode(ColorMode::Always / Never)`          |
| `use_sd_journal(true)`             | Add `tracing_systemd::journald::layer()?` as a separate layer (needs `journald` feature). |

`with_target` and `with_thread_ids` are unchanged. `with_timestamps`,
`with_timestamp_format`, `with_color_theme`, and `with_output` are new.

Other things that changed:

- The default for color is now `ColorMode::Auto` (respects `NO_COLOR` and TTY status).
  Pass `ColorMode::Always` for the old behavior.
- The `colored` feature was renamed to `colors`.
- The `sd-journal` feature is gone; use `journald`, which goes through
  `tracing-journald` (pure Rust, no `libsystemd-dev`).
- The runtime `use_sd_journal(false)` toggle is gone. Pick the layer at construction
  time instead.

## MSRV

Rust 1.85 (edition 2024). MSRV bumps are minor version bumps.

## License

[MIT](LICENSE)
