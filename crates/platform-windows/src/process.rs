//! `Spawner`/`Child` impls over the sys layer (RFC v2 §5.4; extraction
//! map step 2, first slice — spawn/wait/resolve; Job-Object groups and
//! kill-tree follow with suspended spawn). No OS-level code here; the
//! command line comes exclusively from [`crate::winargv`].

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use platform::error::{ErrorKind, OsCode, PlatformError, Result};
use platform::process::{Child, Command, ExitStatus, GroupSpec, Spawner};

use crate::sys::handle::OwnedWinHandle;
use crate::sys::proc;
use crate::winargv;

/// The Windows process backend.
#[derive(Debug, Default)]
pub struct WindowsSpawner;

/// A spawned child; `wait` consumes it (double-wait unrepresentable).
/// A `NewGroup` child holds its kill-on-close Job Object handle: dropping
/// the child without waiting terminates the whole tree (pinned in
/// `docs/behavior/process.md`; the disown-style detach that reverses
/// kill-on-close arrives with rush adoption — extraction map D2).
pub struct WindowsChild {
    process: OwnedWinHandle,
    job: Option<OwnedWinHandle>,
    pid: u32,
    reaped: Option<ExitStatus>,
    /// Parent pipe ends for `Stdio::Pipe` slots, until taken.
    pipes: proc::ParentPipes,
}

impl Child for WindowsChild {
    fn wait(self: Box<Self>) -> Result<ExitStatus> {
        match self.reaped {
            Some(status) => Ok(status),
            None => proc::wait(&self.process),
        }
    }

    fn id(&self) -> u32 {
        self.pid
    }

    fn kill_tree(&self) -> Result<()> {
        match &self.job {
            Some(job) => proc::terminate_job(job),
            None => Err(PlatformError::new(
                ErrorKind::Unsupported,
                OsCode::None,
                "kill_tree",
            )),
        }
    }

    fn kill_single(&self) -> Result<()> {
        proc::terminate_process(&self.process)
    }

    fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        // No reap semantics on Windows — the handle stays valid and the
        // exit code re-readable — but stash anyway so the trait contract
        // ("keeps reporting the same status") holds by construction.
        if self.reaped.is_none() {
            self.reaped = proc::try_wait(&self.process)?;
        }
        Ok(self.reaped)
    }

    fn take_stdin(&mut self) -> Option<Box<dyn platform::fs::File>> {
        self.pipes[0]
            .take()
            .map(|h| Box::new(crate::fs::WindowsFile::from(h)) as Box<dyn platform::fs::File>)
    }

    fn take_stdout(&mut self) -> Option<Box<dyn platform::fs::File>> {
        self.pipes[1]
            .take()
            .map(|h| Box::new(crate::fs::WindowsFile::from(h)) as Box<dyn platform::fs::File>)
    }

    fn take_stderr(&mut self) -> Option<Box<dyn platform::fs::File>> {
        self.pipes[2]
            .take()
            .map(|h| Box::new(crate::fs::WindowsFile::from(h)) as Box<dyn platform::fs::File>)
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
        let (process, job, pid, pipes) = proc::spawn(
            &line,
            &cmd.cwd,
            &cmd.env,
            [cmd.stdin, cmd.stdout, cmd.stderr],
            cmd.group == GroupSpec::NewGroup,
        )?;
        Ok(Box::new(WindowsChild {
            process,
            job,
            pid,
            reaped: None,
            pipes,
        }))
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
