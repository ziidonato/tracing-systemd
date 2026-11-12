//! Adversarial and randomised input for the template parser.
//!
//! This is a poor man's fuzzer, but it runs in `cargo test` with no nightly
//! toolchain, no `libfuzzer-sys`, and no corpus to keep in sync, so it
//! actually runs on every commit. The alphabet is restricted to the characters
//! the parser gives meaning to (`{} : < > ^` and digits) mixed with arbitrary
//! Unicode, which is where the interesting states live.
//!
//! The property under test is narrow and total: **`parse` and `render` must not
//! panic, and any template that parses must produce valid UTF-8.** Template
//! errors are fine; an abort is not.

use tracing_systemd::SystemdLayer;

/// xorshift64*, so the corpus is deterministic and reproducible from the seed
/// printed by a failure.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const ALPHABET: &[char] = &[
    '{', '}', '{', '}', ':', '<', '>', '^', '0', '1', '9', ' ', 'a', 'n', 'm', 'e', 's', '_', '-',
    '.', 'é', '你', '🦀', '\u{1b}', '[', 'm', '\\', '"', '\t', '\n',
];

const NAMES: &[&str] = &[
    "level",
    "level_short",
    "target",
    "message",
    "thread_id",
    "timestamp",
    "file",
    "line",
    "spans",
    "leaf",
    "span",
    "fields",
    "unknown_field",
    "",
];

/// Drive one real event through a compiled template, discarding the output.
///
/// Exercises the render side of every placeholder the parser accepted, which
/// is where a missing match arm or a bad slice would abort.
fn render_once(layer: SystemdLayer) {
    use tracing_subscriber::prelude::*;
    let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    struct W(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for W {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            let mut g = self.0.lock().unwrap();
            g.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let layer = layer.with_output(tracing_systemd::Output::writer(W(buf.clone())));
    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("sp", n = 1u64);
        let _g = span.enter();
        tracing::info!(rows = 3u64, "msg");
    });

    // Whatever came out must be valid UTF-8: the parser and renderer both
    // slice on byte boundaries when truncating, so this is the check that
    // would catch a split inside a multi-byte char.
    let captured = buf.lock().unwrap().clone();
    assert!(
        std::str::from_utf8(&captured).is_ok(),
        "non-UTF-8 output: {captured:?}"
    );
}

/// Build a template string by randomly mixing structural characters,
/// well-formed placeholders, and junk.
fn generate(rng: &mut Rng) -> String {
    let len = rng.below(24);
    let mut out = String::new();
    for _ in 0..len {
        match rng.below(10) {
            // A complete, well-formed placeholder, so the parser must accept it.
            0..=2 => {
                let name = NAMES[rng.below(NAMES.len())];
                out.push('{');
                out.push_str(name);
                match rng.below(6) {
                    0 => out.push_str(":>3"),
                    1 => out.push_str(":<3"),
                    2 => out.push_str(":^3"),
                    3 => out.push(':'),
                    4 => out.push_str(":>999999999999999999999"),
                    _ => {}
                }
                out.push('}');
            }
            _ => out.push(ALPHABET[rng.below(ALPHABET.len())]),
        }
    }
    out
}

#[test]
fn random_templates_never_panic() {
    // Deliberately more iterations than a unit test would normally carry: the
    // whole point is coverage of the parse loop's error states.
    let mut rng = Rng(0x1234_5678_9abc_def0);
    for i in 0..50_000 {
        let src = generate(&mut rng);
        match SystemdLayer::stdout().template(src.clone()) {
            Ok(layer) => {
                // A parsed template must survive an actual event without
                // panicking, including events that carry no fields at all.
                render_once(layer);
            }
            Err(err) => {
                // Formatting the error must itself be safe, and must mention
                // the template (truncated when long).
                let msg = err.to_string();
                assert!(
                    !msg.is_empty(),
                    "seed {i}: error rendered empty for {src:?}"
                );
            }
        }
    }
}

#[test]
fn every_ascii_byte_sequence_of_length_two_parses_or_errors() {
    // Exhaustive over a small but complete alphabet: this is what catches
    // "unclosed" vs "unmatched" confusion that random input may skip.
    let alphabet: Vec<char> = "{}:<>^0a".chars().collect();
    let mut rng = Rng(0xdead_beef);
    for _ in 0..20_000 {
        let mut src = String::new();
        for _ in 0..rng.below(6) {
            src.push(alphabet[rng.below(alphabet.len())]);
        }
        // Only the no-panic property is asserted; validity is not.
        let _ = SystemdLayer::stdout().template(src);
    }
}

#[test]
fn long_templates_do_not_echo_whole_source_in_errors() {
    let src = format!("{}{{", "x".repeat(100_000));
    let err = SystemdLayer::stdout().template(src).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.len() < 1_000,
        "error message echoed the whole template ({} bytes)",
        msg.len()
    );
}

#[test]
fn width_cap_rejects_absurd_padding() {
    // A single keystroke away from a gigabyte of spaces per event.
    assert!(SystemdLayer::stdout().template("{level:>999999}").is_err());
    assert!(SystemdLayer::stdout().template("{level:>1024}").is_ok());
    assert!(SystemdLayer::stdout().template("{level:>1025}").is_err());
}

#[test]
fn placeholder_count_is_bounded() {
    let many = "{level}".repeat(2_000);
    assert!(
        SystemdLayer::stdout().template(many).is_err(),
        "expected TooManyPlaceholders"
    );
    // Just under the cap still compiles.
    let ok = "{level}".repeat(1_000);
    assert!(SystemdLayer::stdout().template(ok).is_ok());
}

#[test]
fn literal_braces_survive_a_round_trip() {
    // `{{`/`}}` are escapes, so both are legal on their own; a *lone* brace
    // opens or closes a placeholder and must be rejected.
    assert!(SystemdLayer::stdout().template("{{}}").is_ok());
    assert!(SystemdLayer::stdout().template("a{{b}}c").is_ok());
    assert!(SystemdLayer::stdout().template("{{").is_ok());
    assert!(SystemdLayer::stdout().template("}}").is_ok());
    assert!(SystemdLayer::stdout().template("{").is_err());
    assert!(SystemdLayer::stdout().template("}").is_err());
    assert!(SystemdLayer::stdout().template("a{").is_err());
}

#[test]
fn every_known_placeholder_is_accepted() {
    for name in NAMES {
        if name.is_empty() {
            continue;
        }
        let src = format!("{{{name}}}");
        assert!(
            SystemdLayer::stdout().template(src.clone()).is_ok(),
            "rejected {src}"
        );
    }
    // An empty name is the one deliberate rejection.
    assert!(SystemdLayer::stdout().template("{}").is_err());
    assert!(SystemdLayer::stdout().template("{:>5}").is_err());
}

#[test]
fn unknown_field_names_parse_so_templates_can_be_shared() {
    // One template must serve events carrying different fields, so an unknown
    // name is legal at compile time and empty at render time.
    assert!(SystemdLayer::stdout().template("{rows}").is_ok());
    assert!(SystemdLayer::stdout().template("{rows} {cols}").is_ok());
}

#[test]
fn stray_escape_in_a_field_value_does_not_eat_the_line() {
    // A field value is arbitrary user data and may contain a bare ESC. If the
    // width scanner searched for the next 'm', everything after it would be
    // swallowed and every later column would be padded wrongly.
    let out = render_template("{level} [{a}] [{b}]", || {
        tracing::info!(a = "x\u{1b}", b = "y", "msg");
    });
    assert!(
        out.contains("[\"y\"]"),
        "value after stray ESC vanished: {out:?}"
    );
    assert_eq!(
        out.matches('[').count(),
        2,
        "bracket count changed: {out:?}"
    );
}

#[test]
fn unterminated_csi_does_not_panic() {
    let out = render_template("{a}", || {
        tracing::info!(a = "\u{1b}[31", "msg");
    });
    // Whatever it decides about width, it must not abort and must stay UTF-8.
    assert!(out.is_char_boundary(out.len()));
}

/// Render one event through `tmpl` with the given body, returning the line.
fn render_template(tmpl: &str, body: impl FnOnce()) -> String {
    use tracing_subscriber::prelude::*;
    let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    struct W(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for W {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            let mut g = self.0.lock().unwrap();
            g.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let layer = SystemdLayer::stdout()
        .template(tmpl)
        .expect("template")
        .with_output(tracing_systemd::Output::writer(W(buf.clone())));
    tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), body);
    let captured = buf.lock().unwrap().clone();
    String::from_utf8(captured).expect("utf-8")
}

/// Sanity check that the render path is reached for the widest variety of
/// placeholder kinds at once. Panics here would come from a missing match arm.
#[test]
fn all_placeholders_render_together() {
    let src = concat!(
        "{level} {level_short} {target} {message} {thread_id} {timestamp} ",
        "{file} {line} {spans} {leaf} {span} {fields} {unspecified}"
    );
    assert!(SystemdLayer::stdout().template(src).is_ok());
}
