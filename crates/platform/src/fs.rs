//! Capability-style filesystem traits (RFC v2 §5.3, decision D-6).
//!
//! There are deliberately no global path functions here. All operations are
//! relative to a [`Dir`] handle opened once — mapping to the `openat` family
//! on Linux and handle-relative opens on Windows. This shape exists for
//! three reasons (RFC v2 §5.3): TOCTOU hygiene, direct support for a
//! consumer-maintained virtual cwd (rush subshells), and because handle-
//! relative NT opens are among the most instructive Windows topics for the
//! project's understanding mandate (M1).
//!
//! Paths and names are `OsStr`/`OsString` — never `str` (RFC v2 §5.2):
//! unix names are bytes, and a `str` surface makes correct behavior on
//! non-UTF-8 names unrepresentable.

use std::ffi::{OsStr, OsString};

use crate::error::Result;

/// Options for opening or creating a file relative to a [`Dir`].
///
/// Mirrors the intersection of `openat` flags and `CreateFileW` dispositions
/// that both backends can honor; per-OS extensions live on the backend
/// types, not here.
#[derive(Debug, Clone, Default)]
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    pub append: bool,
    pub create: bool,
    pub create_new: bool,
    pub truncate: bool,
}

impl OpenOptions {
    pub fn read() -> Self {
        Self { read: true, ..Self::default() }
    }

    pub fn create_truncate() -> Self {
        Self { write: true, create: true, truncate: true, ..Self::default() }
    }
}

/// Metadata for a filesystem entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub file_type: FileType,
    pub len: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FileType {
    File,
    Dir,
    Symlink,
    Other,
}

/// A directory entry yielded by [`Dir::read_dir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// Entry name — a single component, not a path.
    pub name: OsString,
    pub file_type: FileType,
}

/// An open file. Object-safe; backends return `Box<dyn File>`.
pub trait File {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    fn write(&mut self, buf: &[u8]) -> Result<usize>;
    fn flush(&mut self) -> Result<()>;
}

/// An open directory: the capability all filesystem operations flow through.
///
/// Object-safe. `&self` receivers throughout: a `Dir` is a capability that
/// may be shared, and backends manage any interior synchronization their OS
/// primitives require.
pub trait Dir {
    /// Open a file at `rel` (a relative path) under this directory.
    fn open(&self, rel: &OsStr, opts: &OpenOptions) -> Result<Box<dyn File>>;

    /// Open a subdirectory as a new capability.
    fn open_dir(&self, rel: &OsStr) -> Result<Box<dyn Dir>>;

    /// Create a subdirectory.
    fn create_dir(&self, rel: &OsStr) -> Result<()>;

    /// Metadata for the entry at `rel`.
    fn metadata(&self, rel: &OsStr) -> Result<Metadata>;

    /// List this directory's entries.
    ///
    /// Order is unspecified and differs across backends; consumers that
    /// need determinism sort. (Pinned by behavior spec `docs/behavior/fs.md`.)
    fn read_dir(&self) -> Result<Vec<DirEntry>>;

    /// Remove the file at `rel`.
    fn remove_file(&self, rel: &OsStr) -> Result<()>;

    /// Remove the (empty) directory at `rel`.
    fn remove_dir(&self, rel: &OsStr) -> Result<()>;
}
