//! `Spawner`/`Child` impls over the sys layer (RFC v2 §5.4; extraction
//! map step 2, first slice — spawn/wait/resolve; groups and kill-tree
//! follow). No `unsafe` here.

use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use platform::error::{ErrorKind, OsCode, PlatformError, Result};
use platform::process::{Child, Command, ExitStatus, Spawner};

use crate::ffi::libc_surface as c;
use crate::sys::spawn;

/// The Linux process backend.
#[derive(Debug, Default)]
pub struct LinuxSpawner;

/// A spawned child; `wait` consumes it (double-wait unrepresentable).
pub struct LinuxChild {
    pid: c::pid_t,
}

impl Child for LinuxChild {
    fn wait(self: Box<Self>) -> Result<ExitStatus> {
        spawn::wait(self.pid)
    }

    fn id(&self) -> u32 {
        self.pid as u32
    }
}

fn is_executable_file(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

impl Spawner for LinuxSpawner {
    fn spawn(&self, cmd: &Command) -> Result<Box<dyn Child>> {
        let resolved = self.resolve(&cmd.program)?;
        let pid = spawn::spawn(
            &resolved,
            &cmd.program,
            &cmd.argv,
            &cmd.cwd,
            &cmd.env,
            [cmd.stdin, cmd.stdout, cmd.stderr],
        )?;
        Ok(Box::new(LinuxChild { pid }))
    }

    /// Mechanism-level lookup (RFC v2 §5.4): a name containing `/` is used
    /// as-is; otherwise each `$PATH` entry is searched for a regular file
    /// with any execute bit. Policy layers (builtin precedence, shebang
    /// emulation) live in consumers.
    fn resolve(&self, program: &OsStr) -> Result<OsString> {
        use std::os::unix::ffi::OsStrExt;
        if program.is_empty() {
            return Err(
                PlatformError::new(ErrorKind::InvalidInput, OsCode::None, "resolve")
                    .with_path(program),
            );
        }
        if program.as_bytes().contains(&b'/') {
            let p = Path::new(program);
            if is_executable_file(p) {
                return Ok(program.to_os_string());
            }
            return Err(
                PlatformError::new(ErrorKind::NotFound, OsCode::None, "resolve").with_path(program),
            );
        }
        if let Some(path_var) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path_var) {
                let candidate: PathBuf = dir.join(program);
                if is_executable_file(&candidate) {
                    return Ok(candidate.into_os_string());
                }
            }
        }
        Err(PlatformError::new(ErrorKind::NotFound, OsCode::None, "resolve").with_path(program))
    }
}
