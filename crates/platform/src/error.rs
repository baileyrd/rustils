//! Two-axis error model (RFC v2 §5.5, decision D-8).
//!
//! Every error carries (a) a portable [`ErrorKind`] a caller can match on,
//! and (b) the raw OS code in its own number space via [`OsCode`] — never a
//! bare integer that conflates `errno` with `GetLastError`. Operation and
//! path context ride along so an error is diagnosable without a debugger.

use std::path::PathBuf;

/// Portable classification of a platform error.
///
/// Backends map their OS's native codes into this taxonomy; the mapping
/// tables are parity-tested (RFC v2 §9). `Other` is the escape hatch for
/// codes not yet classified — matching on it should prompt extending the
/// taxonomy, not shipping around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    NotFound,
    PermissionDenied,
    AlreadyExists,
    NotADirectory,
    IsADirectory,
    DirectoryNotEmpty,
    InvalidInput,
    WouldBlock,
    Interrupted,
    BrokenPipe,
    Unsupported,
    Other,
}

/// The raw OS error in its own number space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OsCode {
    /// Unix `errno`.
    Errno(i32),
    /// Windows `GetLastError` / NTSTATUS-derived Win32 code.
    Win32(u32),
    /// No OS code applies (e.g. an error synthesized by `platform-mock`).
    None,
}

/// A platform operation failure with full context.
#[derive(Debug, thiserror::Error)]
#[error("{op} failed{}: {kind:?} ({os:?})", path_display(.path))]
pub struct PlatformError {
    pub kind: ErrorKind,
    pub os: OsCode,
    /// The operation that failed, e.g. `"openat"`, `"CreateProcessW"`.
    pub op: &'static str,
    /// The path involved, when one was.
    pub path: Option<PathBuf>,
}

fn path_display(path: &Option<PathBuf>) -> String {
    match path {
        Some(p) => format!(" on {}", p.display()),
        None => String::new(),
    }
}

impl PlatformError {
    /// Construct an error with no path context.
    pub fn new(kind: ErrorKind, os: OsCode, op: &'static str) -> Self {
        Self {
            kind,
            os,
            op,
            path: None,
        }
    }

    /// Attach path context.
    #[must_use]
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }
}

/// Convenience alias used across the workspace.
pub type Result<T> = std::result::Result<T, PlatformError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_carries_context() {
        let e = PlatformError::new(ErrorKind::NotFound, OsCode::Errno(2), "openat")
            .with_path("/tmp/missing");
        let s = e.to_string();
        assert!(s.contains("openat"), "operation missing from: {s}");
        assert!(s.contains("/tmp/missing"), "path missing from: {s}");
        assert!(s.contains("NotFound"), "kind missing from: {s}");
    }

    #[test]
    fn os_code_spaces_do_not_conflate() {
        // The type system is the test: these are different variants, not
        // the same bare u32. This test exists to pin the regression the
        // v1 scaffold shipped (IoError(u32) mixing errno and Win32 spaces).
        assert_ne!(OsCode::Errno(5), OsCode::Win32(5));
    }
}
