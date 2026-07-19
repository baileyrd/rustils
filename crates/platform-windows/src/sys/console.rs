//! Console terminal primitives (extraction map D9, via the rusty_win32
//! donor): tty probe, viewport size, raw mode over console modes, and
//! (slice 2, roadmap Phase 2) live raw-mode probing, poll/read on
//! stdin, and echo toggling.
//!
//! The isatty analog IS `GetConsoleMode` succeeding — a redirected
//! (pipe/file) std handle fails the call. Raw mode clears the cooked
//! input bits (echo, line buffering, Ctrl-C processing) and sets the
//! virtual-terminal bits so a Win10+ console speaks the same byte
//! dialect as a Unix tty in raw mode.

#![allow(unsafe_code)]

use std::ffi::OsStr;
use std::time::Duration;

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

/// Live probe: does stdin's *current* mode look raw (no `ENABLE_ECHO_INPUT`,
/// no `ENABLE_LINE_INPUT`)? A handle that cannot be queried is not
/// usefully "raw" — same best-effort contract as the Linux arm.
pub fn is_raw() -> bool {
    let hin = std_handle(TermStream::Stdin);
    if hin.is_null() || hin == w::INVALID_HANDLE_VALUE {
        return false;
    }
    let mut mode: w::CONSOLE_MODE = 0;
    // SAFETY: `hin` is a live process-owned std handle; `mode` is a
    // valid out-pointer for the duration of the call.
    if unsafe { w::GetConsoleMode(hin, &mut mode) } == 0 {
        return false;
    }
    mode & (w::ENABLE_ECHO_INPUT | w::ENABLE_LINE_INPUT) == 0
}

/// Toggle `ENABLE_ECHO_INPUT` on stdin without touching any other bit,
/// returning the previous on/off state.
pub fn set_echo(on: bool) -> Result<bool> {
    let hin = std_handle(TermStream::Stdin);
    let mut mode: w::CONSOLE_MODE = 0;
    // SAFETY: live std handle; valid out-pointer.
    if unsafe { w::GetConsoleMode(hin, &mut mode) } == 0 {
        return Err(errmap::last_win32_err("GetConsoleMode", OsStr::new("")));
    }
    let was_on = mode & w::ENABLE_ECHO_INPUT != 0;
    let next = if on {
        mode | w::ENABLE_ECHO_INPUT
    } else {
        mode & !w::ENABLE_ECHO_INPUT
    };
    // SAFETY: live console handle (GetConsoleMode above succeeded); no
    // pointer arguments.
    if unsafe { w::SetConsoleMode(hin, next) } == 0 {
        return Err(errmap::last_win32_err("SetConsoleMode", OsStr::new("")));
    }
    Ok(was_on)
}

/// `WaitForSingleObject(stdin, timeout_ms)`; `None` timeout blocks
/// forever. A console input handle is "signaled" when an unread input
/// record is queued — coarser than "a byte is ready" (any input event,
/// not just keystrokes, wakes it), but `ReadFile` afterward blocks
/// correctly on whatever was actually queued, so a spurious wake costs
/// one extra round trip, never a wrong read.
pub fn poll_readable(timeout: Option<Duration>) -> Result<bool> {
    let hin = std_handle(TermStream::Stdin);
    let timeout_ms: u32 = match timeout {
        None => w::INFINITE,
        Some(d) => u32::try_from(d.as_millis()).unwrap_or(u32::MAX),
    };
    // SAFETY: `hin` is a live, waitable std handle.
    let r = unsafe { w::WaitForSingleObject(hin, timeout_ms) };
    if r == w::WAIT_OBJECT_0 {
        Ok(true)
    } else if r == w::WAIT_TIMEOUT {
        Ok(false)
    } else {
        Err(errmap::last_win32_err(
            "WaitForSingleObject",
            OsStr::new(""),
        ))
    }
}

/// `ReadFile(stdin, buf)` — one call, batched, `Ok(0)` = EOF.
pub fn read_chunk(buf: &mut [u8]) -> Result<usize> {
    let hin = std_handle(TermStream::Stdin);
    let mut n: u32 = 0;
    let len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
    // SAFETY: `buf` is a valid writable region of at least `len` bytes
    // and `n` a valid out-pointer, both outliving the call; the handle
    // is synchronous (no OVERLAPPED), so the null overlapped is valid.
    let ok = unsafe { w::ReadFile(hin, buf.as_mut_ptr(), len, &mut n, std::ptr::null_mut()) };
    if ok == 0 {
        return Err(errmap::last_win32_err("ReadFile", OsStr::new("")));
    }
    Ok(n as usize)
}
