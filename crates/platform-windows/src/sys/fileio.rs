//! Handle-based file operations: read/write, metadata and enumeration via
//! `GetFileInformationByHandleEx`, deletion via `SetFileInformationByHandle`
//! (delete-on-close), and the single ambient-path entry point that opens a
//! root directory capability.

#![allow(unsafe_code)]

use std::ffi::{OsStr, OsString};

use platform::error::Result;
use platform::fs::FileType;

use crate::ffi::win32_surface as w;
use crate::sys::errmap;
use crate::sys::handle::OwnedWinHandle;
use crate::util::wide::{from_wide, to_wide_nul};

/// Open an absolute directory path as a root capability handle. The only
/// place an ambient path enters this backend (RFC v2 §5.3) — everything
/// after is relative to a handle via `sys::nt`.
pub fn open_ambient_dir(path: &OsStr) -> Result<OwnedWinHandle> {
    let wide = to_wide_nul(path);
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 buffer outliving the
    // call; all other arguments are documented-valid constants or null.
    let handle = unsafe {
        w::CreateFileW(
            wide.as_ptr(),
            w::FILE_LIST_DIRECTORY | w::FILE_READ_ATTRIBUTES | w::FILE_TRAVERSE | w::SYNCHRONIZE,
            w::FILE_SHARE_READ | w::FILE_SHARE_WRITE | w::FILE_SHARE_DELETE,
            std::ptr::null(),
            w::OPEN_EXISTING,
            // Required to open a directory handle at all; without it
            // CreateFileW refuses directories by design.
            w::FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    OwnedWinHandle::from_raw(handle).ok_or_else(|| errmap::last_win32_err("CreateFileW", path))
}

/// `ReadFile` into `buf`.
pub fn read(handle: &OwnedWinHandle, buf: &mut [u8]) -> Result<usize> {
    let mut n: u32 = 0;
    let len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
    // SAFETY: `buf` is a valid writable region of at least `len` bytes and
    // `n` a valid out-pointer, both outliving the call; the handle is open
    // and synchronous (no OVERLAPPED), so the null overlapped is valid.
    let ok = unsafe {
        w::ReadFile(
            handle.as_raw(),
            buf.as_mut_ptr(),
            len,
            &mut n,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        // SAFETY: `GetLastError` takes no arguments and has no
        // preconditions.
        let code = unsafe { w::GetLastError() };
        if code == w::ERROR_BROKEN_PIPE {
            // A pipe whose write side has fully closed reports
            // BROKEN_PIPE on read; that IS end-of-file for pipes —
            // mirroring unix read() returning 0.
            return Ok(0);
        }
        return Err(errmap::last_win32_err("ReadFile", OsStr::new("")));
    }
    Ok(n as usize)
}

/// `WriteFile` from `buf`.
pub fn write(handle: &OwnedWinHandle, buf: &[u8]) -> Result<usize> {
    let mut n: u32 = 0;
    let len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
    // SAFETY: `buf` is a valid readable region of at least `len` bytes and
    // `n` a valid out-pointer, both outliving the call; the handle is open
    // and synchronous, so the null overlapped is valid.
    let ok = unsafe {
        w::WriteFile(
            handle.as_raw(),
            buf.as_ptr(),
            len,
            &mut n,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(errmap::last_win32_err("WriteFile", OsStr::new("")));
    }
    Ok(n as usize)
}

fn file_type_of_attributes(attrs: u32) -> FileType {
    // Any reparse point is classified Symlink for now — distinguishing
    // IO_REPARSE_TAG_SYMLINK from junctions and other tags is deferred
    // until a consumer needs the distinction (consumer gate, RFC v2 §3).
    if attrs & w::FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        FileType::Symlink
    } else if attrs & w::FILE_ATTRIBUTE_DIRECTORY != 0 {
        FileType::Dir
    } else {
        FileType::File
    }
}

/// (file type, size) for an open handle, via `FileBasicInfo`'s attributes
/// and the basic-info query's `EndOfFile` counterpart.
pub fn metadata_by_handle(handle: &OwnedWinHandle, path: &OsStr) -> Result<(FileType, u64)> {
    // SAFETY: `info` is a valid out-buffer of exactly the queried class's
    // size, outliving the call; the handle is open with at least
    // FILE_READ_ATTRIBUTES access.
    let basic: w::FILE_BASIC_INFO = unsafe {
        let mut info = std::mem::zeroed::<w::FILE_BASIC_INFO>();
        let ok = w::GetFileInformationByHandleEx(
            handle.as_raw(),
            w::FileBasicInfo,
            (&mut info as *mut w::FILE_BASIC_INFO).cast(),
            std::mem::size_of::<w::FILE_BASIC_INFO>() as u32,
        );
        if ok == 0 {
            return Err(errmap::last_win32_err("GetFileInformationByHandleEx", path));
        }
        info
    };
    let file_type = file_type_of_attributes(basic.FileAttributes);
    let len = if file_type == FileType::Dir {
        // Directories report no meaningful byte length; pin 0 across
        // backends rather than exposing an allocation-size accident.
        0
    } else {
        end_of_file(handle, path)?
    };
    Ok((file_type, len))
}

fn end_of_file(handle: &OwnedWinHandle, path: &OsStr) -> Result<u64> {
    // FILE_STANDARD_INFO is not in the curated surface; EndOfFile is also
    // available as GetFileSizeEx, but that widens the surface for one
    // field. Query the standard-info layout directly instead.
    #[repr(C)]
    struct FileStandardInfo {
        allocation_size: i64,
        end_of_file: i64,
        number_of_links: u32,
        delete_pending: u8,
        directory: u8,
    }
    const FILE_STANDARD_INFO_CLASS: i32 = 1; // FileStandardInfo
                                             // SAFETY: `info` is a valid out-buffer at least as large as the
                                             // documented FILE_STANDARD_INFO layout, outliving the call; the handle
                                             // is open with at least FILE_READ_ATTRIBUTES access.
    let info: FileStandardInfo = unsafe {
        let mut info = std::mem::zeroed::<FileStandardInfo>();
        let ok = w::GetFileInformationByHandleEx(
            handle.as_raw(),
            FILE_STANDARD_INFO_CLASS,
            (&mut info as *mut FileStandardInfo).cast(),
            std::mem::size_of::<FileStandardInfo>() as u32,
        );
        if ok == 0 {
            return Err(errmap::last_win32_err("GetFileInformationByHandleEx", path));
        }
        info
    };
    Ok(info.end_of_file as u64)
}

/// Enumerate a directory handle's entries, excluding `.`/`..`. The handle
/// carries per-handle enumeration state, so callers pass a fresh handle
/// (the Linux backend re-opens `"."` for the same reason).
pub fn read_dir_entries(handle: &OwnedWinHandle) -> Result<Vec<(OsString, FileType)>> {
    let mut out = Vec::new();
    // Large enough for many entries per call; the kernel packs as many
    // FILE_FULL_DIR_INFO records as fit.
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        // SAFETY: `buf` is a valid writable region of its stated size,
        // outliving the call; the handle is a directory opened with
        // FILE_LIST_DIRECTORY access.
        let ok = unsafe {
            w::GetFileInformationByHandleEx(
                handle.as_raw(),
                w::FileFullDirectoryInfo,
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
            )
        };
        if ok == 0 {
            // SAFETY: `GetLastError` takes no arguments and has no
            // preconditions.
            let code = unsafe { w::GetLastError() };
            if code == w::ERROR_NO_MORE_FILES {
                return Ok(out);
            }
            return Err(errmap::last_win32_err(
                "GetFileInformationByHandleEx",
                OsStr::new(""),
            ));
        }
        let mut offset = 0usize;
        loop {
            // SAFETY: on success the kernel wrote a chain of
            // FILE_FULL_DIR_INFO records into `buf` starting at offset 0,
            // each 8-aligned, linked by NextEntryOffset, with
            // FileNameLength bytes of UTF-16 name immediately at FileName;
            // `offset` only ever takes values from that chain, so the
            // reads below stay inside the initialized region.
            let (name, attrs, next) = unsafe {
                let entry = buf.as_ptr().add(offset).cast::<w::FILE_FULL_DIR_INFO>();
                let name_len = (*entry).FileNameLength as usize / 2;
                let name_ptr = std::ptr::addr_of!((*entry).FileName).cast::<u16>();
                let name_units: Vec<u16> = std::slice::from_raw_parts(name_ptr, name_len).to_vec();
                (
                    name_units,
                    (*entry).FileAttributes,
                    (*entry).NextEntryOffset as usize,
                )
            };
            if name != [b'.' as u16] && name != [b'.' as u16, b'.' as u16] {
                out.push((from_wide(&name), file_type_of_attributes(attrs)));
            }
            if next == 0 {
                break;
            }
            offset += next;
        }
    }
}

/// Mark an open handle's file for deletion at last close (the disposition
/// path both `remove_file` and `remove_dir` take; a non-empty directory is
/// refused here by the OS with ERROR_DIR_NOT_EMPTY).
pub fn mark_delete(handle: &OwnedWinHandle, path: &OsStr) -> Result<()> {
    let info = w::FILE_DISPOSITION_INFO { DeleteFile: 1 };
    // SAFETY: `info` is a valid, fully initialized FILE_DISPOSITION_INFO
    // outliving the call; the handle is open with DELETE access.
    let ok = unsafe {
        w::SetFileInformationByHandle(
            handle.as_raw(),
            w::FileDispositionInfo,
            (&info as *const w::FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of::<w::FILE_DISPOSITION_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(errmap::last_win32_err("SetFileInformationByHandle", path));
    }
    Ok(())
}
