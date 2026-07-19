//! Scripted process backend: spawn requests are matched against
//! pre-registered scripts and produce deterministic results. This is how
//! consumer process-orchestration logic is tested with zero real processes.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::sync::{Arc, Mutex};

use platform::error::{ErrorKind, OsCode, PlatformError, Result};
use platform::process::{Child, Command, ExitStatus, GroupSpec, Spawner, Stdio};

/// A scripted response for a program name.
#[derive(Debug, Clone)]
pub struct Script {
    pub status: ExitStatus,
    /// Bytes served through `take_stdout` when the spawn piped stdout.
    pub stdout: Vec<u8>,
}

/// In-memory pipe end: reads drain the buffer; writes are accepted and
/// discarded (a scripted child consumes stdin without observable effect).
struct MemPipe {
    data: Vec<u8>,
    pos: usize,
}

impl platform::fs::File for MemPipe {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = buf.len().min(self.data.len() - self.pos);
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn sync_all(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Spawner whose children terminate exactly as scripted.
#[derive(Default)]
pub struct MockSpawner {
    scripts: BTreeMap<OsString, Script>,
    /// Log of every spawn request, for assertions.
    pub spawned: Arc<Mutex<Vec<Command>>>,
}

impl MockSpawner {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn script(mut self, program: impl Into<OsString>, status: ExitStatus) -> Self {
        self.scripts.insert(
            program.into(),
            Script {
                status,
                stdout: Vec::new(),
            },
        );
        self
    }

    #[must_use]
    pub fn script_with_output(
        mut self,
        program: impl Into<OsString>,
        status: ExitStatus,
        stdout: impl Into<Vec<u8>>,
    ) -> Self {
        self.scripts.insert(
            program.into(),
            Script {
                status,
                stdout: stdout.into(),
            },
        );
        self
    }
}

struct MockChild {
    status: ExitStatus,
    own_group: bool,
    stdin: Option<Box<dyn platform::fs::File>>,
    stdout: Option<Box<dyn platform::fs::File>>,
    stderr: Option<Box<dyn platform::fs::File>>,
}

impl Child for MockChild {
    fn wait(self: Box<Self>) -> Result<ExitStatus> {
        Ok(self.status)
    }

    fn id(&self) -> u32 {
        0
    }

    /// Scripted children have already "finished"; kill succeeds without
    /// changing the scripted status. The `NewGroup` precondition is
    /// enforced exactly like the native backends so consumer logic can be
    /// tested against it.
    fn kill_tree(&self) -> Result<()> {
        if self.own_group {
            Ok(())
        } else {
            Err(PlatformError::new(
                ErrorKind::Unsupported,
                OsCode::None,
                "kill_tree",
            ))
        }
    }

    fn kill_single(&self) -> Result<()> {
        Ok(())
    }

    /// Scripted children have already terminated by construction.
    fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        Ok(Some(self.status))
    }

    fn take_stdin(&mut self) -> Option<Box<dyn platform::fs::File>> {
        self.stdin.take()
    }

    fn take_stdout(&mut self) -> Option<Box<dyn platform::fs::File>> {
        self.stdout.take()
    }

    fn take_stderr(&mut self) -> Option<Box<dyn platform::fs::File>> {
        self.stderr.take()
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn mem_pipe(data: Vec<u8>) -> Box<dyn platform::fs::File> {
    Box::new(MemPipe { data, pos: 0 })
}

impl Spawner for MockSpawner {
    fn spawn(&self, cmd: &Command) -> Result<Box<dyn Child>> {
        self.spawned.lock().expect("mock lock").push(cmd.clone());
        let script = self.scripts.get(&cmd.program).ok_or_else(|| {
            PlatformError::new(ErrorKind::NotFound, OsCode::None, "spawn")
                .with_path(cmd.program.clone())
        })?;
        Ok(Box::new(MockChild {
            status: script.status,
            own_group: cmd.group == GroupSpec::NewGroup,
            stdin: (cmd.stdin == Stdio::Pipe).then(|| mem_pipe(Vec::new())),
            stdout: (cmd.stdout == Stdio::Pipe).then(|| mem_pipe(script.stdout.clone())),
            stderr: (cmd.stderr == Stdio::Pipe).then(|| mem_pipe(Vec::new())),
        }))
    }

    fn resolve(&self, program: &OsStr) -> Result<OsString> {
        if self.scripts.contains_key(program) {
            Ok(program.to_os_string())
        } else {
            Err(PlatformError::new(ErrorKind::NotFound, OsCode::None, "resolve").with_path(program))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_child_reports_decoded_status() {
        let spawner = MockSpawner::new().script("true", ExitStatus::Code(0));
        let child = spawner.spawn(&Command::new("true", "/")).expect("spawn");
        assert!(child.wait().expect("wait").success());
    }

    #[test]
    fn wait_consumes_the_child() {
        // Compile-time property: `wait(self: Box<Self>)` means a second
        // wait cannot be written. This test documents the intent; the type
        // system is the enforcement (pins v1 scaffold bug B-4).
        let spawner = MockSpawner::new().script("x", ExitStatus::Code(1));
        let child = spawner.spawn(&Command::new("x", "/")).expect("spawn");
        let _status = child.wait().expect("wait");
        // `child.wait()` again would not compile.
    }

    #[test]
    fn kill_tree_requires_new_group() {
        let spawner = MockSpawner::new().script("x", ExitStatus::Code(0));
        let inherit = spawner.spawn(&Command::new("x", "/")).expect("spawn");
        assert_eq!(
            inherit.kill_tree().expect_err("must refuse").kind,
            ErrorKind::Unsupported
        );
        let grouped = spawner
            .spawn(&Command::new("x", "/").group(GroupSpec::NewGroup))
            .expect("spawn");
        grouped.kill_tree().expect("kill_tree with NewGroup");
        grouped.kill_single().expect("kill_single always works");
    }

    #[test]
    fn spawn_log_supports_assertions() {
        let spawner = MockSpawner::new().script("prog", ExitStatus::Code(0));
        let log = spawner.spawned.clone();
        spawner
            .spawn(&Command::new("prog", "/work").arg("--flag"))
            .expect("spawn");
        let spawned = log.lock().expect("mock lock");
        assert_eq!(spawned.len(), 1);
        assert_eq!(spawned[0].cwd, OsString::from("/work"));
    }
}
