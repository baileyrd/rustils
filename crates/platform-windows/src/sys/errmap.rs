//! Win32 / NTSTATUS → `PlatformError` mapping (RFC v2 §5.5).
//!
//! Two native number spaces feed this backend: `GetLastError` codes from
//! Win32 calls and `NTSTATUS` from `NtCreateFile`. Classification into the
//! portable [`ErrorKind`] happens on the code actually returned; an
//! NTSTATUS is additionally converted with `RtlNtStatusToDosError` so the
//! recorded [`OsCode::Win32`] is always in the one documented Win32 space
//! rather than leaking raw NTSTATUS values into diagnostics.

#![allow(unsafe_code)]

use std::ffi::OsStr;

use platform::error::{ErrorKind, OsCode, PlatformError};

use crate::ffi::win32_surface as w;

fn kind_of_win32(code: u32) -> ErrorKind {
    match code {
        w::ERROR_FILE_NOT_FOUND | w::ERROR_PATH_NOT_FOUND => ErrorKind::NotFound,
        w::ERROR_ACCESS_DENIED | w::ERROR_SHARING_VIOLATION => ErrorKind::PermissionDenied,
        w::ERROR_FILE_EXISTS | w::ERROR_ALREADY_EXISTS => ErrorKind::AlreadyExists,
        w::ERROR_DIR_NOT_EMPTY => ErrorKind::DirectoryNotEmpty,
        w::ERROR_DIRECTORY => ErrorKind::NotADirectory,
        w::ERROR_INVALID_PARAMETER => ErrorKind::InvalidInput,
        _ => ErrorKind::Other,
    }
}

fn kind_of_ntstatus(status: w::NTSTATUS) -> ErrorKind {
    match status {
        w::STATUS_OBJECT_NAME_NOT_FOUND | w::STATUS_OBJECT_PATH_NOT_FOUND => ErrorKind::NotFound,
        w::STATUS_ACCESS_DENIED | w::STATUS_SHARING_VIOLATION | w::STATUS_DELETE_PENDING => {
            ErrorKind::PermissionDenied
        }
        w::STATUS_OBJECT_NAME_COLLISION => ErrorKind::AlreadyExists,
        w::STATUS_FILE_IS_A_DIRECTORY => ErrorKind::IsADirectory,
        w::STATUS_NOT_A_DIRECTORY => ErrorKind::NotADirectory,
        w::STATUS_DIRECTORY_NOT_EMPTY => ErrorKind::DirectoryNotEmpty,
        w::STATUS_OBJECT_NAME_INVALID => ErrorKind::InvalidInput,
        _ => ErrorKind::Other,
    }
}

/// Error from the calling thread's last Win32 error code.
pub fn last_win32_err(op: &'static str, path: &OsStr) -> PlatformError {
    // SAFETY: `GetLastError` takes no arguments and has no preconditions.
    let code = unsafe { w::GetLastError() };
    let e = PlatformError::new(kind_of_win32(code), OsCode::Win32(code), op);
    if path.is_empty() {
        e
    } else {
        e.with_path(path)
    }
}

/// Error from a failed `NtCreateFile`-family NTSTATUS.
pub fn nt_err(status: w::NTSTATUS, op: &'static str, path: &OsStr) -> PlatformError {
    // SAFETY: `RtlNtStatusToDosError` is a pure translation function with
    // no preconditions on its argument.
    let code = unsafe { w::RtlNtStatusToDosError(status) };
    let e = PlatformError::new(kind_of_ntstatus(status), OsCode::Win32(code), op);
    if path.is_empty() {
        e
    } else {
        e.with_path(path)
    }
}
