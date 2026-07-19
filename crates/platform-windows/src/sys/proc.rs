//! Process spawning over `CreateProcessW` (RFC v2 §5.4; extraction map
//! step 2, first slice). The command line is built exclusively by
//! [`crate::winargv`] — no other quoting path exists in this crate.
//!
//! Stdio model: when every slot is `Inherit`, the child inherits the
//! parent's std handles the default way (no handle list). When any slot
//! is `Null`, `STARTF_USESTDHANDLES` is used and each slot gets an
//! explicit handle — an inheritable `NUL` device handle for `Null`, or an
//! inheritable duplicate of the parent's own std handle for `Inherit`
//! (duplication because the parent's handle may itself be
//! non-inheritable; the duplicates are closed after the spawn snapshot,
//! mirroring rush's winstdio lesson that `CreateProcessW` snapshots at
//! spawn).

#![allow(unsafe_code)]

use std::ffi::OsStr;

use platform::error::{ErrorKind, OsCode, PlatformError, Result};
use platform::process::{EnvSpec, ExitStatus, Stdio};

use crate::ffi::win32_surface as w;
use crate::sys::errmap;
use crate::sys::handle::OwnedWinHandle;
use crate::util::wide::to_wide_nul;

/// Build the `CREATE_UNICODE_ENVIRONMENT` block for an explicit env, or
/// `None` to inherit. Each entry is `NAME=value\0`, with a final extra
/// NUL (double-NUL for the empty block).
fn env_block(env: &EnvSpec) -> Option<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;
    let map = match env {
        EnvSpec::Inherit => return None,
        EnvSpec::Explicit(map) => map,
    };
    let mut block: Vec<u16> = Vec::new();
    for (k, v) in map {
        block.extend(k.encode_wide());
        block.push(u16::from(b'='));
        block.extend(v.encode_wide());
        block.push(0);
    }
    block.push(0);
    if block.len() == 1 {
        block.push(0);
    }
    Some(block)
}

/// An inheritable handle for one std slot, plus the RAII that closes any
/// handle this module itself opened or duplicated.
enum SlotHandle {
    Owned(OwnedWinHandle),
}

impl SlotHandle {
    fn raw(&self) -> w::HANDLE {
        match self {
            SlotHandle::Owned(h) => h.as_raw(),
        }
    }
}

fn inheritable_nul(read: bool) -> Result<SlotHandle> {
    let access = if read {
        w::FILE_GENERIC_READ
    } else {
        w::FILE_GENERIC_WRITE
    };
    let sa = w::SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<w::SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };
    let name = to_wide_nul(OsStr::new("NUL"));
    // SAFETY: `name` is a valid NUL-terminated UTF-16 buffer and `sa` a
    // fully initialized SECURITY_ATTRIBUTES, both outliving the call.
    let h = unsafe {
        w::CreateFileW(
            name.as_ptr(),
            access,
            w::FILE_SHARE_READ | w::FILE_SHARE_WRITE,
            &sa,
            w::OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    OwnedWinHandle::from_raw(h)
        .map(SlotHandle::Owned)
        .ok_or_else(|| errmap::last_win32_err("CreateFileW", OsStr::new("NUL")))
}

fn inheritable_dup_of_std(slot: u32) -> Result<Option<SlotHandle>> {
    // SAFETY: `GetStdHandle` takes a documented slot constant and has no
    // other preconditions.
    let current = unsafe { w::GetStdHandle(slot) };
    if current.is_null() || current == w::INVALID_HANDLE_VALUE {
        // No handle in this slot (e.g. a detached process): leave it
        // empty rather than fail the whole spawn.
        return Ok(None);
    }
    let mut dup: w::HANDLE = std::ptr::null_mut();
    // SAFETY: source process/handle are this process's own valid handles;
    // `dup` is a valid out-pointer; DUPLICATE_SAME_ACCESS with
    // bInheritHandle=1 is the documented way to mint an inheritable
    // duplicate.
    let ok = unsafe {
        w::DuplicateHandle(
            w::GetCurrentProcess(),
            current,
            w::GetCurrentProcess(),
            &mut dup,
            0,
            1,
            w::DUPLICATE_SAME_ACCESS,
        )
    };
    if ok == 0 {
        return Err(errmap::last_win32_err("DuplicateHandle", OsStr::new("")));
    }
    Ok(OwnedWinHandle::from_raw(dup).map(SlotHandle::Owned))
}

/// Spawn `command_line` (winargv-built, not yet NUL-terminated) with
/// working directory `cwd`. Returns (process handle, pid); the thread
/// handle is closed here — nothing in this slice resumes threads
/// (suspended spawn arrives with process groups).
pub fn spawn(
    command_line: &[u16],
    cwd: &OsStr,
    env: &EnvSpec,
    stdio: [Stdio; 3],
) -> Result<(OwnedWinHandle, u32)> {
    let mut line: Vec<u16> = command_line.to_vec();
    line.push(0);
    let cwd_w = to_wide_nul(cwd);
    let block = env_block(env);

    let use_handles = stdio.iter().any(|s| matches!(s, Stdio::Null));
    let mut slot_handles: [Option<SlotHandle>; 3] = [None, None, None];
    // SAFETY: STARTUPINFOW is plain-old-data for which all-zeroes is a
    // valid starting value; `cb` is set before use.
    let mut si: w::STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<w::STARTUPINFOW>() as u32;
    if use_handles {
        let slots = [
            w::STD_INPUT_HANDLE,
            w::STD_OUTPUT_HANDLE,
            w::STD_ERROR_HANDLE,
        ];
        for (i, spec) in stdio.iter().enumerate() {
            slot_handles[i] = match spec {
                Stdio::Null => Some(inheritable_nul(i == 0)?),
                Stdio::Inherit => inheritable_dup_of_std(slots[i])?,
            };
        }
        si.dwFlags |= w::STARTF_USESTDHANDLES;
        si.hStdInput = slot_handles[0]
            .as_ref()
            .map_or(std::ptr::null_mut(), SlotHandle::raw);
        si.hStdOutput = slot_handles[1]
            .as_ref()
            .map_or(std::ptr::null_mut(), SlotHandle::raw);
        si.hStdError = slot_handles[2]
            .as_ref()
            .map_or(std::ptr::null_mut(), SlotHandle::raw);
    }

    let flags = if block.is_some() {
        w::CREATE_UNICODE_ENVIRONMENT
    } else {
        0
    };
    let env_ptr = block
        .as_ref()
        .map_or(std::ptr::null(), |b| b.as_ptr().cast::<core::ffi::c_void>());

    // SAFETY: PROCESS_INFORMATION is plain-old-data; CreateProcessW
    // overwrites it on success before we read it.
    let mut pi: w::PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `line` is an owned, mutable, NUL-terminated UTF-16 buffer
    // (CreateProcessW may write into it); `cwd_w` is NUL-terminated and
    // outlives the call; `env_ptr` is null (inherit) or a well-formed
    // double-NUL block per `env_block`; `si`/`pi` are valid with `cb`
    // set; any slot handles are open, inheritable, and outlive the call
    // (`slot_handles` lives to end of scope); null security attributes
    // and application name are documented-valid.
    let ok = unsafe {
        w::CreateProcessW(
            std::ptr::null(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            i32::from(use_handles),
            flags,
            env_ptr,
            cwd_w.as_ptr(),
            &si,
            &mut pi,
        )
    };
    if ok == 0 {
        return Err(errmap::last_win32_err("CreateProcessW", cwd));
    }
    // Slot handles (NUL opens and inheritable duplicates) close as
    // `slot_handles` drops here — CreateProcessW has snapshotted them.
    let process = OwnedWinHandle::from_raw(pi.hProcess)
        .ok_or_else(|| PlatformError::new(ErrorKind::Other, OsCode::None, "CreateProcessW"))?;
    // The main thread runs immediately in this slice; its handle is not
    // retained. Suspended spawn (groups) will retain it.
    drop(OwnedWinHandle::from_raw(pi.hThread));
    Ok((process, pi.dwProcessId))
}

/// Block until `process` exits; decode the code. `Signaled` is never
/// produced on Windows (behavior spec `docs/behavior/process.md`).
pub fn wait(process: &OwnedWinHandle) -> Result<ExitStatus> {
    // SAFETY: `process` is a valid open process handle for the life of
    // `&self`.
    let r = unsafe { w::WaitForSingleObject(process.as_raw(), w::INFINITE) };
    if r != w::WAIT_OBJECT_0 {
        return Err(errmap::last_win32_err(
            "WaitForSingleObject",
            OsStr::new(""),
        ));
    }
    let mut code: u32 = 0;
    // SAFETY: same valid handle; `code` is a valid out-pointer.
    let ok = unsafe { w::GetExitCodeProcess(process.as_raw(), &mut code) };
    if ok == 0 {
        return Err(errmap::last_win32_err("GetExitCodeProcess", OsStr::new("")));
    }
    Ok(ExitStatus::Code(code as i32))
}
