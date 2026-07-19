//! Handle-based file operations: read/write, metadata and enumeration via
//! `GetFileInformationByHandleEx`, deletion via `SetFileInformationByHandle`
//! (delete-on-close), and the single ambient-path entry point that opens a
//! root directory capability.

#![allow(unsafe_code)]

use std::ffi::{OsStr, OsString};

use platform::error::Result;
use platform::fs::FileType;

use crate::ffi::nt_surface as nt;
use crate::ffi::win32_surface as w;
use crate::sys::errmap;
use crate::sys::handle::OwnedWinHandle;
use crate::sys::nt as ntsys;
use crate::util::wide::{from_wide, to_wide_nt_component, to_wide_nul};

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

/// `FlushFileBuffers` — durability (`File::sync_all`).
pub fn sync_all(handle: &OwnedWinHandle) -> Result<()> {
    // SAFETY: the handle is a live, open, writable file handle.
    let ok = unsafe { w::FlushFileBuffers(handle.as_raw()) };
    if ok == 0 {
        return Err(errmap::last_win32_err("FlushFileBuffers", OsStr::new("")));
    }
    Ok(())
}

/// Rename `from` (opened with DELETE access, relative to `dir`) to `to`
/// (a name relative to that same `dir`), via `NtSetInformationFile` +
/// `FILE_RENAME_INFORMATION`'s `RootDirectory` field — the
/// handle-relative rename this backend's capability model needs (D11,
/// convergence roadmap Phase 3), since there is no ambient path to hand
/// `MoveFileExW`.
///
/// **Not** the Win32 `SetFileInformationByHandle`: a real windows-latest
/// CI run proved that wrapper rejects a non-null `RootDirectory` for the
/// classic `FileRenameInfo` class with `ERROR_INVALID_PARAMETER` —
/// handle-relative rename turns out to be a Win32-layer restriction, not
/// an NT one (a second, narrower ntdll admission, `ffi::nt_surface`).
///
/// `FILE_RENAME_INFORMATION` ends in a flexible array member
/// (`FileName`) at an offset **not** equal to `size_of::<…>()` — the
/// struct's own alignment (8, from the `HANDLE` field) pads its *end*,
/// past `FileName`, so `size_of` overshoots by that padding (the first
/// bug this function shipped with, before the Win32-vs-NT one). The
/// offset is computed via `addr_of!` pointer subtraction on a zeroed
/// instance — the compiler's own layout, not a hand-derived guess, and
/// available without `offset_of!` (unavailable at this workspace's 1.75
/// MSRV; stabilized in 1.77).
pub fn rename(dir: &OwnedWinHandle, from: &OsStr, to: &OsStr, replace: bool) -> Result<()> {
    // The target may be a file or a directory; try the file disposition
    // first (the common case) and fall back to the directory one rather
    // than requiring the caller to know which (mirroring the Linux
    // backend, where renameat2 doesn't care either).
    let handle = ntsys::open_relative(
        dir,
        from,
        w::DELETE | w::SYNCHRONIZE,
        nt::FILE_OPEN,
        nt::FILE_NON_DIRECTORY_FILE | nt::FILE_SYNCHRONOUS_IO_NONALERT,
    )
    .or_else(|_| {
        ntsys::open_relative(
            dir,
            from,
            w::DELETE | w::SYNCHRONIZE,
            nt::FILE_OPEN,
            nt::FILE_DIRECTORY_FILE | nt::FILE_SYNCHRONOUS_IO_NONALERT,
        )
    })?;

    let name = to_wide_nt_component(to);
    let name_bytes = (name.len() * 2) as u32;

    // SAFETY: an all-zero bit pattern is a valid `FILE_RENAME_INFORMATION`
    // (BOOLEAN/HANDLE/u32/u16 are all valid at all-zeroes); never read
    // through the union, only used to compute `FileName`'s real offset.
    let probe: nt::FILE_RENAME_INFORMATION = unsafe { std::mem::zeroed() };
    let base = std::ptr::addr_of!(probe) as usize;
    let name_field = std::ptr::addr_of!(probe.FileName) as usize;
    let name_offset = name_field - base;

    // SAFETY: an all-zero bit pattern is a valid `FILE_RENAME_INFORMATION`;
    // every field is then overwritten below before this value is read as
    // bytes.
    let mut header: nt::FILE_RENAME_INFORMATION = unsafe { std::mem::zeroed() };
    header.Anonymous = nt::FILE_RENAME_INFORMATION_0 {
        ReplaceIfExists: u8::from(replace),
    };
    header.RootDirectory = dir.as_raw();
    header.FileNameLength = name_bytes;

    let mut buf = vec![0u8; name_offset + name_bytes as usize];
    // SAFETY: `buf` is at least `name_offset` bytes (allocated above);
    // copying exactly the header portion up to (not including)
    // `FileName` is in-bounds on both sides and leaves `FileName`
    // itself untouched — it is overwritten fully by the loop below.
    unsafe {
        std::ptr::copy_nonoverlapping(
            (&header as *const nt::FILE_RENAME_INFORMATION).cast::<u8>(),
            buf.as_mut_ptr(),
            name_offset,
        );
    }
    // `FILE_RENAME_INFORMATION`'s `FileName` is raw UTF-16 code units,
    // not a `u16` slice the ABI guarantees any transmute safety for;
    // write each unit's bytes explicitly rather than reinterpreting the
    // slice.
    for (i, unit) in name.iter().enumerate() {
        let at = name_offset + i * 2;
        buf[at..at + 2].copy_from_slice(&unit.to_le_bytes());
    }

    // SAFETY: `IO_STATUS_BLOCK` is plain-old-data for which all-zeroes is
    // a valid starting value, overwritten by the call below.
    let mut iosb: w::IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
    // SAFETY: `iosb` is a valid out-pointer; `buf` holds a fully
    // populated `FILE_RENAME_INFORMATION` at its start followed by the
    // wide filename at its real, compiler-computed offset, with its
    // length passed exactly as `buf.len()`; `handle` is open with
    // DELETE access.
    let status = unsafe {
        nt::NtSetInformationFile(
            handle.as_raw(),
            &mut iosb,
            buf.as_ptr().cast(),
            buf.len() as u32,
            nt::FileRenameInformation,
        )
    };
    if status != w::STATUS_SUCCESS {
        return Err(errmap::nt_err(status, "NtSetInformationFile", to));
    }
    Ok(())
}
