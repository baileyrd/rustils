//! Owned HANDLE wrapper — RAII done once, used everywhere.
//!
//! Constructors take ownership of a freshly-returned, valid HANDLE and
//! close it exactly once on drop. No `Copy`, no shared-close: the v1
//! scaffold's close-through-shared-reference bug (B-4) is unrepresentable
//! against this type.

#![allow(unsafe_code)]

use crate::ffi::win32_surface as w;

/// An owned Win32 HANDLE, closed on drop.
#[derive(Debug)]
pub struct OwnedWinHandle(w::HANDLE);

impl OwnedWinHandle {
    /// Take ownership of `handle`.
    ///
    /// # Safety contract (checked by the caller)
    /// `handle` must be a valid handle returned by a Win32 creation call,
    /// not `INVALID_HANDLE_VALUE`, and not owned elsewhere.
    pub fn from_raw(handle: w::HANDLE) -> Option<Self> {
        (handle != w::INVALID_HANDLE_VALUE && !handle.is_null()).then_some(Self(handle))
    }

    pub fn as_raw(&self) -> w::HANDLE {
        self.0
    }
}

impl Drop for OwnedWinHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` was validated non-invalid at construction, is
        // owned uniquely by this value, and is closed exactly once here.
        unsafe {
            w::CloseHandle(self.0);
        }
    }
}
