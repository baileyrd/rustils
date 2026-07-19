//! `Dir`/`File` trait impls over the sys layer — the Linux backend's
//! mirror, with `NtCreateFile` handle-relative opens standing in for the
//! `openat` family (RFC v2 §5.3).

use std::ffi::OsStr;
use std::os::windows::io::{AsHandle, BorrowedHandle, OwnedHandle};
use std::path::Path;

use platform::error::{ErrorKind, OsCode, PlatformError, Result};
use platform::fs::{Dir, DirEntry, File, Metadata, OpenOptions};

use crate::ffi::nt_surface as nt;
use crate::ffi::win32_surface as w;
use crate::sys::handle::OwnedWinHandle;
use crate::sys::{fileio, nt as ntsys};

/// A directory capability backed by a directory HANDLE. All operations are
/// handle-relative (`NtCreateFile` with `RootDirectory`) — the ambient
/// process namespace is consulted only by [`WindowsDir::open_ambient`]
/// (RFC v2 §5.3).
pub struct WindowsDir {
    handle: OwnedWinHandle,
}

impl WindowsDir {
    /// Open an absolute path as the root capability. This is the only
    /// place an absolute path enters the backend; everything after is
    /// relative to a capability.
    pub fn open_ambient(path: &Path) -> Result<Self> {
        let handle = fileio::open_ambient_dir(path.as_os_str())?;
        Ok(Self { handle })
    }
}

/// An open file backed by an [`OwnedWinHandle`]. Public for std interop
/// (RFC v2 §5.1); the [`Dir`] trait still hands out `Box<dyn File>`.
pub struct WindowsFile {
    handle: OwnedWinHandle,
}

// std interop (RFC v2 §5.1): delegation to OwnedWinHandle's conversions —
// handle ownership moves, no raw-handle juggling at this layer.

impl AsHandle for WindowsDir {
    fn as_handle(&self) -> BorrowedHandle<'_> {
        self.handle.as_handle()
    }
}

impl From<WindowsDir> for OwnedHandle {
    fn from(dir: WindowsDir) -> OwnedHandle {
        OwnedHandle::from(dir.handle)
    }
}

impl AsHandle for WindowsFile {
    fn as_handle(&self) -> BorrowedHandle<'_> {
        self.handle.as_handle()
    }
}

impl From<WindowsFile> for std::fs::File {
    fn from(file: WindowsFile) -> std::fs::File {
        std::fs::File::from(OwnedHandle::from(file.handle))
    }
}

impl From<std::fs::File> for WindowsFile {
    fn from(file: std::fs::File) -> Self {
        Self {
            handle: OwnedWinHandle::from(OwnedHandle::from(file)),
        }
    }
}

/// Any readable/writable handle works as a [`WindowsFile`] — pipe ends
/// included (the process backend hands captured-stdio ends out this way).
impl From<OwnedWinHandle> for WindowsFile {
    fn from(handle: OwnedWinHandle) -> Self {
        Self { handle }
    }
}

impl File for WindowsFile {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        fileio::read(&self.handle, buf)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        fileio::write(&self.handle, buf)
    }

    fn flush(&mut self) -> Result<()> {
        // Synchronous WriteFile has no userspace buffer to flush;
        // durability is the distinct, explicit sync_all below —
        // mirroring the Linux backend.
        Ok(())
    }

    fn sync_all(&mut self) -> Result<()> {
        fileio::sync_all(&self.handle)
    }
}

/// (access, disposition) for a file open — the Windows analog of the Linux
/// backend's `open_flags`.
fn open_params(opts: &OpenOptions) -> Result<(u32, u32)> {
    let mut access = w::SYNCHRONIZE;
    if opts.read {
        access |= w::FILE_GENERIC_READ;
    }
    if opts.append {
        // Append-only: grant everything generic write carries except
        // arbitrary-position writes — the OS then atomically appends.
        access |= (w::FILE_GENERIC_WRITE & !w::FILE_WRITE_DATA) | w::FILE_APPEND_DATA;
    } else if opts.write {
        access |= w::FILE_GENERIC_WRITE;
    }
    if !opts.read && !opts.write && !opts.append {
        return Err(PlatformError::new(
            ErrorKind::InvalidInput,
            OsCode::None,
            "open",
        ));
    }
    let disposition = match (opts.create_new, opts.create, opts.truncate) {
        (true, _, _) => nt::FILE_CREATE,
        (false, true, true) => nt::FILE_OVERWRITE_IF,
        (false, true, false) => nt::FILE_OPEN_IF,
        (false, false, true) => nt::FILE_OVERWRITE,
        (false, false, false) => nt::FILE_OPEN,
    };
    Ok((access, disposition))
}

impl Dir for WindowsDir {
    fn open(&self, rel: &OsStr, opts: &OpenOptions) -> Result<Box<dyn File>> {
        let (access, disposition) = open_params(opts)?;
        let handle = ntsys::open_relative(
            &self.handle,
            rel,
            access,
            disposition,
            nt::FILE_NON_DIRECTORY_FILE | nt::FILE_SYNCHRONOUS_IO_NONALERT,
        )?;
        Ok(Box::new(WindowsFile { handle }))
    }

    fn open_dir(&self, rel: &OsStr) -> Result<Box<dyn Dir>> {
        let handle = ntsys::open_relative(
            &self.handle,
            rel,
            w::FILE_LIST_DIRECTORY | w::FILE_READ_ATTRIBUTES | w::FILE_TRAVERSE | w::SYNCHRONIZE,
            nt::FILE_OPEN,
            nt::FILE_DIRECTORY_FILE | nt::FILE_SYNCHRONOUS_IO_NONALERT,
        )?;
        Ok(Box::new(WindowsDir { handle }))
    }

    fn create_dir(&self, rel: &OsStr) -> Result<()> {
        // The returned handle is dropped immediately: creation is the
        // operation, the capability is not retained.
        ntsys::open_relative(
            &self.handle,
            rel,
            w::FILE_LIST_DIRECTORY | w::SYNCHRONIZE,
            nt::FILE_CREATE,
            nt::FILE_DIRECTORY_FILE | nt::FILE_SYNCHRONOUS_IO_NONALERT,
        )?;
        Ok(())
    }

    fn metadata(&self, rel: &OsStr) -> Result<Metadata> {
        // Attributes-only open; FILE_OPEN_REPARSE_POINT mirrors the Linux
        // backend's AT_SYMLINK_NOFOLLOW (report the link, don't follow it).
        let handle = ntsys::open_relative(
            &self.handle,
            rel,
            w::FILE_READ_ATTRIBUTES | w::SYNCHRONIZE,
            nt::FILE_OPEN,
            nt::FILE_SYNCHRONOUS_IO_NONALERT | nt::FILE_OPEN_REPARSE_POINT,
        )?;
        let (file_type, len) = fileio::metadata_by_handle(&handle, rel)?;
        Ok(Metadata { file_type, len })
    }

    fn read_dir(&self) -> Result<Vec<DirEntry>> {
        // A fresh handle for enumeration: directory-enumeration state is
        // per-handle, and this capability's own handle must stay pristine
        // for further operations (the Linux backend re-opens "." for the
        // same reason). An empty relative name re-opens this directory.
        let handle = ntsys::open_relative(
            &self.handle,
            OsStr::new(""),
            w::FILE_LIST_DIRECTORY | w::SYNCHRONIZE,
            nt::FILE_OPEN,
            nt::FILE_DIRECTORY_FILE | nt::FILE_SYNCHRONOUS_IO_NONALERT,
        )?;
        Ok(fileio::read_dir_entries(&handle)?
            .into_iter()
            .map(|(name, file_type)| DirEntry { name, file_type })
            .collect())
    }

    fn remove_file(&self, rel: &OsStr) -> Result<()> {
        let handle = ntsys::open_relative(
            &self.handle,
            rel,
            w::DELETE | w::SYNCHRONIZE,
            nt::FILE_OPEN,
            nt::FILE_NON_DIRECTORY_FILE
                | nt::FILE_SYNCHRONOUS_IO_NONALERT
                | nt::FILE_OPEN_REPARSE_POINT,
        )?;
        fileio::mark_delete(&handle, rel)
        // Deletion completes when `handle` drops (delete-on-close).
    }

    fn remove_dir(&self, rel: &OsStr) -> Result<()> {
        let handle = ntsys::open_relative(
            &self.handle,
            rel,
            w::DELETE | w::SYNCHRONIZE,
            nt::FILE_OPEN,
            nt::FILE_DIRECTORY_FILE
                | nt::FILE_SYNCHRONOUS_IO_NONALERT
                | nt::FILE_OPEN_REPARSE_POINT,
        )?;
        // A non-empty directory is refused here by the OS itself
        // (ERROR_DIR_NOT_EMPTY → DirectoryNotEmpty).
        fileio::mark_delete(&handle, rel)
    }

    fn rename(&self, from: &OsStr, to: &OsStr) -> Result<()> {
        fileio::rename(&self.handle, from, to, true)
    }

    fn rename_no_replace(&self, from: &OsStr, to: &OsStr) -> Result<()> {
        fileio::rename(&self.handle, from, to, false)
    }
}
