//! Deferred signal delivery — the D6 core, verbatim in spirit from
//! rush's `trap.rs`: the installed handler is a single atomic store,
//! nothing more. No allocation, no locks, no I/O — the entire
//! async-signal-safe budget is one `AtomicI32::store`.

#![allow(unsafe_code)]

use std::ffi::OsStr;
use std::sync::atomic::{AtomicI32, Ordering};

use platform::error::{ErrorKind, OsCode, PlatformError, Result};

use crate::ffi::libc_surface as c;

/// The single slot. Signal dispositions are process-global on every OS,
/// so this static is the honest shape, not a shortcut.
static PENDING: AtomicI32 = AtomicI32::new(0);

/// The entire handler (async-signal-safe by construction: one atomic
/// store, sequentially consistent to pair with [`take`]'s swap).
extern "C" fn record(signum: c::c_int) {
    PENDING.store(signum, Ordering::SeqCst);
}

/// Install [`record`] for `signum`.
pub fn install(signum: c::c_int) -> Result<()> {
    let handler: extern "C" fn(c::c_int) = record;
    // SAFETY: `handler` is an async-signal-safe extern "C" routine (one
    // atomic store); `signal` accepts a handler function pointer for a
    // valid signal number. glibc's signal() gives BSD semantics
    // (SA_RESTART), matching the donor.
    let prev = unsafe { c::signal(signum, handler as c::sighandler_t) };
    if prev == c::SIG_ERR {
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        return Err(
            PlatformError::new(ErrorKind::InvalidInput, OsCode::Errno(code), "signal")
                .with_path(OsStr::new("")),
        );
    }
    Ok(())
}

/// Consume the pending signal number, if any (atomic swap with 0).
pub fn take() -> Option<c::c_int> {
    match PENDING.swap(0, Ordering::SeqCst) {
        0 => None,
        signum => Some(signum),
    }
}
