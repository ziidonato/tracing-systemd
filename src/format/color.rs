//! Color handling: [`ColorMode`] (when to apply ANSI) and [`ColorTheme`]
//! (which styles to use per element).

#![cfg(feature = "colors")]

use nu_ansi_term::{Color, Style};

/// When to emit ANSI color escapes on stdout output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(docsrs, doc(cfg(feature = "colors")))]
pub enum ColorMode {
    /// Emit colors only when the output is a TTY *and* the `NO_COLOR`
    /// environment variable is not set.
    #[default]
    Auto,
    /// Always emit colors, regardless of TTY status or `NO_COLOR`.
    Always,
    /// Never emit colors.
    Never,
}

impl ColorMode {
    /// Resolve the mode given runtime conditions. Pure function for testability;
    /// the layer evaluates `is_terminal` and `no_color_set` at construction time.
    pub(crate) fn resolve(self, is_terminal: bool, no_color_set: bool) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => is_terminal && !no_color_set,
        }
    }

    /// Resolve using the current process's `NO_COLOR` env var.
    pub(crate) fn resolve_now(self, is_terminal: bool) -> bool {
        self.resolve(is_terminal, std::env::var_os("NO_COLOR").is_some())
    }
}

/// Per-element ANSI styles applied to formatted output.
///
/// Construct via [`ColorTheme::default`] (matches the 0.1 palette) or
/// [`ColorTheme::monochrome`] (no styling), then override individual
/// fields as needed.
///
/// ```
/// use tracing_systemd::ColorTheme;
/// use nu_ansi_term::{Color, Style};
///
/// let theme = ColorTheme {
///     info: Style::new().fg(Color::Cyan).bold(),
///     ..ColorTheme::default()
/// };
/// # let _ = theme;
/// ```
#[derive(Debug, Clone, Copy)]
#[cfg_attr(docsrs, doc(cfg(feature = "colors")))]
#[allow(missing_docs)] // Field names are self-documenting.
pub struct ColorTheme {
    pub trace: Style,
    pub debug: Style,
    pub info: Style,
    pub warn: Style,
    pub error: Style,
    pub span_name: Style,
    pub field_name: Style,
    pub field_value: Style,
    pub target: Style,
    pub thread_id: Style,
}

impl ColorTheme {
    /// All-empty theme. Useful for users who want structure without ANSI
    /// (e.g. when piping to a log file but still wanting the runtime
    /// option of [`ColorMode::Always`]).
    #[must_use]
    pub fn monochrome() -> Self {
        Self {
            trace: Style::new(),
            debug: Style::new(),
            info: Style::new(),
            warn: Style::new(),
            error: Style::new(),
            span_name: Style::new(),
            field_name: Style::new(),
            field_value: Style::new(),
            target: Style::new(),
            thread_id: Style::new(),
        }
    }

    /// Style for a given level. Used internally by the formatter.
    pub(crate) fn level_style(&self, level: tracing::Level) -> Style {
        match level {
            tracing::Level::TRACE => self.trace,
            tracing::Level::DEBUG => self.debug,
            tracing::Level::INFO => self.info,
            tracing::Level::WARN => self.warn,
            tracing::Level::ERROR => self.error,
        }
    }
}

impl Default for ColorTheme {
    /// Matches the color palette used by `tracing-systemd` 0.1.
    fn default() -> Self {
        Self {
            trace: Style::new().fg(Color::Magenta),
            debug: Style::new().fg(Color::Blue),
            info: Style::new().fg(Color::Green),
            warn: Style::new().fg(Color::Yellow),
            error: Style::new().fg(Color::Red),
            span_name: Style::new(),
            field_name: Style::new(),
            field_value: Style::new(),
            target: Style::new(),
            thread_id: Style::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_resolves_against_tty_and_no_color() {
        assert!(ColorMode::Auto.resolve(true, false));
        assert!(!ColorMode::Auto.resolve(true, true));
        assert!(!ColorMode::Auto.resolve(false, false));
        assert!(!ColorMode::Auto.resolve(false, true));
    }

    #[test]
    fn always_overrides_tty_and_env() {
        assert!(ColorMode::Always.resolve(false, true));
        assert!(ColorMode::Always.resolve(true, true));
    }

    #[test]
    fn never_overrides_tty_and_env() {
        assert!(!ColorMode::Never.resolve(true, false));
    }

    #[test]
    fn level_style_picks_correct_style() {
        let theme = ColorTheme::default();
        assert_eq!(theme.level_style(tracing::Level::INFO), theme.info);
        assert_eq!(theme.level_style(tracing::Level::ERROR), theme.error);
    }

    #[test]
    fn monochrome_has_empty_styles() {
        let theme = ColorTheme::monochrome();
        let plain = Style::new();
        assert_eq!(theme.info, plain);
        assert_eq!(theme.error, plain);
        assert_eq!(theme.span_name, plain);
    }
}
