//! `Dir`/`File` trait impls over the sys layer. No `unsafe` here.

use std::ffi::{OsStr, OsString};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::path::Path;

use platform::error::{ErrorKind, OsCode, PlatformError, Result};
use platform::fs::{Dir, DirEntry, File, Metadata, OpenOptions};

use crate::ffi::libc_surface as c;
use crate::sys::fdio;

/// A directory capability backed by an `O_DIRECTORY` file descriptor.
/// All operations are dirfd-relative (`openat` family) — the ambient cwd
/// is never consulted (RFC v2 §5.3).
pub struct LinuxDir {
    fd: OwnedFd,
}

impl LinuxDir {
    /// Open an absolute path as the root capability. This is the only
    /// place an absolute path enters the backend; everything after is
    /// relative to a capability.
    pub fn open_ambient(path: &Path) -> Result<Self> {
        let fd = fdio::openat(
            c::AT_FDCWD,
            path.as_os_str(),
            c::O_RDONLY | c::O_DIRECTORY,
            0,
        )?;
        Ok(Self { fd })
    }
}

/// An open file backed by an `OwnedFd`. Public for std interop (RFC v2
/// §5.1); the [`Dir`] trait still hands out `Box<dyn File>`.
pub struct LinuxFile {
    fd: OwnedFd,
}

// std interop (RFC v2 §5.1): handle types are adoptable incrementally,
// not a total buy-in island. All conversions are safe fd-ownership moves.

impl AsFd for LinuxDir {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

impl From<LinuxDir> for OwnedFd {
    fn from(dir: LinuxDir) -> OwnedFd {
        dir.fd
    }
}

/// The fd must reference a directory; operations on a capability built
/// from a non-directory fd fail with `NotADirectory` at call time.
impl From<OwnedFd> for LinuxDir {
    fn from(fd: OwnedFd) -> Self {
        Self { fd }
    }
}

impl AsFd for LinuxFile {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

impl From<LinuxFile> for std::fs::File {
    fn from(file: LinuxFile) -> std::fs::File {
        std::fs::File::from(file.fd)
    }
}

impl From<std::fs::File> for LinuxFile {
    fn from(file: std::fs::File) -> Self {
        Self {
            fd: OwnedFd::from(file),
        }
    }
}

/// Any readable/writable fd works as a [`LinuxFile`] — pipe ends included
/// (the process backend hands captured-stdio ends out this way).
impl From<OwnedFd> for LinuxFile {
    fn from(fd: OwnedFd) -> Self {
        Self { fd }
    }
}

impl File for LinuxFile {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        fdio::read(&self.fd, buf)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        fdio::write(&self.fd, buf)
    }

    fn flush(&mut self) -> Result<()> {
        // write(2) has no userspace buffer to flush; durability (fsync)
        // is the distinct, explicit sync_all below.
        Ok(())
    }

    fn sync_all(&mut self) -> Result<()> {
        fdio::fsync(&self.fd)
    }
}

fn open_flags(opts: &OpenOptions) -> Result<i32> {
    let mut flags = match (opts.read, opts.write || opts.append) {
        (true, true) => c::O_RDWR,
        (true, false) => c::O_RDONLY,
        (false, true) => c::O_WRONLY,
        (false, false) => {
            return Err(PlatformError::new(
                ErrorKind::InvalidInput,
                OsCode::None,
                "open",
            ))
        }
    };
    if opts.append {
        flags |= c::O_APPEND;
    }
    if opts.create_new {
        flags |= c::O_CREAT | c::O_EXCL;
    } else if opts.create {
        flags |= c::O_CREAT;
    }
    if opts.truncate {
        flags |= c::O_TRUNC;
    }
    Ok(flags)
}

impl Dir for LinuxDir {
    fn open(&self, rel: &OsStr, opts: &OpenOptions) -> Result<Box<dyn File>> {
        let flags = open_flags(opts)?;
        let fd = fdio::openat(self.fd.as_raw_fd(), rel, flags, 0o666)?;
        Ok(Box::new(LinuxFile { fd }))
    }

    fn open_dir(&self, rel: &OsStr) -> Result<Box<dyn Dir>> {
        let fd = fdio::openat(self.fd.as_raw_fd(), rel, c::O_RDONLY | c::O_DIRECTORY, 0)?;
        Ok(Box::new(LinuxDir { fd }))
    }

    fn create_dir(&self, rel: &OsStr) -> Result<()> {
        fdio::mkdirat(self.fd.as_raw_fd(), rel)
    }

    fn metadata(&self, rel: &OsStr) -> Result<Metadata> {
        let (file_type, len) = fdio::statat(self.fd.as_raw_fd(), rel)?;
        Ok(Metadata { file_type, len })
    }

    fn read_dir(&self) -> Result<Vec<DirEntry>> {
        // A fresh fd for enumeration: fdopendir consumes its fd, and this
        // capability's own fd must stay valid for further operations.
        let fd = fdio::openat(
            self.fd.as_raw_fd(),
            OsStr::new("."),
            c::O_RDONLY | c::O_DIRECTORY,
            0,
        )?;
        Ok(fdio::read_dir(fd)?
            .into_iter()
            .map(|(name, file_type)| DirEntry { name, file_type })
            .collect())
    }

    fn remove_file(&self, rel: &OsStr) -> Result<()> {
        fdio::unlinkat(self.fd.as_raw_fd(), rel, false)
    }

    fn remove_dir(&self, rel: &OsStr) -> Result<()> {
        fdio::unlinkat(self.fd.as_raw_fd(), rel, true)
    }

    fn symlink(&self, target: &OsStr, link_name: &OsStr) -> Result<()> {
        fdio::symlink(self.fd.as_raw_fd(), target, link_name)
    }

    fn read_link(&self, rel: &OsStr) -> Result<OsString> {
        fdio::read_link(self.fd.as_raw_fd(), rel)
    }

    fn rename(&self, from: &OsStr, to: &OsStr) -> Result<()> {
        fdio::rename(self.fd.as_raw_fd(), from, to)
    }

    fn rename_no_replace(&self, from: &OsStr, to: &OsStr) -> Result<()> {
        fdio::rename_no_replace(self.fd.as_raw_fd(), from, to)
    }
}
