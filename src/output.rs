//! The destination a [`SystemdLayer`](crate::SystemdLayer) writes to.
//!
//! Use [`Output::stdout`] / [`Output::stderr`] for the typical case, or
//! [`Output::writer`] to capture into any [`io::Write`] (handy in tests).

use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::sync::Mutex;

/// Where a layer writes formatted lines.
pub struct Output {
    sink: Sink,
    is_terminal: bool,
}

enum Sink {
    Stdout,
    Stderr,
    Custom(Mutex<Box<dyn Write + Send>>),
}

impl Output {
    /// Write to standard output. TTY-detected at construction.
    #[must_use]
    pub fn stdout() -> Self {
        Self {
            sink: Sink::Stdout,
            is_terminal: io::stdout().is_terminal(),
        }
    }

    /// Write to standard error. TTY-detected at construction.
    #[must_use]
    pub fn stderr() -> Self {
        Self {
            sink: Sink::Stderr,
            is_terminal: io::stderr().is_terminal(),
        }
    }

    /// Write to any [`io::Write`]. Treated as non-TTY (no auto-color).
    ///
    /// Useful for testing, or to send formatted lines to a file.
    /// The writer is held behind a mutex so the layer can write from any thread.
    pub fn writer<W>(writer: W) -> Self
    where
        W: Write + Send + 'static,
    {
        Self {
            sink: Sink::Custom(Mutex::new(Box::new(writer))),
            is_terminal: false,
        }
    }

    /// Whether the output is connected to a terminal. Used for color auto-detection.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.is_terminal
    }

    /// Write a single line. Errors are silently dropped — a logging layer
    /// should never crash the program because stdout/stderr/writer failed.
    pub(crate) fn write_line(&self, line: &str) {
        match &self.sink {
            Sink::Stdout => {
                let mut guard = io::stdout().lock();
                let _ = writeln!(guard, "{line}");
            }
            Sink::Stderr => {
                let mut guard = io::stderr().lock();
                let _ = writeln!(guard, "{line}");
            }
            Sink::Custom(m) => {
                // Recover from poisoning rather than panicking in a logger.
                let mut guard = m.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                let _ = writeln!(guard, "{line}");
            }
        }
    }
}

impl fmt::Debug for Output {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match &self.sink {
            Sink::Stdout => "Stdout",
            Sink::Stderr => "Stderr",
            Sink::Custom(_) => "Custom",
        };
        f.debug_struct("Output")
            .field("sink", &kind)
            .field("is_terminal", &self.is_terminal)
            .finish()
    }
}

impl Default for Output {
    fn default() -> Self {
        Self::stdout()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);
    impl Write for Buf {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn writer_captures_lines() {
        let buf = Buf::default();
        let captured = buf.0.clone();
        let out = Output::writer(buf);

        out.write_line("hello");
        out.write_line("world");

        let bytes = captured.lock().unwrap();
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), "hello\nworld\n");
    }

    #[test]
    fn writer_is_not_a_terminal() {
        let buf = Buf::default();
        let out = Output::writer(buf);
        assert!(!out.is_terminal());
    }

    #[test]
    fn debug_does_not_panic() {
        let _ = format!("{:?}", Output::stdout());
        let _ = format!("{:?}", Output::stderr());
        let _ = format!("{:?}", Output::writer(Vec::<u8>::new()));
    }
}
