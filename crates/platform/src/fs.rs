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
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{ErrorKind, OsCode, PlatformError, Result};

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
        Self {
            read: true,
            ..Self::default()
        }
    }

    pub fn create_truncate() -> Self {
        Self {
            write: true,
            create: true,
            truncate: true,
            ..Self::default()
        }
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

    /// Durability: block until this file's writes are on stable storage
    /// (`fsync`/`FlushFileBuffers`), not merely past the OS page/write
    /// cache. `flush` above is not this — a synchronous `write` already
    /// has no userspace buffer to flush; this is the distinct, explicit
    /// operation `flush`'s doc comment always said would come when a
    /// consumer needed it. [`Dir::write_atomic`] (D11, convergence
    /// roadmap Phase 3) is that consumer.
    fn sync_all(&mut self) -> Result<()>;
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

    /// Rename `from` to `to`, both relative to this directory,
    /// **replacing** `to` if it already exists (POSIX `rename(2)` /
    /// `renameat2` with no flags; Windows `FILE_RENAME_INFO` with
    /// `ReplaceIfExists`). Atomic: `to` is never observably absent —
    /// concurrent readers see either the old file or the new one.
    fn rename(&self, from: &OsStr, to: &OsStr) -> Result<()>;

    /// Rename `from` to `to`, refusing (`AlreadyExists`) if `to` already
    /// exists, instead of replacing it (`RENAME_NOREPLACE` /
    /// `ReplaceIfExists = false`) — the check-and-rename happens
    /// atomically in the kernel, so this is race-free where a
    /// stat-then-rename from the consumer would not be.
    fn rename_no_replace(&self, from: &OsStr, to: &OsStr) -> Result<()>;

    /// Durably write `contents` to `rel`, atomically: never leaves a
    /// partially-written or missing file observable at that name, even
    /// across a crash between the write and the rename that publishes
    /// it (D11, convergence roadmap Phase 3 — the pattern independently
    /// present in nexus's `storage/atomic.rs` and rusty_naner's staged
    /// install). Default-provided: it composes [`open`](Dir::open) +
    /// [`File::write`] + [`File::sync_all`] + [`rename`](Dir::rename),
    /// so every backend gets it for free and there is exactly one
    /// implementation to trust.
    ///
    /// Sequence: write into a same-directory temp name (guaranteeing
    /// the final rename is same-filesystem, hence atomic) → `sync_all`
    /// the temp file (durability *before* the rename, not after — a
    /// crash before this point leaves only the temp name, never a
    /// half-written `rel`) → close it → `rename` over `rel`. The temp
    /// file is best-effort removed if the write/sync step fails.
    fn write_atomic(&self, rel: &OsStr, contents: &[u8]) -> Result<()> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut tmp_name = rel.to_os_string();
        tmp_name.push(format!(".rustils-tmp-{}-{n:x}", std::process::id()));

        let write_and_sync = || -> Result<()> {
            let mut f = self.open(
                &tmp_name,
                &OpenOptions {
                    write: true,
                    create_new: true,
                    ..OpenOptions::default()
                },
            )?;
            let mut off = 0usize;
            while off < contents.len() {
                let n = f.write(&contents[off..])?;
                if n == 0 {
                    return Err(
                        PlatformError::new(ErrorKind::Other, OsCode::None, "write_atomic")
                            .with_path(tmp_name.as_os_str()),
                    );
                }
                off += n;
            }
            f.sync_all()
        };

        if let Err(e) = write_and_sync() {
            let _ = self.remove_file(&tmp_name);
            return Err(e);
        }
        self.rename(&tmp_name, rel)
    }
}
