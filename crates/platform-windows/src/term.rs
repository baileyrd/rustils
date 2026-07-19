//! `Terminal` impl over `sys::console` (extraction map D9).

use platform::error::Result;
use platform::term::{TermStream, Terminal, WinSize};

use crate::sys::console;

/// The Windows terminal, over the process's std handles. Raw-mode state
/// (the saved console modes) lives here for correct, idempotent
/// enter/leave pairing.
#[derive(Default)]
pub struct WindowsTerminal {
    saved: Option<console::SavedModes>,
}

impl WindowsTerminal {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Terminal for WindowsTerminal {
    fn is_tty(&self, stream: TermStream) -> bool {
        console::is_tty(stream)
    }

    fn window_size(&self) -> Result<WinSize> {
        let (rows, cols) = console::window_size()?;
        Ok(WinSize { rows, cols })
    }

    fn enter_raw(&mut self) -> Result<()> {
        if self.saved.is_some() {
            return Ok(());
        }
        self.saved = Some(console::enter_raw()?);
        Ok(())
    }

    fn leave_raw(&mut self) -> Result<()> {
        if let Some(saved) = self.saved.take() {
            console::restore(&saved)?;
        }
        Ok(())
    }
}
