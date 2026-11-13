# tracing-systemd

[![crates.io](https://img.shields.io/crates/v/tracing-systemd.svg)](https://crates.io/crates/tracing-systemd)
[![docs.rs](https://img.shields.io/docsrs/tracing-systemd)](https://docs.rs/tracing-systemd)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue)](#msrv)

A [`tracing-subscriber`](https://docs.rs/tracing-subscriber) layer that prints span
chains to stdout in a format that reads well locally and ingests cleanly into the
systemd journal when run under a unit.

```text
INFO [1] my_app::request(method: "GET", path: "/api")::handler(): served in 4ms
WARN [1] my_app::request(method: "POST", path: "/api")::handler(): retrying: {attempt: 2}
```

| Feature    | Default | Effect |
|------------|---------|--------|
| `colors`   | yes     | ANSI colour via [`nu-ansi-term`](https://docs.rs/nu-ansi-term) |
| `journald` | no      | `Output::journald()` — writes the *same* formatted line to the journal socket. Pure Rust, no `libsystemd-dev` |
| `json`     | no      | `SystemdLayer::json()` — one JSON object per event |

Templates, quoting, separators and theming need no feature flag.

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

The default line carries no syslog priority prefix. Add one with
`SystemdLayer::unit_stdout()` (or `with_level_prefix(true)`) when the process runs
under a systemd unit and you want `PRIORITY` inferred from stdout.

---

# The line

Everything else in this README is a variation on this section: a line is built from
**pieces**, and you can change the *layout* of the pieces, the *appearance* of each
piece, or both.

```text
INFO [1] my_app::request(method: "GET")::handler(): served in 4ms: {latency_ms: 4}
└┬─┘ └┬┘ └──────────────┬──────────────┘└───┬───┘ └────┬────┘  └──────┬──────┘
 │    │                 │                    │          │              │
 │    │              span chain          leaf span   message      field tail
 │    │            (+ arguments)         (+ args)
 │  thread id
level
```

Two rules cover almost every question:

1. **The layout** (order, literals, whether the target shows) is set either by the
   default renderer's knobs or by a template — never both at once.
2. **The appearance** of a piece (colour, quoting, separators *inside* a piece) is
   set by the builder and survives a template.

### Knobs or template?

| You want | Use |
| --- | --- |
| Add/remove the target, thread id, timestamp; change a separator | the `with_*` knobs |
| Reorder fields, drop the span chain, add literal text, align columns | a **template** |
| Structured output for a log processor | **JSON** |

A template replaces the whole pretty line. If you only want to move one thing, a
knob is cheaper and keeps the default look.

### What survives what

This is the table to read before mixing a template with the builder. "Layout" means
it changes *which pieces and in what order*; "appearance" means it changes *how a
piece renders*.

| Builder call | Kind | Under a template |
| --- | --- | --- |
| `with_target` | layout | overridden — name `{target}` to include it |
| `with_thread_ids`, `with_thread_id_prefix/suffix` | layout | overridden — name `{thread_id}` |
| `with_timestamps`, `with_timestamp_format` | layout | overridden — `{timestamp}` always renders (RFC 3339 if the config says `None`) |
| `with_level_prefix` | layout | overridden — write the `<n>` prefix into the template yourself |
| `with_span_separator`, `with_message_separator`, `with_level_separator` | layout | **ignored** — the template's own literals join the pieces |
| `with_function_bracket_left/right` | appearance | **works** — brackets around span arguments |
| `with_arguments_equality`, `with_arguments_separator` | appearance | **works** — used inside span args and `{fields}` |
| `with_field_bracket_left/right` | appearance | **works** — used by `{fields}` |
| `with_quotes` | appearance | **works** — applies to every value |
| `with_color_mode`, `with_color_theme` | appearance | **works** |

So `{fields}` renders *identically* to the default tail, because it reads the same
`with_arguments_*` and `with_field_bracket_*` settings. A template changes the
skeleton, not the vocabulary inside it.

### Templates

Compiled at construction, so a typo is a startup error rather than a broken log line.

```rust,ignore
let layer = SystemdLayer::stdout()
    .template("{level:>5} {leaf:<12} {message} {fields}")
    .expect("template");
```

```text
 INFO request      served {latency_ms: 4}
ERROR request      upstream failed {code: 500}
```

| Placeholder | Value |
| --- | --- |
| `{level}` / `{level_short}` | `INFO` / `INF` (also `DBG`, `TRC`, `WRN`, `ERR`) |
| `{target}` | module path |
| `{message}` | the event's message field |
| `{thread_id}` | OS thread id, including `with_thread_id_prefix/suffix` |
| `{timestamp}` | formatted per `with_timestamp_format`; RFC 3339 when unset or `None` |
| `{file}` / `{line}` | call site |
| `{spans}` | the parent chain, `a(x: 1)::b()::` |
| `{leaf}` | the innermost span, name plus arguments (`handler()`) |
| `{span}` | parent chain plus leaf, rendered in full |
| `{fields}` | every event field except `message`, wrapped in `with_field_bracket_*` |

**Anything else is an event field.** `{rows}` renders the `rows` field from
`info!(rows = 3, "done")`. A field the event doesn't carry renders empty rather than
erroring, so one template serves events with different fields. Literal braces are
`{{` and `}}`.

Width and alignment follow Rust's own syntax and are measured on the *visible* text,
so ANSI codes don't inflate the column:

```rust,ignore
"{level:>5}"   // right-aligned in 5 columns
"{level:<5}"   // left-aligned
"{leaf:^12}"   // centred
```

An empty placeholder removes itself, not the text around it: `"{message} {fields}"`
on an event with no extra fields prints `served ` with a trailing space. Put the
optional part last, or accept the space.

Templates are bounded at compile time: a width above 1024, or more than 1024
placeholders, is rejected by `template()`. Rendering appends into the line buffer and
`{fields}` borrows the event's fields, so allocation is bounded by the template rather
than by what the event carries.

**Modifiers** change one placeholder without touching the rest of the line:

| Modifier | Effect |
| --- | --- |
| `plain`, `nocolor`, `no_color` | render this placeholder without colour |
| `raw`, `unquoted` | render this placeholder's value without quotes |

They sit before the alignment spec, separated by `:` or `.`:

```rust,ignore
"{level:plain} {level}"     // INFO INFO — first bare, second coloured
"{message:raw}"             // done — not "done"
"{message:nocolor:>8}"      // right-aligned, uncoloured
```

`plain` subtracts colour; it cannot add it. Under the default theme the only visible
effect is on `{level}` and `{level_short}`, because the level slots are the only ones
with a colour — see [Colours](#colours). `raw` applies wherever a value is quoted;
on a metadata slot (`{level:raw}`) it is a harmless no-op. `{fields}` ignores `raw`,
because unquoting a whole tail lets a value containing `, ` read as a separator — use
`with_quotes(false)` for that. An unknown modifier fails at startup.

A template also applies to the journald sink, which is how to give the journal a
terser `MESSAGE` than the terminal:

```rust,ignore
let terse = SystemdLayer::stdout()
    .template("{message} {fields}")
    .expect("template");
```

### Quoting

String-like values (`&str`, `Debug`, errors) are quoted and escaped in span arguments
and the field tail, so a value containing `, ` or `}` cannot be mistaken for a
separator:

```text
worker(id: 1, name: "a, b")     not  worker(id: 1, name: a, b)
```

Only `"` and `\` are escaped — not newlines, not tabs. It is the same rule Rust's
`{:?}` uses.

`with_quotes(false)` renders those values bare:

```rust,ignore
SystemdLayer::stdout().with_quotes(false)
```

That is the right trade when the sink mangles backslashes and the wrong one when a
value can contain `, ` or `}`, because the line stops being parseable. The `raw`
modifier is the per-placeholder version of the same choice.

### Timestamps

One knob: `with_timestamp_format(fmt)`. `TimestampFormat::None` turns them off and
`with_timestamps(bool)` is a shortcut for RFC 3339 / off. `None` means the default
line carries no timestamp; a template naming `{timestamp}` always renders one (RFC
3339), because naming the placeholder states the intent.

### Separators and brackets

Every separator, bracket and prefix is overridable and takes anything
`Into<Cow<'static, str>>`, so both `&'static str` and `String` work.

```rust,ignore
let layer = SystemdLayer::stdout()
    // Span arguments: `worker[id=1]`
    .with_function_bracket_left("[")
    .with_function_bracket_right("]")
    .with_arguments_equality("=")
    // Event field tail: `... [count=3]`
    .with_field_bracket_left("[")
    .with_field_bracket_right("]");
```

---

# Colours

`ColorMode::Auto` (the default) respects `NO_COLOR` and TTY status. To disable colour
entirely, either set `ColorMode::Never` or build without default features:

```toml
tracing-systemd = { version = "0.4", default-features = false }
```

`ColorTheme` has one slot per rendered element. **Only the five levels carry colour by
default** — every other slot is `Style::new()`, so the default palette renders exactly
the line 0.1 did. Fill a slot to colour it:

| Slot | Colours | Default |
| --- | --- | --- |
| `trace` `debug` `info` `warn` `error` | the level and `{level_short}` | coloured |
| `span_name` | `{leaf}`, `{spans}`, `{span}` | none |
| `field_name` | field names in span args and `{fields}` | none |
| `field_value` | field values, including `{message}` | none |
| `target` | `{target}` | none |
| `thread_id` | `{thread_id}`, brackets included | none |
| `timestamp` | `{timestamp}` | none |
| `file` | `{file}` | none |
| `line` | `{line}` | none |

```rust,ignore
use nu_ansi_term::{Color, Style};
use tracing_systemd::ColorTheme;

let theme = ColorTheme {
    info: Style::new().fg(Color::Cyan).bold(),
    target: Style::new().dimmed(),
    ..ColorTheme::default()
};
```

`ColorTheme::monochrome()` zeroes every slot, including the levels.

Once a slot has a colour, `:plain` in a template becomes useful for every element:

```rust,ignore
// toolbar dimmed, message bare — same line
"{target} {message:plain}"
```

---

# Logging to the journal

There are two ways into journald, and they differ in what the journal stores.

**Under a systemd unit**, stdout is already routed to the journal.
`SystemdLayer::unit_stdout()` emits the `<3>`–`<7>` syslog priority prefix that
`journalctl` uses to assign levels, and disables colour:

```rust,ignore
tracing_subscriber::registry()
    .with(SystemdLayer::unit_stdout())
    .init();
```

**Outside a unit**, or when you want native journald fields, enable the `journald`
feature and write to the socket instead of stdout:

```rust,ignore
use tracing_systemd::{Output, SystemdLayer};

let output = Output::journald()
    .expect("no journal socket")
    .with_syslog_identifier("my-app");

tracing_subscriber::registry()
    .with(SystemdLayer::stdout().with_output(output))
    .init();
```

The journal receives **the same rendered line stdout would**, span chain and all, plus
native `PRIORITY`, `TARGET`, `CODE_FILE`, `CODE_LINE` and one `SPAN_NAME` per parent
span. So `journalctl -t my-app` shows a readable log *and* the entries stay queryable
(`journalctl PRIORITY=3`). Every formatting option applies, because the sink sits
behind the normal renderer rather than replacing it.

The syslog `<n>` prefix is suppressed on a journald sink — `PRIORITY` already carries
the level, so a `<5>` inside `MESSAGE` would be duplicated data.

To write to both places, share one configuration:

```rust,ignore
tracing_subscriber::registry()
    .with(SystemdLayer::stdout())
    .with(journald_output.map(|o| SystemdLayer::stdout().with_output(o)))
    .init();
```

`Option<Layer>` implements `Layer<S>`, so `None` (no socket) becomes a no-op without
an `if let`.

> **Note on `tracing-journald`:** earlier versions of this crate re-exported it. It
> cannot show formatted output: its `MESSAGE` is built from the event's `message`
> field alone, so span chains never reach the journal. That is why the transport is
> built in (see `src/journald.rs`).

# JSON output

```toml
tracing-systemd = { version = "0.4", features = ["json"] }
```

```rust,ignore
tracing_subscriber::registry()
    .with(SystemdLayer::json().with_thread_ids(true))
    .init();
```

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

Defaults differ from the pretty constructors: `target` is on, timestamps default to
RFC 3339 in UTC, and the syslog prefix stays off so each line is a standalone JSON
object. Field types are preserved (`bool`, integer, float, string); non-finite floats
become `null`, matching `tracing-subscriber`'s JSON formatter and `JSON.stringify`.

Pretty-only builders (`with_color_*`, separators, brackets, thread-id prefix/suffix,
`template`) compile but have no effect in JSON mode. `with_level_prefix(true)` does
still apply and produces a line that is no longer parsable as JSON — useful only when
you want an explicit `PRIORITY` alongside a JSON payload.

# Testing

Redirect output to any `io::Write`:

```rust,ignore
use std::sync::{Arc, Mutex};
use tracing_systemd::{Output, SystemdLayer};

let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
let layer = SystemdLayer::stdout()
    .with_output(Output::writer(MyShared(buf.clone())));
```

# Migrating

## 0.3 → 0.4

`0.4` changes the default line format and **removes** `tracing_systemd::journald`
along with the `tracing_journald` re-export. Code that called `journald::layer()` or
`journald::layer_with_identifier("my-app")` will not compile until it moves to
`Output::journald()`.

The rest of this section is about behaviour that changes silently.

| Change | Before | After |
|--------|--------|-------|
| Syslog prefix | `stdout()` emitted `<5>INFO ...` by default | default is `false`; `unit_stdout()` is the prefixed constructor |
| String field values | span args unquoted, event fields quoted | both quoted and escaped |
| `with_timestamps(bool)` | set a flag no renderer read, so `true` alone printed nothing | `true` selects RFC 3339, `false` selects `TimestampFormat::None` |
| `show_timestamp` | separate knob alongside `timestamp_format` | gone; `TimestampFormat::None` is the only off-switch |
| Event field tail | hardcoded `", "` / `": "` / `{}`, ignoring the builder | honours `with_arguments_*=`, and has its own `with_field_bracket_*` |
| `span.record(..)`, late `#[instrument]` params, `field::Empty` | silently dropped (no `on_record`) | captured and printed |
| `ColorTheme::{span_name, field_name, field_value, target}` | accepted, never read | applied |
| `{timestamp}`, `{file}`, `{line}` | not stylable at all | `ColorTheme::{timestamp, file, line}` added, defaulting to `Style::new()` so output is unchanged |
| `tracing_systemd::journald::{layer, layer_with_identifier}`, `tracing_journald` | formatted only `MESSAGE`; span chains never reached the journal | removed; `Output::journald()` carries the full rendered line |

If you relied on the prefixed default, switch `SystemdLayer::stdout()` to
`SystemdLayer::unit_stdout()`. That one does not fail at compile time and it matters:
under a unit the `<n>` prefix is how `journalctl -p err` finds your errors, so an
upgrading service quietly loses level filtering. Check any `stdout()` constructor in
code that runs under a unit.

If you relied on unquoted span arguments, use `with_quotes(false)` or the `raw`
modifier. Quoting stays on by default because it keeps `f(s: "a, b")` unambiguous.

## 0.1 → 0.2

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
| `use_sd_journal(true)`             | `SystemdLayer::stdout().with_output(Output::journald()?)` (needs `journald`) |

`with_target` and `with_thread_ids` are unchanged. `with_timestamps`,
`with_timestamp_format`, `with_color_theme`, and `with_output` are new.

Other changes:

- Default colour is now `ColorMode::Auto` (respects `NO_COLOR` and TTY status). Pass
  `ColorMode::Always` for the old behaviour.
- The `colored` feature was renamed to `colors`.
- The `sd-journal` feature is gone; use `journald`, which speaks the journal protocol
  directly (pure Rust, no `libsystemd-dev`).
- The runtime `use_sd_journal(false)` toggle is gone. Pick the layer at construction
  time instead.

# MSRV

Rust 1.85 (edition 2024). MSRV bumps are minor version bumps.

# License

[MIT](LICENSE)
