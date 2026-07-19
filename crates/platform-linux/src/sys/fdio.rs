//! Owned-fd primitives: openat-family operations, read/write, directory
//! enumeration. The building blocks the `Dir`/`File` impls compose.

#![allow(unsafe_code)] // the one place in this crate it is permitted

use std::ffi::{CStr, CString, OsStr, OsString};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};

use platform::error::{ErrorKind, OsCode, PlatformError, Result};
use platform::fs::FileType;

use crate::ffi::libc_surface as c;

fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// Track P error path: a raw syscall's failure comes back as the `Errno`
/// value in the return register — the thread-local `errno` that `os_err`
/// reads is never touched. The code must flow from the returned value.
#[cfg(feature = "track-p")]
fn trackp_err(op: &'static str, e: rusty_libc::Errno) -> PlatformError {
    PlatformError::new(kind_of(e.0), OsCode::Errno(e.0), op)
}

fn kind_of(errno: i32) -> ErrorKind {
    match errno {
        libc::ENOENT => ErrorKind::NotFound,
        libc::EACCES | libc::EPERM => ErrorKind::PermissionDenied,
        libc::EEXIST => ErrorKind::AlreadyExists,
        libc::ENOTDIR => ErrorKind::NotADirectory,
        libc::EISDIR => ErrorKind::IsADirectory,
        libc::ENOTEMPTY => ErrorKind::DirectoryNotEmpty,
        libc::EINVAL => ErrorKind::InvalidInput,
        libc::EAGAIN => ErrorKind::WouldBlock,
        libc::EINTR => ErrorKind::Interrupted,
        libc::EPIPE => ErrorKind::BrokenPipe,
        _ => ErrorKind::Other,
    }
}

fn os_err(op: &'static str, path: &OsStr) -> PlatformError {
    let e = errno();
    PlatformError::new(kind_of(e), OsCode::Errno(e), op).with_path(path)
}

fn to_cstring(path: &OsStr, op: &'static str) -> Result<CString> {
    CString::new(path.as_bytes())
        .map_err(|_| PlatformError::new(ErrorKind::InvalidInput, OsCode::None, op).with_path(path))
}

/// `openat(dirfd, rel, flags, mode)` returning an owned fd.
pub fn openat(dirfd: RawFd, rel: &OsStr, flags: i32, mode: u32) -> Result<OwnedFd> {
    let c_rel = to_cstring(rel, "openat")?;
    // SAFETY: `c_rel` is a valid NUL-terminated string that outlives the
    // call; `dirfd` is a valid open descriptor owned by our caller; openat
    // has no other pointer parameters.
    let fd = unsafe { c::openat(dirfd, c_rel.as_ptr(), flags | c::O_CLOEXEC, mode) };
    if fd < 0 {
        return Err(os_err("openat", rel));
    }
    // SAFETY: `fd` is a freshly returned, valid, otherwise-unowned
    // descriptor; wrapping it exactly once transfers ownership.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// `read(2)` into `buf`.
#[cfg(not(feature = "track-p"))]
pub fn read(fd: &OwnedFd, buf: &mut [u8]) -> Result<usize> {
    // SAFETY: `buf` is a valid, writable region of exactly `buf.len()`
    // bytes for the duration of the call; `fd` is a valid open descriptor.
    let n = unsafe { c::read(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
    if n < 0 {
        return Err(os_err("read", OsStr::new("")));
    }
    Ok(n as usize)
}

/// `read(2)` into `buf` — Track P: raw `SYS_read`, no libc in the path.
/// rusty_libc's safe wrapper derives the pointer/length pair from the slice
/// itself, so no unsafe block appears at this call site.
#[cfg(feature = "track-p")]
pub fn read(fd: &OwnedFd, buf: &mut [u8]) -> Result<usize> {
    rusty_libc::fd::read(fd.as_raw_fd(), buf).map_err(|e| trackp_err("read", e))
}

/// `write(2)` from `buf`.
#[cfg(not(feature = "track-p"))]
pub fn write(fd: &OwnedFd, buf: &[u8]) -> Result<usize> {
    // SAFETY: `buf` is a valid readable region of exactly `buf.len()`
    // bytes for the duration of the call; `fd` is a valid open descriptor.
    let n = unsafe { c::write(fd.as_raw_fd(), buf.as_ptr().cast(), buf.len()) };
    if n < 0 {
        return Err(os_err("write", OsStr::new("")));
    }
    Ok(n as usize)
}

/// `write(2)` from `buf` — Track P: raw `SYS_write`, no libc in the path.
#[cfg(feature = "track-p")]
pub fn write(fd: &OwnedFd, buf: &[u8]) -> Result<usize> {
    rusty_libc::fd::write(fd.as_raw_fd(), buf).map_err(|e| trackp_err("write", e))
}

/// `mkdirat(dirfd, rel, 0o777)` (mode filtered by umask, as usual).
pub fn mkdirat(dirfd: RawFd, rel: &OsStr) -> Result<()> {
    let c_rel = to_cstring(rel, "mkdirat")?;
    // SAFETY: valid NUL-terminated path outliving the call; valid dirfd.
    let r = unsafe { c::mkdirat(dirfd, c_rel.as_ptr(), 0o777) };
    if r < 0 {
        return Err(os_err("mkdirat", rel));
    }
    Ok(())
}

/// `unlinkat(dirfd, rel, flags)`.
pub fn unlinkat(dirfd: RawFd, rel: &OsStr, remove_dir: bool) -> Result<()> {
    let c_rel = to_cstring(rel, "unlinkat")?;
    let flags = if remove_dir { c::AT_REMOVEDIR } else { 0 };
    // SAFETY: valid NUL-terminated path outliving the call; valid dirfd.
    let r = unsafe { c::unlinkat(dirfd, c_rel.as_ptr(), flags) };
    if r < 0 {
        return Err(os_err("unlinkat", rel));
    }
    Ok(())
}

/// `fstatat` returning (file type, size).
pub fn statat(dirfd: RawFd, rel: &OsStr) -> Result<(FileType, u64)> {
    let c_rel = to_cstring(rel, "fstatat")?;
    // SAFETY: `stat` is a plain-old-data struct for which the all-zeroes
    // bit pattern is a valid (if meaningless) value; the kernel overwrites
    // it on success.
    let mut st: c::stat = unsafe { std::mem::zeroed() };
    // SAFETY: valid path pointer and out-pointer to a properly sized
    // `stat` struct, both outliving the call; valid dirfd.
    let r = unsafe { c::fstatat(dirfd, c_rel.as_ptr(), &mut st, c::AT_SYMLINK_NOFOLLOW) };
    if r < 0 {
        return Err(os_err("fstatat", rel));
    }
    let ft = match st.st_mode & c::S_IFMT {
        m if m == c::S_IFREG => FileType::File,
        m if m == c::S_IFDIR => FileType::Dir,
        m if m == c::S_IFLNK => FileType::Symlink,
        _ => FileType::Other,
    };
    Ok((ft, st.st_size as u64))
}

/// Enumerate a directory via `fdopendir`/`readdir`, consuming an fd opened
/// with `O_DIRECTORY`. Returns (name, file type) pairs, excluding `.`/`..`.
pub fn read_dir(dirfd: OwnedFd) -> Result<Vec<(OsString, FileType)>> {
    use std::os::fd::IntoRawFd;
    // SAFETY: `into_raw_fd` transfers ownership of a valid directory fd to
    // fdopendir, which takes ownership on success; on failure we must (and
    // do) reconstruct and drop the fd to avoid a leak.
    let dir: *mut c::DIR = unsafe { c::fdopendir(dirfd.as_raw_fd()) };
    if dir.is_null() {
        return Err(os_err("fdopendir", OsStr::new("")));
    }
    // fdopendir now owns the fd; into_raw_fd relinquishes our ownership so
    // Drop will not close it.
    let _ = dirfd.into_raw_fd();

    let mut out = Vec::new();
    loop {
        // SAFETY: `dir` is the valid stream returned above and is only
        // used on this thread; readdir's returned pointer is valid until
        // the next readdir on the same stream, and we copy out of it
        // before looping.
        let ent: *const c::dirent = unsafe { c::readdir(dir) };
        if ent.is_null() {
            break;
        }
        // SAFETY: non-null `ent` points at a valid dirent whose d_name is
        // NUL-terminated per POSIX.
        let (name_bytes, d_type) = unsafe {
            let name = CStr::from_ptr((*ent).d_name.as_ptr()).to_bytes().to_vec();
            (name, (*ent).d_type)
        };
        if name_bytes == b"." || name_bytes == b".." {
            continue;
        }
        let ft = match d_type {
            t if t == c::DT_REG => FileType::File,
            t if t == c::DT_DIR => FileType::Dir,
            t if t == c::DT_LNK => FileType::Symlink,
            _ => FileType::Other,
        };
        out.push((OsString::from_vec(name_bytes), ft));
    }
    // SAFETY: `dir` is a valid open stream not used after this call.
    unsafe { libc::closedir(dir) };
    Ok(out)
}
