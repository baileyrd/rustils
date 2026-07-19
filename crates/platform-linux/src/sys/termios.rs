//! Terminal primitives (extraction map D9): isatty, window size, raw
//! mode with saved-state restore. Both floors: libc (`tcgetattr` /
//! `cfmakeraw` / `ioctl(TIOCGWINSZ)`) and rusty_libc raw syscalls under
//! `track-p` (kernel-shape `Termios`, NCCS=19 — the D4 layout landmine
//! lives in the dependency, not here).

#![allow(unsafe_code)]

use platform::error::{ErrorKind, OsCode, PlatformError, Result};

use crate::ffi::libc_surface as c;

fn term_err(op: &'static str, code: i32) -> PlatformError {
    let kind = match code {
        // ENOTTY is the "stream is not a terminal" answer — the caller-
        // visible case behavior/term.md specifies.
        libc::ENOTTY => ErrorKind::Other,
        libc::EBADF => ErrorKind::InvalidInput,
        _ => ErrorKind::Other,
    };
    PlatformError::new(kind, OsCode::Errno(code), op)
}

/// The raw fd of a standard stream — private to this module; fd numbers
/// stop at the sys boundary (RFC v2 §5).
pub fn stream_fd(stream: platform::term::TermStream) -> i32 {
    match stream {
        platform::term::TermStream::Stdin => c::STDIN_FILENO,
        platform::term::TermStream::Stdout => c::STDOUT_FILENO,
        platform::term::TermStream::Stderr => c::STDERR_FILENO,
    }
}

/// `isatty(fd)`.
#[cfg(not(feature = "track-p"))]
pub fn is_tty(fd: i32) -> bool {
    // SAFETY: isatty takes a plain integer and has no pointer arguments.
    (unsafe { c::isatty(fd) }) == 1
}

/// `isatty(fd)` — Track P: a `tcgetattr` probe (that is all isatty is).
#[cfg(feature = "track-p")]
pub fn is_tty(fd: i32) -> bool {
    rusty_libc::termios::isatty(fd)
}

/// `ioctl(fd, TIOCGWINSZ)` → (rows, cols).
#[cfg(not(feature = "track-p"))]
pub fn window_size(fd: i32) -> Result<(u16, u16)> {
    // SAFETY: `winsize` is plain-old-data; all-zeroes is a valid value
    // the kernel overwrites on success.
    let mut ws: c::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: valid fd; `ws` is a valid out-pointer of the exact struct
    // TIOCGWINSZ writes, outliving the call.
    let r = unsafe { c::ioctl(fd, c::TIOCGWINSZ, &mut ws) };
    if r != 0 {
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        return Err(term_err("TIOCGWINSZ", code));
    }
    Ok((ws.ws_row, ws.ws_col))
}

/// Window size — Track P: raw `ioctl(TIOCGWINSZ)` via rusty_libc.
#[cfg(feature = "track-p")]
pub fn window_size(fd: i32) -> Result<(u16, u16)> {
    let ws = rusty_libc::tty::window_size(fd).map_err(|e| term_err("TIOCGWINSZ", e.0))?;
    Ok((ws.ws_row, ws.ws_col))
}

/// The saved terminal state [`enter_raw`] returns and [`restore`] takes
/// back — opaque to callers, one per backend arm.
#[cfg(not(feature = "track-p"))]
pub struct SavedTermios(c::termios);
/// Track P saved state: the kernel-shape `Termios`.
#[cfg(feature = "track-p")]
pub struct SavedTermios(rusty_libc::termios::Termios);

/// Read the current attributes, apply the raw-mode recipe
/// (`cfmakeraw`), and return the previous state for [`restore`].
#[cfg(not(feature = "track-p"))]
pub fn enter_raw(fd: i32) -> Result<SavedTermios> {
    // SAFETY: `termios` is plain-old-data; zeroed is valid scratch that
    // tcgetattr overwrites on success.
    let mut cur: c::termios = unsafe { std::mem::zeroed() };
    // SAFETY: valid fd; `cur` is a valid out-pointer outliving the call.
    if unsafe { c::tcgetattr(fd, &mut cur) } != 0 {
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        return Err(term_err("tcgetattr", code));
    }
    let saved = SavedTermios(cur);
    let mut raw = cur;
    // SAFETY: `raw` is a valid termios initialized by tcgetattr above;
    // cfmakeraw only writes its flag fields.
    unsafe { c::cfmakeraw(&mut raw) };
    // SAFETY: valid fd; `raw` is a fully initialized termios outliving
    // the call. TCSADRAIN lets pending output flush before the switch.
    if unsafe { c::tcsetattr(fd, c::TCSADRAIN, &raw) } != 0 {
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        return Err(term_err("tcsetattr", code));
    }
    Ok(saved)
}

/// Raw mode — Track P: rusty_libc's kernel-layout `Termios` and its
/// `make_raw` (the same cfmakeraw recipe, spelled against NCCS=19).
#[cfg(feature = "track-p")]
pub fn enter_raw(fd: i32) -> Result<SavedTermios> {
    use rusty_libc::termios as rt;
    let cur = rt::tcgetattr(fd).map_err(|e| term_err("tcgetattr", e.0))?;
    let saved = SavedTermios(cur);
    let mut raw = cur;
    raw.make_raw();
    rt::tcsetattr_with(fd, rt::TCSADRAIN, &raw).map_err(|e| term_err("tcsetattr", e.0))?;
    Ok(saved)
}

/// Restore the state saved by [`enter_raw`].
#[cfg(not(feature = "track-p"))]
pub fn restore(fd: i32, saved: &SavedTermios) -> Result<()> {
    // SAFETY: valid fd; `saved.0` is the fully initialized termios
    // captured by enter_raw, outliving the call.
    if unsafe { c::tcsetattr(fd, c::TCSADRAIN, &saved.0) } != 0 {
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        return Err(term_err("tcsetattr", code));
    }
    Ok(())
}

/// Restore — Track P.
#[cfg(feature = "track-p")]
pub fn restore(fd: i32, saved: &SavedTermios) -> Result<()> {
    rusty_libc::termios::tcsetattr_with(fd, rusty_libc::termios::TCSADRAIN, &saved.0)
        .map_err(|e| term_err("tcsetattr", e.0))
}
