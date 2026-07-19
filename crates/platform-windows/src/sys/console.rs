//! Console terminal primitives (extraction map D9, via the rusty_win32
//! donor): tty probe, viewport size, raw mode over console modes.
//!
//! The isatty analog IS `GetConsoleMode` succeeding — a redirected
//! (pipe/file) std handle fails the call. Raw mode clears the cooked
//! input bits (echo, line buffering, Ctrl-C processing) and sets the
//! virtual-terminal bits so a Win10+ console speaks the same byte
//! dialect as a Unix tty in raw mode.

#![allow(unsafe_code)]

use std::ffi::OsStr;

use platform::error::Result;
use platform::term::TermStream;

use crate::ffi::win32_surface as w;
use crate::sys::errmap;

/// The std-handle slot for a stream — handle values stop at this
/// boundary (RFC v2 §5).
fn slot(stream: TermStream) -> u32 {
    match stream {
        TermStream::Stdin => w::STD_INPUT_HANDLE,
        TermStream::Stdout => w::STD_OUTPUT_HANDLE,
        TermStream::Stderr => w::STD_ERROR_HANDLE,
    }
}

fn std_handle(stream: TermStream) -> w::HANDLE {
    // SAFETY: GetStdHandle takes a documented slot constant and has no
    // pointer arguments; the returned handle is process-owned and must
    // not be closed here.
    unsafe { w::GetStdHandle(slot(stream)) }
}

/// Whether `stream`'s handle is a console (`GetConsoleMode` succeeds).
pub fn is_tty(stream: TermStream) -> bool {
    let h = std_handle(stream);
    if h.is_null() || h == w::INVALID_HANDLE_VALUE {
        return false;
    }
    let mut mode: w::CONSOLE_MODE = 0;
    // SAFETY: `h` is a live process-owned std handle; `mode` is a valid
    // out-pointer for the duration of the call.
    (unsafe { w::GetConsoleMode(h, &mut mode) }) != 0
}

/// Viewport size (srWindow, not the scrollback buffer) of the first
/// console-attached std stream.
pub fn window_size() -> Result<(u16, u16)> {
    for stream in [TermStream::Stdout, TermStream::Stderr, TermStream::Stdin] {
        if !is_tty(stream) {
            continue;
        }
        let h = std_handle(stream);
        // SAFETY: CONSOLE_SCREEN_BUFFER_INFO is plain-old-data; zeroed
        // is valid scratch the call overwrites on success.
        let mut info: w::CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
        // SAFETY: `h` is a live console handle (probed above); `info`
        // is a valid out-pointer outliving the call.
        if unsafe { w::GetConsoleScreenBufferInfo(h, &mut info) } == 0 {
            return Err(errmap::last_win32_err(
                "GetConsoleScreenBufferInfo",
                OsStr::new(""),
            ));
        }
        let cols = (info.srWindow.Right - info.srWindow.Left + 1).max(0) as u16;
        let rows = (info.srWindow.Bottom - info.srWindow.Top + 1).max(0) as u16;
        return Ok((rows, cols));
    }
    Err(errmap::last_win32_err("GetConsoleMode", OsStr::new("")))
}

/// The saved console modes `enter_raw` returns and `restore` takes back.
pub struct SavedModes {
    input: w::CONSOLE_MODE,
    output: Option<w::CONSOLE_MODE>,
}

/// Switch stdin (and stdout when attached) to raw mode, returning the
/// previous modes.
pub fn enter_raw() -> Result<SavedModes> {
    let hin = std_handle(TermStream::Stdin);
    let mut in_mode: w::CONSOLE_MODE = 0;
    // SAFETY: live std handle; valid out-pointer.
    if unsafe { w::GetConsoleMode(hin, &mut in_mode) } == 0 {
        return Err(errmap::last_win32_err("GetConsoleMode", OsStr::new("")));
    }
    let raw_in = (in_mode
        & !(w::ENABLE_ECHO_INPUT | w::ENABLE_LINE_INPUT | w::ENABLE_PROCESSED_INPUT))
        | w::ENABLE_VIRTUAL_TERMINAL_INPUT;
    // SAFETY: live console handle (GetConsoleMode above succeeded); no
    // pointer arguments.
    if unsafe { w::SetConsoleMode(hin, raw_in) } == 0 {
        return Err(errmap::last_win32_err("SetConsoleMode", OsStr::new("")));
    }

    // Output VT processing is best-effort: stdout may be redirected
    // while stdin is still a console.
    let mut output = None;
    if is_tty(TermStream::Stdout) {
        let hout = std_handle(TermStream::Stdout);
        let mut out_mode: w::CONSOLE_MODE = 0;
        // SAFETY: live console handle; valid out-pointer.
        if unsafe { w::GetConsoleMode(hout, &mut out_mode) } != 0 {
            let vt = out_mode | w::ENABLE_PROCESSED_OUTPUT | w::ENABLE_VIRTUAL_TERMINAL_PROCESSING;
            // SAFETY: live console handle; no pointer arguments.
            if unsafe { w::SetConsoleMode(hout, vt) } != 0 {
                output = Some(out_mode);
            }
        }
    }
    Ok(SavedModes {
        input: in_mode,
        output,
    })
}

/// Restore the modes saved by [`enter_raw`].
pub fn restore(saved: &SavedModes) -> Result<()> {
    let hin = std_handle(TermStream::Stdin);
    // SAFETY: live std handle; no pointer arguments.
    if unsafe { w::SetConsoleMode(hin, saved.input) } == 0 {
        return Err(errmap::last_win32_err("SetConsoleMode", OsStr::new("")));
    }
    if let Some(out_mode) = saved.output {
        let hout = std_handle(TermStream::Stdout);
        // SAFETY: live std handle; no pointer arguments.
        unsafe { w::SetConsoleMode(hout, out_mode) };
    }
    Ok(())
}
