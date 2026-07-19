//! Scripted `Terminal`: tests configure tty-ness and size by hand and
//! observe raw-mode transitions, under the same contract the native
//! backends implement (idempotent enter/leave, errors when not a tty).

use platform::error::{ErrorKind, OsCode, PlatformError, Result};
use platform::term::{TermStream, Terminal, WinSize};

/// A mock terminal. Defaults to "not a tty at all" — the shape a CI
/// pipe presents — so tests opt *in* to tty behavior.
#[derive(Debug, Default)]
pub struct MockTerminal {
    stdin_tty: bool,
    stdout_tty: bool,
    stderr_tty: bool,
    size: Option<WinSize>,
    raw: bool,
    raw_transitions: u32,
}

impl MockTerminal {
    pub fn new() -> Self {
        Self::default()
    }

    /// A fully-interactive terminal of the given size.
    pub fn interactive(rows: u16, cols: u16) -> Self {
        Self {
            stdin_tty: true,
            stdout_tty: true,
            stderr_tty: true,
            size: Some(WinSize { rows, cols }),
            raw: false,
            raw_transitions: 0,
        }
    }

    /// Mark one stream as a tty.
    pub fn set_tty(&mut self, stream: TermStream, tty: bool) {
        match stream {
            TermStream::Stdin => self.stdin_tty = tty,
            TermStream::Stdout => self.stdout_tty = tty,
            TermStream::Stderr => self.stderr_tty = tty,
        }
    }

    /// Whether the terminal is currently in raw mode.
    pub fn in_raw(&self) -> bool {
        self.raw
    }

    /// How many times raw mode was actually entered (idempotent
    /// re-enters do not count — that is the contract under test).
    pub fn raw_transitions(&self) -> u32 {
        self.raw_transitions
    }
}

fn not_a_tty(op: &'static str) -> PlatformError {
    PlatformError::new(ErrorKind::Other, OsCode::None, op)
}

impl Terminal for MockTerminal {
    fn is_tty(&self, stream: TermStream) -> bool {
        match stream {
            TermStream::Stdin => self.stdin_tty,
            TermStream::Stdout => self.stdout_tty,
            TermStream::Stderr => self.stderr_tty,
        }
    }

    fn window_size(&self) -> Result<WinSize> {
        self.size.ok_or_else(|| not_a_tty("window_size"))
    }

    fn enter_raw(&mut self) -> Result<()> {
        if !self.stdin_tty {
            return Err(not_a_tty("enter_raw"));
        }
        if !self.raw {
            self.raw = true;
            self.raw_transitions += 1;
        }
        Ok(())
    }

    fn leave_raw(&mut self) -> Result<()> {
        self.raw = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_no_tty_and_size_errors() {
        let t = MockTerminal::new();
        assert!(!t.is_tty(TermStream::Stdin));
        assert!(t.window_size().is_err());
    }

    #[test]
    fn raw_mode_is_idempotent_and_gated_on_stdin_tty() {
        let mut t = MockTerminal::new();
        assert!(t.enter_raw().is_err(), "raw mode without a tty must fail");

        let mut t = MockTerminal::interactive(24, 80);
        t.enter_raw().unwrap();
        t.enter_raw().unwrap();
        assert!(t.in_raw());
        assert_eq!(t.raw_transitions(), 1, "re-enter must be a no-op");
        t.leave_raw().unwrap();
        t.leave_raw().unwrap();
        assert!(!t.in_raw());
        assert_eq!(t.window_size().unwrap(), WinSize { rows: 24, cols: 80 });
    }
}
