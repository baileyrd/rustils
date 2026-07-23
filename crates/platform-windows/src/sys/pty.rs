//! ConPTY primitives (RFC v2 R5+, D13, convergence roadmap Phase 7,
//! rustils#83, part 2/2 following #82's Linux backend): pseudo console
//! creation, spawning attached to it, resize, and drain-before-close
//! teardown.
//!
//! **Attach happens only at `CreateProcessW` time.** Unlike Unix, there
//! is no Win32 call to attach an already-created pseudo console to an
//! already-running process — `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE` only
//! exists as a `STARTUPINFOEXW` attribute passed to `CreateProcessW`
//! itself. See `docs/design-discussion-pty.md`'s "shape question"
//! section for why `platform::pty::Pty::spawn` is one atomic operation
//! rather than separable open/attach steps.
//!
//! **Two separate handles, not one fd.** ConPTY's master side is a pair
//! of anonymous pipes — an input pipe the master writes to, an output
//! pipe the master reads from — not a single bidirectional descriptor
//! the way a Unix pty master fd is. `WindowsPtyMaster` (crate root)
//! holds both; there is no honest single-handle `AsHandle`/`AsRawHandle`
//! impl to offer the way `LinuxPtyMaster`'s `AsFd`/`AsRawFd` does, so it
//! isn't attempted — two named accessors instead (see that type's own
//! doc comment).
//!
//! **`ClosePseudoConsole` can deadlock against an un-drained output
//! pipe.** It blocks until conhost's internal writer thread finishes,
//! which can itself be blocked writing into a full pipe nobody is
//! reading — the EOF-vs-exit teardown lesson D13 already flagged.
//! [`close`] drains the output pipe with a bounded, non-blocking
//! `PeekNamedPipe` loop before calling `ClosePseudoConsole`, rather than
//! spawning a background reader thread for every session: the trait's
//! own `PtyMaster::read`/`write` contract is synchronous/blocking
//! already (matching #82's Linux shape), so nothing needs a thread
//! except this one teardown step.

#![allow(unsafe_code)]

use std::ffi::OsStr;

use platform::error::{ErrorKind, OsCode, PlatformError, Result};
use platform::process::EnvSpec;
use platform::term::WinSize;

use crate::ffi::win32_surface as w;
use crate::sys::errmap;
use crate::sys::handle::OwnedWinHandle;
use crate::sys::proc;
use crate::util::wide::to_wide_nul;

fn create_pipe_pair() -> Result<(OwnedWinHandle, OwnedWinHandle)> {
    let mut read: w::HANDLE = std::ptr::null_mut();
    let mut write: w::HANDLE = std::ptr::null_mut();
    // SAFETY: `read`/`write` are valid out-pointers; null security
    // attributes and a zero buffer-size hint are both documented-valid
    // (the OS chooses a default buffer size). Unlike `sys::proc`'s own
    // `make_pipe`, neither end needs to be inheritable — ConPTY consumes
    // the ends handed to it directly (`create_pty` closes them right
    // after `CreatePseudoConsole` returns; see Microsoft's own sample,
    // which does the same), not via child-process handle inheritance.
    let ok = unsafe { w::CreatePipe(&mut read, &mut write, std::ptr::null(), 0) };
    if ok == 0 {
        return Err(errmap::last_win32_err("CreatePipe", OsStr::new("")));
    }
    let read = OwnedWinHandle::from_raw(read)
        .ok_or_else(|| PlatformError::new(ErrorKind::Other, OsCode::None, "CreatePipe"))?;
    let write = OwnedWinHandle::from_raw(write)
        .ok_or_else(|| PlatformError::new(ErrorKind::Other, OsCode::None, "CreatePipe"))?;
    Ok((read, write))
}

/// Create a pseudo console of `size`. Returns `(hpc, master_input,
/// master_output)` — `master_input` is the write end the master writes
/// keystrokes/input to (ConPTY reads the other end), `master_output` is
/// the read end the master reads the child's output from (ConPTY writes
/// the other end).
pub fn create_pty(size: WinSize) -> Result<(w::HPCON, OwnedWinHandle, OwnedWinHandle)> {
    let (conpty_input, master_input) = create_pipe_pair()?;
    let (master_output, conpty_output) = create_pipe_pair()?;

    let coord = w::COORD {
        X: size.cols as i16,
        Y: size.rows as i16,
    };
    let mut hpc: w::HPCON = 0;
    // SAFETY: `conpty_input`/`conpty_output` are valid open pipe handles
    // for the duration of this call; `hpc` is a valid out-pointer.
    let hr = unsafe {
        w::CreatePseudoConsole(
            coord,
            conpty_input.as_raw(),
            conpty_output.as_raw(),
            0,
            &mut hpc,
        )
    };
    // conhost duplicates what it needs internally — these ends close now
    // regardless of outcome, matching Microsoft's own ConPTY sample.
    drop(conpty_input);
    drop(conpty_output);
    if hr < 0 {
        return Err(errmap::hresult_err(hr, "CreatePseudoConsole"));
    }
    Ok((hpc, master_input, master_output))
}

/// Spawn `command_line` (winargv-built, not yet NUL-terminated) attached
/// to `hpc` as its console, with working directory `cwd`. Always grouped
/// (a fresh kill-on-close Job Object, suspended → assign → resume, the
/// same race-free sequence `sys::proc::spawn`'s `new_group` path uses) —
/// a pty-hosted child is unconditionally its own session on Linux (#82),
/// and giving it `kill_tree` semantics unconditionally here mirrors that.
/// Returns `(process, job, pid)`.
pub fn spawn_attached(
    hpc: w::HPCON,
    command_line: &[u16],
    cwd: &OsStr,
    env: &EnvSpec,
) -> Result<(OwnedWinHandle, OwnedWinHandle, u32)> {
    let mut line: Vec<u16> = command_line.to_vec();
    line.push(0);
    let cwd_w = to_wide_nul(cwd);
    let block = proc::env_block(env);

    let mut attr_list_size: usize = 0;
    // SAFETY: a null attribute-list pointer with count=1 is the
    // documented way to query the required buffer size — this call is
    // *expected* to report failure here; only `attr_list_size` (written
    // regardless of the return value, per the documented query mode) is
    // read afterward.
    unsafe {
        w::InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut attr_list_size);
    }
    if attr_list_size == 0 {
        return Err(errmap::last_win32_err(
            "InitializeProcThreadAttributeList",
            OsStr::new(""),
        ));
    }
    let mut attr_buf: Vec<u8> = vec![0u8; attr_list_size];
    let attr_list: w::LPPROC_THREAD_ATTRIBUTE_LIST = attr_buf.as_mut_ptr().cast();
    // SAFETY: `attr_list` points to a buffer of exactly `attr_list_size`
    // bytes, sized by the query above, outliving every use of
    // `attr_list` in this function (owned by the still-in-scope
    // `attr_buf`).
    let ok = unsafe { w::InitializeProcThreadAttributeList(attr_list, 1, 0, &mut attr_list_size) };
    if ok == 0 {
        return Err(errmap::last_win32_err(
            "InitializeProcThreadAttributeList",
            OsStr::new(""),
        ));
    }

    // `DeleteProcThreadAttributeList` must run on every exit path from
    // here on — computed as a value first, cleaned up once after,
    // rather than duplicating the call at every early return.
    let result = (|| -> Result<(OwnedWinHandle, OwnedWinHandle, u32)> {
        // SAFETY: `attr_list` was just successfully initialized above;
        // `hpc` is a valid, live pseudo console handle for the duration
        // of this call (the caller's — it outlives the spawned process);
        // `size_of::<HPCON>()` matches the pointed-to value's actual
        // size.
        let ok = unsafe {
            w::UpdateProcThreadAttribute(
                attr_list,
                0,
                w::PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                (&hpc as *const w::HPCON).cast(),
                std::mem::size_of::<w::HPCON>(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if ok == 0 {
            return Err(errmap::last_win32_err(
                "UpdateProcThreadAttribute",
                OsStr::new(""),
            ));
        }

        // SAFETY: STARTUPINFOEXW is plain-old-data for which all-zeroes
        // is a valid starting value; `cb`/`lpAttributeList` are set
        // before use.
        let mut siex: w::STARTUPINFOEXW = unsafe { std::mem::zeroed() };
        siex.StartupInfo.cb = std::mem::size_of::<w::STARTUPINFOEXW>() as u32;
        siex.lpAttributeList = attr_list;

        let mut flags = w::EXTENDED_STARTUPINFO_PRESENT | w::CREATE_SUSPENDED;
        if block.is_some() {
            flags |= w::CREATE_UNICODE_ENVIRONMENT;
        }
        let env_ptr = block
            .as_ref()
            .map_or(std::ptr::null(), |b| b.as_ptr().cast::<core::ffi::c_void>());

        // SAFETY: PROCESS_INFORMATION is plain-old-data; CreateProcessW
        // overwrites it on success before it's read.
        let mut pi: w::PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        let si_ptr = (&siex as *const w::STARTUPINFOEXW).cast::<w::STARTUPINFOW>();
        // SAFETY: `line` is an owned, mutable, NUL-terminated UTF-16
        // buffer (CreateProcessW may write into it); `cwd_w` is
        // NUL-terminated and outlives the call; `env_ptr` is null
        // (inherit) or a well-formed double-NUL block per
        // `proc::env_block`; `si_ptr` points at a fully initialized
        // `STARTUPINFOEXW` whose leading `STARTUPINFOW.cb` is set to the
        // *extended* struct's size — the documented way `CreateProcessW`
        // recognizes `EXTENDED_STARTUPINFO_PRESENT` and reads
        // `lpAttributeList`; `pi` is a valid out-pointer;
        // `bInheritHandles = FALSE` — ConPTY wiring goes through the
        // attribute list, not inherited std handles, matching
        // Microsoft's own sample; null security attributes and
        // application name are documented-valid.
        let ok = unsafe {
            w::CreateProcessW(
                std::ptr::null(),
                line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                flags,
                env_ptr,
                cwd_w.as_ptr(),
                si_ptr,
                &mut pi,
            )
        };
        if ok == 0 {
            return Err(errmap::last_win32_err("CreateProcessW", cwd));
        }

        let process = OwnedWinHandle::from_raw(pi.hProcess)
            .ok_or_else(|| PlatformError::new(ErrorKind::Other, OsCode::None, "CreateProcessW"))?;
        let thread = OwnedWinHandle::from_raw(pi.hThread);

        let sequence = (|| -> Result<OwnedWinHandle> {
            let job = proc::create_kill_on_close_job()?;
            // SAFETY: both handles are valid and open; the process is
            // suspended, so job membership precedes its first
            // instruction.
            let ok = unsafe { w::AssignProcessToJobObject(job.as_raw(), process.as_raw()) };
            if ok == 0 {
                return Err(errmap::last_win32_err(
                    "AssignProcessToJobObject",
                    OsStr::new(""),
                ));
            }
            let thread = thread.as_ref().ok_or_else(|| {
                PlatformError::new(ErrorKind::Other, OsCode::None, "CreateProcessW thread")
            })?;
            // SAFETY: `thread` is the valid, suspended main-thread
            // handle.
            let prev = unsafe { w::ResumeThread(thread.as_raw()) };
            if prev == u32::MAX {
                return Err(errmap::last_win32_err("ResumeThread", OsStr::new("")));
            }
            Ok(job)
        })();

        match sequence {
            Ok(job) => Ok((process, job, pi.dwProcessId)),
            Err(e) => {
                // SAFETY: `process` is the valid handle of the
                // still-suspended (or at worst just-assigned) child this
                // call created; terminated exactly once here.
                unsafe {
                    w::TerminateProcess(process.as_raw(), 1);
                }
                Err(e)
            }
        }
    })();

    // SAFETY: `attr_list` was successfully initialized above and is
    // destroyed exactly once here, on every exit path — `attr_buf`
    // (which owns the memory `attr_list` points into) is still in scope.
    unsafe {
        w::DeleteProcThreadAttributeList(attr_list);
    }

    result
}

/// `ResizePseudoConsole` — `TIOCSWINSZ`'s ConPTY counterpart.
pub fn resize(hpc: w::HPCON, size: WinSize) -> Result<()> {
    let coord = w::COORD {
        X: size.cols as i16,
        Y: size.rows as i16,
    };
    // SAFETY: `hpc` is a valid, live pseudo console handle owned by the
    // caller for the duration of this call.
    let hr = unsafe { w::ResizePseudoConsole(hpc, coord) };
    if hr < 0 {
        return Err(errmap::hresult_err(hr, "ResizePseudoConsole"));
    }
    Ok(())
}

/// Drain `output` (bounded, non-blocking via `PeekNamedPipe`) then call
/// `ClosePseudoConsole` — see this module's doc comment for why the
/// drain has to happen first.
pub fn close(hpc: w::HPCON, output: &OwnedWinHandle) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut buf = [0u8; 4096];
    loop {
        let mut available: u32 = 0;
        // SAFETY: `output` is a valid open pipe handle; `available` is a
        // valid out-pointer; every other out-pointer is null (not needed
        // here).
        let ok = unsafe {
            w::PeekNamedPipe(
                output.as_raw(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            // Broken pipe (conhost's write end already closed) or any
            // other failure — nothing left to drain either way.
            break;
        }
        if available == 0 {
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            continue;
        }
        let mut read_count: u32 = 0;
        // SAFETY: `output` is a valid open pipe handle; `buf` is a valid
        // writable region of exactly `buf.len()` bytes and `read_count` a
        // valid out-pointer, both outliving the call.
        let ok = unsafe {
            w::ReadFile(
                output.as_raw(),
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut read_count,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            break;
        }
    }
    // SAFETY: `hpc` is a valid, live pseudo console handle; closed
    // exactly once here (the sole caller, `WindowsPtyMaster::drop`,
    // never calls this twice).
    unsafe {
        w::ClosePseudoConsole(hpc);
    }
}
