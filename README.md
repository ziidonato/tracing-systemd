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

The default line carries no syslog priority prefix. Add one with
`SystemdLayer::unit_stdout()` (or `with_level_prefix(true)`) when the process
runs under a systemd unit and you want `PRIORITY` inferred from stdout.

## Usage

```toml
[dependencies]
tracing            = "0.1"
tracing-subscriber = "0.3"
tracing-systemd    = "0.4"
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
| *(none)*   | n/a     | Format templates (`SystemdLayer::template`) need no feature. |
| `journald` | no      | Adds `Output::journald()`, a sink that writes the *same* formatted line to the journal socket. Pure Rust, no `libsystemd-dev`. |
| `json`     | no      | Adds `SystemdLayer::json()` for one JSON object per event (uses [`serde_json`](https://docs.rs/serde_json)). |

To turn off color output:

```toml
tracing-systemd = { version = "0.4", default-features = false }
```

## Logging to the journal

There are two ways to get logs into journald, and they differ in what the journal
ends up storing.

**Under a systemd unit**, stdout is already routed to the journal.
`SystemdLayer::unit_stdout()` emits the `<3>`–`<7>` syslog priority prefix that
`journalctl` uses to assign levels, and disables colour:

```rust
# use tracing_subscriber::prelude::*;
# use tracing_systemd::SystemdLayer;
tracing_subscriber::registry()
    .with(SystemdLayer::unit_stdout())
    .init();
```

**Outside a unit**, or when you want native journald fields, enable the `journald`
feature and send the output to the journal socket instead of stdout:

```rust,ignore
use tracing_subscriber::prelude::*;
use tracing_systemd::{Output, SystemdLayer};

let output = Output::journald()
    .expect("no journal socket")
    .with_syslog_identifier("my-app");

tracing_subscriber::registry()
    .with(SystemdLayer::stdout().with_output(output))
    .init();
```

**The journal receives the same rendered line stdout would**, span chain and all,
plus native `PRIORITY`, `TARGET`, `CODE_FILE`, `CODE_LINE` and one `SPAN_NAME` per
parent span. So `journalctl -t my-app` shows a readable log *and* the entries are
queryable (`journalctl PRIORITY=3`). Every formatting option applies, because the
sink sits behind the normal renderer rather than replacing it.

The syslog `<n>` prefix is suppressed on a journald sink, because `PRIORITY`
already carries the level and a `<5>` inside `MESSAGE` would be duplicated data.

To write to both places, use two layers sharing one configuration:

```rust,ignore
tracing_subscriber::registry()
    .with(SystemdLayer::stdout())
    .with(journald_output.map(|o| SystemdLayer::stdout().with_output(o)))
    .init();
```

`Option<Layer>` implements `Layer<S>`, so `None` (no socket) becomes a no-op
without an `if let`.

> **Note on `tracing-journald`:** earlier versions of this crate re-exported it.
> It cannot show formatted output: its `MESSAGE` is built from the event's
> `message` field alone, so span chains never reach the journal. That is why the
> transport is now built in (see `src/journald.rs`).

## JSON output

Enable the `json` feature and use `SystemdLayer::json()` to emit one JSON object
per event:

```toml
tracing-systemd = { version = "0.4", features = ["json"] }
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
default to RFC 3339 in UTC, and the `<3>`–`<7>` syslog prefix stays off (the
default there too) so each line is a valid standalone JSON object. Field types are preserved (`bool`,
integer, float, string); non-finite floats (`NaN`, `±Infinity`) become `null`,
matching `tracing-subscriber`'s JSON formatter and `JSON.stringify`.

Pretty-only builders (`with_color_*`, separators, brackets, thread-id
prefix/suffix) compile but have no effect in JSON mode. `with_level_prefix(true)`
does still apply, and produces a line that is no longer parsable as JSON. Useful
only when you want an explicit `PRIORITY` alongside a JSON payload.

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
    // Span arguments: `worker[id=1]`
    .with_function_bracket_left("[")
    .with_function_bracket_right("]")
    .with_arguments_equality("=")
    // Event field tail: `... [count=3]`
    .with_field_bracket_left("[")
    .with_field_bracket_right("]")
    .with_color_mode(ColorMode::Auto)
    .with_color_theme(ColorTheme {
        info: Style::new().fg(Color::Cyan).bold(),
        ..ColorTheme::default()
    });
```

Timestamps are controlled by a single knob: `with_timestamp_format(fmt)` sets the
format and `TimestampFormat::None` turns them off. `with_timestamps(bool)` is a
shortcut for RFC 3339 / off. `None` means the default pretty line carries no
timestamp; a template that names `{timestamp}` always renders one (RFC 3339 when
the config says `None`), because naming the placeholder states the intent.

String-like field values (`&str`, `Debug`, errors) are quoted and escaped in both
span arguments and the event field tail, so a value containing `, ` or `}`
cannot be mistaken for a separator.

Span fields recorded after creation (`span.record(..)`, late `#[instrument]`
parameters, `tracing::field::Empty` placeholders) are picked up via the layer's
`on_record` hook and appear in subsequent lines.

For tests, redirect output to any `io::Write`:

```rust,ignore
use std::sync::{Arc, Mutex};
use tracing_systemd::{Output, SystemdLayer};

let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
let layer = SystemdLayer::stdout()
    .with_output(Output::writer(MyShared(buf.clone())));
```

### Format templates

When separators aren't enough, and you want a genuinely different line shape,
pass a template. It's compiled at construction, so a typo is a startup error
instead of a silently broken log line.

```rust,ignore
let layer = SystemdLayer::stdout()
    .template("{level:>5} {leaf:<12} {message} {fields}")
    .expect("template");
```

```text
 INFO request      served {took_ms: 4}
ERROR request      upstream failed {code: 500}
```

Placeholders fall into two groups:

| Placeholder | Value |
| --- | --- |
| `{level}` / `{level_short}` | `INFO` / `INF` (also `DBG`, `TRC`, `WRN`, `ERR`) |
| `{target}` | module path |
| `{message}` | the event's message field |
| `{thread_id}` | OS thread id |
| `{timestamp}` | formatted per `with_timestamp_format`; RFC 3339 when unset or `None` |
| `{file}` / `{line}` | call site |
| `{spans}` | the parent chain, `a(x: 1)::b()::` |
| `{leaf}` | the innermost span, name plus arguments (`handler()`) |
| `{span}` | parent chain plus the leaf, rendered in full |
| `{fields}` | every event field except `message`, wrapped in `with_field_bracket_*` |

**Anything else is an event field.** `{rows}` renders the `rows` field from
`info!(rows = 3, "done")`. A field that the event doesn't carry renders as empty
rather than erroring, so one template can serve events with different fields.

Width and alignment follow Rust's own syntax and are measured on the *visible*
text, so ANSI colour codes don't inflate the column:

```rust,ignore
"{level:>5}"   // right-aligned in 5 columns
"{level:<5}"   // left-aligned
"{leaf:^12}"   // centred
```

Literal braces are `{{` and `}}`.

An empty placeholder removes itself, not the text around it: `"{message} {fields}"`
on an event with no extra fields prints `served ` with a trailing space. Put the
optional part last, or accept the space.

Templates are bounded at compile time. A width above 1024, or more than 1024
placeholders, is rejected by `template()`. Rendering a placeholder appends into
the line buffer and `{fields}` borrows the event's fields, so allocation is
bounded by the template, not by what the event carries — not unbounded, and not
zero either.

Templates replace the entire pretty line, so `with_target`, `with_thread_ids`,
and the separator knobs no longer apply. Colours still do, per element,
including `{level_short}`.

`{fields}` is the exception: it reads `with_arguments_separator`,
`with_arguments_equality` and `with_field_bracket_*`, so a template's field tail
matches the default renderer's.

A template also applies to the journald sink, which is the way to give the
journal a terser `MESSAGE` than the terminal while keeping both readable:

```rust,ignore
let terse = SystemdLayer::stdout()
    .template("{message} {fields}")
    .expect("template");
```

## Migrating from 0.3 to 0.4

`0.4` changes the default line format and **removes** `tracing_systemd::journald`
along with the `tracing_journald` re-export. Code that called
`journald::layer()` or `journald::layer_with_identifier("my-app")` will not
compile until it moves to `Output::journald()`.

The rest of this section is about behaviour that changes silently.

| Change | Before | After |
|--------|--------|-------|
| Syslog prefix | `stdout()` emitted `<5>INFO ...` by default (`use_level_prefix: true`) | default is `false`; `unit_stdout()` is the prefixed constructor |
| String field values | span args printed unquoted (`worker(id: 1)`), event fields quoted (`{name: "a"}`) | both quoted and escaped (`worker(id: 1, name: "a")`) |
| `with_timestamps(bool)` | set a flag no renderer read, so `true` alone printed nothing | `true` selects RFC 3339, `false` selects `TimestampFormat::None` |
| `show_timestamp` | separate knob alongside `timestamp_format` | gone; `TimestampFormat::None` is the only off-switch |
| Event field tail | hardcoded `", "` / `": "` / `{}`, ignoring the builder | honours `with_arguments_*=`, and has its own `with_field_bracket_*` |
| `span.record(..)`, late `#[instrument]` params, `field::Empty` | silently dropped (no `on_record`) | captured and printed |
| `ColorTheme::{span_name, field_name, field_value, target}` | accepted, never read | applied |
| `tracing_systemd::journald::{layer, layer_with_identifier}` and the `tracing_journald` re-export | formatted only `MESSAGE`; span chains never reached the journal | removed; the transport is built in as `Output::journald()` and carries the full rendered line |

If you relied on the prefixed default, switch `SystemdLayer::stdout()` to
`SystemdLayer::unit_stdout()`. That one does not fail at compile time, and it
matters: under a systemd unit the `<n>` prefix is how `journalctl -p err` finds
your errors, so an upgrading service quietly loses level filtering. Check any
`stdout()` constructor in code that runs under a unit.

If you relied on unquoted span arguments, set
`with_arguments_equality`/`with_arguments_separator`. The quoting itself is not
configurable, because it is what keeps `f(s: "a, b")` unambiguous.

## Migrating from 0.1

| 0.1                                | 0.2+                                                  |
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
| `use_sd_journal(true)`             | `SystemdLayer::stdout().with_output(Output::journald()?)` (needs `journald` feature). |

`with_target` and `with_thread_ids` are unchanged. `with_timestamps`,
`with_timestamp_format`, `with_color_theme`, and `with_output` are new.

Other things that changed:

- The default for color is now `ColorMode::Auto` (respects `NO_COLOR` and TTY status).
  Pass `ColorMode::Always` for the old behavior.
- The `colored` feature was renamed to `colors`.
- The `sd-journal` feature is gone; use `journald`, which writes the journal's
  native protocol directly (pure Rust, no `libsystemd-dev`).
- The runtime `use_sd_journal(false)` toggle is gone. Pick the layer at construction
  time instead.

## MSRV

Rust 1.85 (edition 2024). MSRV bumps are minor version bumps.

## License

[MIT](LICENSE)
