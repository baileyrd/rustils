//! Process spawning over `posix_spawn` (RFC v2 §5.4). Extraction map D1
//! informs the shape; `posix_spawn` is preferred over hand-rolled
//! fork+exec because it removes the async-signal-safe critical region
//! from this crate entirely — every allocation (CStrings, pointer arrays,
//! file actions) happens before the call, in the parent, which is the fix
//! standard for the v1 scaffold's B-1/B-2 bug class by construction.

#![allow(unsafe_code)]

use std::ffi::{CString, OsStr, OsString};
use std::os::unix::ffi::OsStrExt;

use platform::error::{ErrorKind, OsCode, PlatformError, Result};
use platform::process::{EnvSpec, ExitStatus, GroupSpec, Stdio};

use crate::ffi::libc_surface as c;

fn to_cstring(s: &OsStr, op: &'static str) -> Result<CString> {
    CString::new(s.as_bytes())
        .map_err(|_| PlatformError::new(ErrorKind::InvalidInput, OsCode::None, op).with_path(s))
}

fn errno_err(op: &'static str, code: i32, path: &OsStr) -> PlatformError {
    let kind = match code {
        libc::ENOENT => ErrorKind::NotFound,
        libc::EACCES | libc::EPERM => ErrorKind::PermissionDenied,
        libc::ENOTDIR => ErrorKind::NotADirectory,
        libc::EINVAL => ErrorKind::InvalidInput,
        _ => ErrorKind::Other,
    };
    let e = PlatformError::new(kind, OsCode::Errno(code), op);
    if path.is_empty() {
        e
    } else {
        e.with_path(path)
    }
}

/// The environment for the child, fully materialized. `Inherit` snapshots
/// the parent's environment at spawn time (the same semantics std's
/// spawn has); `Explicit` contains exactly the given variables.
fn build_env(env: &EnvSpec) -> Result<Vec<CString>> {
    let pairs: Vec<(OsString, OsString)> = match env {
        EnvSpec::Inherit => std::env::vars_os().collect(),
        EnvSpec::Explicit(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
    };
    let mut out = Vec::with_capacity(pairs.len());
    for (k, v) in pairs {
        let mut kv = k;
        kv.push("=");
        kv.push(&v);
        out.push(to_cstring(&kv, "posix_spawn env")?);
    }
    Ok(out)
}

/// RAII for `posix_spawn_file_actions_t` so every early-error path
/// destroys what it initialized.
struct FileActions(c::posix_spawn_file_actions_t);

impl FileActions {
    fn new() -> Result<Self> {
        // SAFETY: `actions` is a valid out-pointer; init writes it before
        // any use, and the value is destroyed exactly once by Drop.
        let (r, actions) = unsafe {
            let mut actions: c::posix_spawn_file_actions_t = std::mem::zeroed();
            let r = c::posix_spawn_file_actions_init(&mut actions);
            (r, actions)
        };
        if r != 0 {
            return Err(errno_err(
                "posix_spawn_file_actions_init",
                r,
                OsStr::new(""),
            ));
        }
        Ok(Self(actions))
    }
}

impl Drop for FileActions {
    fn drop(&mut self) {
        // SAFETY: `self.0` was successfully initialized at construction
        // and is destroyed exactly once here.
        unsafe {
            c::posix_spawn_file_actions_destroy(&mut self.0);
        }
    }
}

/// RAII for `posix_spawnattr_t`, mirroring [`FileActions`].
struct SpawnAttr(c::posix_spawnattr_t);

impl SpawnAttr {
    fn new() -> Result<Self> {
        // SAFETY: `attr` is a valid out-pointer; init writes it before
        // any use, and the value is destroyed exactly once by Drop.
        let (r, attr) = unsafe {
            let mut attr: c::posix_spawnattr_t = std::mem::zeroed();
            let r = c::posix_spawnattr_init(&mut attr);
            (r, attr)
        };
        if r != 0 {
            return Err(errno_err("posix_spawnattr_init", r, OsStr::new("")));
        }
        Ok(Self(attr))
    }
}

impl Drop for SpawnAttr {
    fn drop(&mut self) {
        // SAFETY: `self.0` was successfully initialized at construction
        // and is destroyed exactly once here.
        unsafe {
            c::posix_spawnattr_destroy(&mut self.0);
        }
    }
}

/// Spawn `path` (an already-resolved program path) with `argv0` + `args`,
/// working directory `cwd`, environment per `env`, and the given stdio
/// wiring. `GroupSpec::NewGroup` makes the child a fresh process-group
/// leader before it executes (`POSIX_SPAWN_SETPGROUP` with pgroup 0 — the
/// race-free at-spawn placement; extraction map D1's double-`setpgid`
/// lesson is subsumed by the kernel doing it during spawn). Returns the
/// child pid.
pub fn spawn(
    path: &OsStr,
    argv0: &OsStr,
    args: &[OsString],
    cwd: &OsStr,
    env: &EnvSpec,
    stdio: [Stdio; 3],
    group: GroupSpec,
) -> Result<c::pid_t> {
    // Every allocation happens here, before the spawn call (B-1/B-2 fix
    // standard): owned CStrings outlive the raw pointer arrays built from
    // them, and both outlive the call.
    let c_path = to_cstring(path, "posix_spawn")?;
    let c_cwd = to_cstring(cwd, "posix_spawn chdir")?;
    let mut c_argv: Vec<CString> = Vec::with_capacity(args.len() + 1);
    c_argv.push(to_cstring(argv0, "posix_spawn argv")?);
    for a in args {
        c_argv.push(to_cstring(a, "posix_spawn argv")?);
    }
    let c_env = build_env(env)?;

    let mut argv_ptrs: Vec<*mut c::c_char> = c_argv.iter().map(|s| s.as_ptr().cast_mut()).collect();
    argv_ptrs.push(std::ptr::null_mut());
    let mut env_ptrs: Vec<*mut c::c_char> = c_env.iter().map(|s| s.as_ptr().cast_mut()).collect();
    env_ptrs.push(std::ptr::null_mut());

    let mut actions = FileActions::new()?;
    // SAFETY: `actions.0` is initialized; `c_cwd` is a valid
    // NUL-terminated path outliving the spawn call.
    let r = unsafe { c::posix_spawn_file_actions_addchdir_np(&mut actions.0, c_cwd.as_ptr()) };
    if r != 0 {
        return Err(errno_err("posix_spawn chdir", r, cwd));
    }
    let devnull = CString::new("/dev/null").expect("no interior NUL");
    for (fd, spec) in stdio.iter().enumerate() {
        if matches!(spec, Stdio::Null) {
            let flags = if fd == 0 { c::O_RDONLY } else { c::O_WRONLY };
            // SAFETY: `actions.0` is initialized; `devnull` is a valid
            // NUL-terminated path outliving the spawn call.
            let r = unsafe {
                c::posix_spawn_file_actions_addopen(
                    &mut actions.0,
                    fd as c::c_int,
                    devnull.as_ptr(),
                    flags,
                    0,
                )
            };
            if r != 0 {
                return Err(errno_err("posix_spawn addopen", r, OsStr::new("/dev/null")));
            }
        }
    }

    let mut attr = SpawnAttr::new()?;
    if group == GroupSpec::NewGroup {
        // SAFETY: `attr.0` is initialized; setflags/setpgroup have no
        // pointer arguments beyond it.
        let r = unsafe {
            let r = c::posix_spawnattr_setflags(&mut attr.0, c::POSIX_SPAWN_SETPGROUP as _);
            if r == 0 {
                c::posix_spawnattr_setpgroup(&mut attr.0, 0)
            } else {
                r
            }
        };
        if r != 0 {
            return Err(errno_err("posix_spawnattr_setpgroup", r, OsStr::new("")));
        }
    }

    let mut pid: c::pid_t = 0;
    // SAFETY: every pointer argument references an owned value that
    // outlives this call (`c_path`, the NUL-terminated `argv_ptrs`/
    // `env_ptrs` arrays whose elements point into `c_argv`/`c_env`, the
    // initialized `actions.0` and `attr.0`); `pid` is a valid
    // out-pointer.
    let r = unsafe {
        c::posix_spawn(
            &mut pid,
            c_path.as_ptr(),
            &actions.0,
            &attr.0,
            argv_ptrs.as_ptr(),
            env_ptrs.as_ptr(),
        )
    };
    if r != 0 {
        return Err(errno_err("posix_spawn", r, path));
    }
    Ok(pid)
}

/// `SIGKILL` the whole process group led by `pid` (which must have been
/// spawned with `GroupSpec::NewGroup`, making pid == pgid).
pub fn kill_group(pid: c::pid_t) -> Result<()> {
    // SAFETY: kill has no pointer arguments; the negative-pid form
    // targets the process group `pid` leads.
    let r = unsafe { c::kill(-pid, c::SIGKILL) };
    if r != 0 {
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        return Err(errno_err("kill", code, OsStr::new("")));
    }
    Ok(())
}

/// `SIGKILL` the single process `pid`.
pub fn kill_single(pid: c::pid_t) -> Result<()> {
    // SAFETY: kill has no pointer arguments.
    let r = unsafe { c::kill(pid, c::SIGKILL) };
    if r != 0 {
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        return Err(errno_err("kill", code, OsStr::new("")));
    }
    Ok(())
}

fn decode(status: c::c_int) -> ExitStatus {
    if c::WIFEXITED(status) {
        ExitStatus::Code(c::WEXITSTATUS(status))
    } else if c::WIFSIGNALED(status) {
        ExitStatus::Signaled(c::WTERMSIG(status))
    } else {
        // Stop/continue events are impossible without WUNTRACED/
        // WCONTINUED flags; classify defensively rather than panic.
        ExitStatus::Code(1)
    }
}

/// Blocking `waitpid` on `pid`, decoding the raw status word into the
/// uniform [`ExitStatus`] (B-5: the raw word never crosses this boundary).
pub fn wait(pid: c::pid_t) -> Result<ExitStatus> {
    let mut status: c::c_int = 0;
    loop {
        // SAFETY: `status` is a valid out-pointer; `pid` is a child this
        // process spawned and has not yet waited on (enforced by the
        // consuming `Child::wait` above this layer).
        let r = unsafe { c::waitpid(pid, &mut status, 0) };
        if r == pid {
            break;
        }
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        if code == libc::EINTR {
            continue;
        }
        return Err(errno_err("waitpid", code, OsStr::new("")));
    }
    Ok(decode(status))
}

/// Non-blocking `waitpid(WNOHANG)`: `Some(decoded)` if `pid` terminated
/// (the zombie is reaped — the caller must stash the result), `None` if
/// still running.
pub fn try_wait(pid: c::pid_t) -> Result<Option<ExitStatus>> {
    let mut status: c::c_int = 0;
    loop {
        // SAFETY: `status` is a valid out-pointer; `pid` is a child this
        // process spawned and has not reaped yet (the caller stashes the
        // result of a successful poll and never polls again).
        let r = unsafe { c::waitpid(pid, &mut status, c::WNOHANG) };
        if r == pid {
            return Ok(Some(decode(status)));
        }
        if r == 0 {
            return Ok(None);
        }
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        if code == libc::EINTR {
            continue;
        }
        return Err(errno_err("waitpid", code, OsStr::new("")));
    }
}
