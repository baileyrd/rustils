//! Owned HANDLE wrapper — RAII done once, used everywhere.
//!
//! Constructors take ownership of a freshly-returned, valid HANDLE and
//! close it exactly once on drop. No `Copy`, no shared-close: the v1
//! scaffold's close-through-shared-reference bug (B-4) is unrepresentable
//! against this type.

#![allow(unsafe_code)]

use std::os::windows::io::{AsHandle, BorrowedHandle, FromRawHandle, IntoRawHandle, OwnedHandle};

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

    /// Relinquish ownership: the caller (or the type it hands the raw
    /// value to) becomes responsible for the single close.
    pub fn into_raw(self) -> w::HANDLE {
        let handle = self.0;
        std::mem::forget(self);
        handle
    }
}

// std interop (RFC v2 §5.1): conversions to and from `std::os::windows`
// handle types, so this layer is adoptable incrementally.

impl AsHandle for OwnedWinHandle {
    fn as_handle(&self) -> BorrowedHandle<'_> {
        // SAFETY: `self.0` is a valid open handle for as long as `self`
        // lives, which bounds the returned borrow's lifetime.
        unsafe { BorrowedHandle::borrow_raw(self.0) }
    }
}

impl From<OwnedWinHandle> for OwnedHandle {
    fn from(handle: OwnedWinHandle) -> OwnedHandle {
        // SAFETY: `into_raw` transfers sole ownership of a valid,
        // currently-open handle; the std type now closes it exactly once.
        unsafe { OwnedHandle::from_raw_handle(handle.into_raw()) }
    }
}

impl From<OwnedHandle> for OwnedWinHandle {
    fn from(handle: OwnedHandle) -> OwnedWinHandle {
        // `OwnedHandle` guarantees a valid open handle, satisfying this
        // type's construction contract; `into_raw_handle` transfers sole
        // ownership.
        Self(handle.into_raw_handle())
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
