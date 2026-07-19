//! `Spawner`/`Child` impls over the sys layer (RFC v2 §5.4; extraction
//! map step 2, first slice — spawn/wait/resolve; Job-Object groups and
//! kill-tree follow with suspended spawn). No OS-level code here; the
//! command line comes exclusively from [`crate::winargv`].

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use platform::error::{ErrorKind, OsCode, PlatformError, Result};
use platform::process::{Child, Command, ExitStatus, Spawner};

use crate::sys::handle::OwnedWinHandle;
use crate::sys::proc;
use crate::winargv;

/// The Windows process backend.
#[derive(Debug, Default)]
pub struct WindowsSpawner;

/// A spawned child; `wait` consumes it (double-wait unrepresentable).
pub struct WindowsChild {
    process: OwnedWinHandle,
    pid: u32,
}

impl Child for WindowsChild {
    fn wait(self: Box<Self>) -> Result<ExitStatus> {
        proc::wait(&self.process)
    }

    fn id(&self) -> u32 {
        self.pid
    }
}

/// The default PATHEXT set, per the OS default when the variable is
/// absent.
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

fn has_extension(program: &OsStr) -> bool {
    Path::new(program)
        .extension()
        .is_some_and(|e| !e.is_empty())
}

fn candidate_if_file(path: PathBuf) -> Option<PathBuf> {
    std::fs::metadata(&path)
        .map(|m| m.is_file())
        .unwrap_or(false)
        .then_some(path)
}

/// Try `base` as given (if it already has an extension), then with each
/// PATHEXT suffix.
fn resolve_with_pathext(base: &Path, pathext: &OsStr) -> Option<PathBuf> {
    if has_extension(base.as_os_str()) {
        if let Some(hit) = candidate_if_file(base.to_path_buf()) {
            return Some(hit);
        }
    }
    let exts = pathext.to_string_lossy();
    for ext in exts.split(';').map(str::trim).filter(|e| !e.is_empty()) {
        let mut with_ext = base.as_os_str().to_os_string();
        with_ext.push(ext);
        if let Some(hit) = candidate_if_file(PathBuf::from(with_ext)) {
            return Some(hit);
        }
    }
    None
}

impl Spawner for WindowsSpawner {
    fn spawn(&self, cmd: &Command) -> Result<Box<dyn Child>> {
        let resolved = self.resolve(&cmd.program)?;
        let args: Vec<&OsStr> = cmd.argv.iter().map(OsString::as_os_str).collect();
        // The security boundary: winargv classifies the resolved program
        // (.bat/.cmd get cmd-rules quoting or refusal) and builds the one
        // command line handed to CreateProcessW.
        let line = winargv::build_command_line(&resolved, &args)?;
        let (process, pid) = proc::spawn(
            &line,
            &cmd.cwd,
            &cmd.env,
            [cmd.stdin, cmd.stdout, cmd.stderr],
        )?;
        Ok(Box::new(WindowsChild { process, pid }))
    }

    /// Mechanism-level lookup (RFC v2 §5.4): a name containing a path
    /// separator is tried directly (with PATHEXT completion if it lacks
    /// an extension); a bare name is searched through `PATH`, each entry
    /// tried with PATHEXT. Policy layers live in consumers.
    fn resolve(&self, program: &OsStr) -> Result<OsString> {
        if program.is_empty() {
            return Err(
                PlatformError::new(ErrorKind::InvalidInput, OsCode::None, "resolve")
                    .with_path(program),
            );
        }
        let pathext =
            std::env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(DEFAULT_PATHEXT));
        let is_pathy =
            program.to_string_lossy().contains(['\\', '/']) || Path::new(program).is_absolute();
        if is_pathy {
            if let Some(hit) = resolve_with_pathext(Path::new(program), &pathext) {
                return Ok(hit.into_os_string());
            }
            return Err(
                PlatformError::new(ErrorKind::NotFound, OsCode::None, "resolve").with_path(program),
            );
        }
        if let Some(path_var) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path_var) {
                if dir.as_os_str().is_empty() {
                    continue;
                }
                if let Some(hit) = resolve_with_pathext(&dir.join(program), &pathext) {
                    return Ok(hit.into_os_string());
                }
            }
        }
        Err(PlatformError::new(ErrorKind::NotFound, OsCode::None, "resolve").with_path(program))
    }
}
