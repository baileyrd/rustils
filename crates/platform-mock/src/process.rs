//! Scripted process backend: spawn requests are matched against
//! pre-registered scripts and produce deterministic results. This is how
//! consumer process-orchestration logic is tested with zero real processes.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::sync::{Arc, Mutex};

use platform::error::{ErrorKind, OsCode, PlatformError, Result};
use platform::process::{Child, Command, ExitStatus, Spawner};

/// A scripted response for a program name.
#[derive(Debug, Clone)]
pub struct Script {
    pub status: ExitStatus,
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
        self.scripts.insert(program.into(), Script { status });
        self
    }
}

struct MockChild {
    status: ExitStatus,
}

impl Child for MockChild {
    fn wait(self: Box<Self>) -> Result<ExitStatus> {
        Ok(self.status)
    }

    fn id(&self) -> u32 {
        0
    }
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
