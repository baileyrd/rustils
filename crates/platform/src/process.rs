//! Process types and spawning trait (RFC v2 §5.4, decision D-7).
//!
//! What is here now is the surface the reference consumer (`coreutils`)
//! and the parity suite need: the spawn specification types, a uniformly
//! *decoded* [`ExitStatus`], and the object-safe [`Spawner`]/[`Child`]
//! pair with consuming `wait` (double-wait is unrepresentable by
//! construction — the v1 scaffold's use-after-close class cannot be
//! written against this API).
//!
//! What is deliberately NOT here yet (RFC v2 §5.6): the reactor
//! (wait-any), process groups, pipes, and pty. Those shapes are contracted
//! to arrive from rush at the R2 hoist with their semantics already proven;
//! designing them speculatively here is exactly what the consumer gate
//! (§3) forbids.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};

use crate::error::Result;

/// How the child's environment is constructed.
#[derive(Debug, Clone, Default)]
pub enum EnvSpec {
    /// Inherit the parent environment unchanged.
    #[default]
    Inherit,
    /// Start empty; only the given variables are set.
    Explicit(BTreeMap<OsString, OsString>),
}

/// Stdio wiring for the child.
///
/// Minimal on purpose: `Inherit` and `Null` cover the reference consumer.
/// Pipe wiring arrives with the R2 hoist alongside the reactor that makes
/// it usable without deadlocks.
#[derive(Debug, Clone, Copy, Default)]
pub enum Stdio {
    #[default]
    Inherit,
    Null,
}

/// Process-group placement for the child (RFC v2 §5.4 — groups are
/// first-class). Maps to `setpgid`-at-spawn on unix and a kill-on-close
/// Job Object joined before the first instruction on Windows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GroupSpec {
    /// Stay in the parent's group / no job object.
    #[default]
    Inherit,
    /// Lead a fresh group: the child (and everything it spawns) becomes
    /// [`Child::kill_tree`]'s target. On Windows the job is
    /// kill-on-close, so dropping the child without waiting terminates
    /// the tree (see `docs/behavior/process.md`).
    NewGroup,
}

/// A fully-specified spawn request.
///
/// Built with [`Command`], executed by a [`Spawner`]. `argv` is a list of
/// discrete arguments end to end; any joining or quoting an OS requires is
/// backend-internal and never caller-visible (the Windows backend's quoting
/// module is the security boundary here — RFC v2 §5.4).
#[derive(Debug, Clone)]
pub struct Command {
    pub program: OsString,
    pub argv: Vec<OsString>,
    /// Working directory for the child. Always explicit — there is no
    /// "inherit ambient cwd" variant, by design: consumers own their cwd
    /// policy (rush virtualizes it; RFC v2 §5.3 rationale).
    pub cwd: OsString,
    pub env: EnvSpec,
    pub stdin: Stdio,
    pub stdout: Stdio,
    pub stderr: Stdio,
    pub group: GroupSpec,
}

impl Command {
    pub fn new(program: impl Into<OsString>, cwd: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            argv: Vec::new(),
            cwd: cwd.into(),
            env: EnvSpec::default(),
            stdin: Stdio::default(),
            stdout: Stdio::default(),
            stderr: Stdio::default(),
            group: GroupSpec::default(),
        }
    }

    #[must_use]
    pub fn arg(mut self, a: impl Into<OsString>) -> Self {
        self.argv.push(a.into());
        self
    }

    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.argv.extend(args.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn env(mut self, env: EnvSpec) -> Self {
        self.env = env;
        self
    }

    #[must_use]
    pub fn group(mut self, group: GroupSpec) -> Self {
        self.group = group;
        self
    }
}

/// How a child terminated — decoded uniformly on every backend.
///
/// Linux backends decode the raw `waitpid` status word (`WIFEXITED` /
/// `WIFSIGNALED`); Windows backends report the exit code. A raw status
/// word must never cross this boundary (pins v1 scaffold bug B-5; the
/// parity suite's permanent sentinel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    /// Normal exit with the given code.
    Code(i32),
    /// Terminated by a signal (unix); never produced on Windows.
    Signaled(i32),
}

impl ExitStatus {
    pub fn success(self) -> bool {
        matches!(self, ExitStatus::Code(0))
    }
}

/// A spawned child. Object-safe.
pub trait Child {
    /// Wait for termination, consuming the child.
    ///
    /// Consuming `self` makes double-wait — and therefore the
    /// wait-after-close bug class — unrepresentable.
    fn wait(self: Box<Self>) -> Result<ExitStatus>;

    /// OS process identifier, for display/diagnostics.
    fn id(&self) -> u32;

    /// Forcibly terminate the child's whole group (the child and every
    /// descendant). Requires [`GroupSpec::NewGroup`] at spawn — on a
    /// child spawned with `Inherit` this fails `Unsupported` rather than
    /// guessing at a target (killing the parent's own group is the
    /// alternative, and it is never what the caller meant). The child
    /// must still be `wait`ed to observe the resulting status; the form
    /// that status takes is OS-divergent (divergence 001).
    fn kill_tree(&self) -> Result<()>;

    /// Forcibly terminate the child process only — descendants survive.
    fn kill_single(&self) -> Result<()>;

    /// Non-blocking poll: `Some(status)` if the child has terminated,
    /// `None` if it is still running. A child that has reported a status
    /// here keeps reporting the same status (backends stash the reaped
    /// result), and the consuming [`Child::wait`] afterwards returns it —
    /// polling never loses the exit status.
    fn try_wait(&mut self) -> Result<Option<ExitStatus>>;
}

/// Block until *some* child in `children` terminates, for up to `timeout`
/// (`None` = wait forever). Returns `Some(index)` of a terminated child —
/// retrieve the status via that child's [`Child::try_wait`]/[`Child::wait`]
/// — or `None` on timeout. An empty slice is `InvalidInput` (mirrors the
/// OS primitives' own refusal).
///
/// This is the wait-any *seed* (extraction map step 3): a portable
/// poll-over-`try_wait` loop with a 10ms tick — the same coarser stand-in
/// rush ran before adopting `WaitForMultipleObjects`, correct on every
/// backend including the mock. The OS-multiplexed reactor (pidfd+poll on
/// Linux; `WaitForMultipleObjects` with the 64-handle limit absorbed
/// internally, RFC v2 §5.6) replaces this loop's internals at R3 without
/// changing this contract.
pub fn wait_any(
    children: &mut [Box<dyn Child>],
    timeout: Option<std::time::Duration>,
) -> Result<Option<usize>> {
    use crate::error::{ErrorKind, OsCode, PlatformError};
    if children.is_empty() {
        return Err(PlatformError::new(
            ErrorKind::InvalidInput,
            OsCode::None,
            "wait_any",
        ));
    }
    let start = std::time::Instant::now();
    const TICK: std::time::Duration = std::time::Duration::from_millis(10);
    loop {
        for (i, child) in children.iter_mut().enumerate() {
            if child.try_wait()?.is_some() {
                return Ok(Some(i));
            }
        }
        let elapsed = start.elapsed();
        let sleep = match timeout {
            Some(limit) if elapsed >= limit => return Ok(None),
            Some(limit) => TICK.min(limit - elapsed),
            None => TICK,
        };
        std::thread::sleep(sleep);
    }
}

/// A backend capable of spawning processes. Object-safe.
pub trait Spawner {
    fn spawn(&self, cmd: &Command) -> Result<Box<dyn Child>>;

    /// Resolve `program` against this backend's executable-lookup rules
    /// (PATH + exec bit on unix; PATH + PATHEXT on Windows). Mechanism
    /// only — policy layers (e.g. a shell's builtin/function precedence)
    /// live in the consumer.
    fn resolve(&self, program: &OsStr) -> Result<OsString>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_builder_composes() {
        let c = Command::new("prog", "/work").arg("a").args(["b", "c"]);
        assert_eq!(c.argv.len(), 3);
        assert_eq!(c.cwd, OsString::from("/work"));
    }

    #[test]
    fn exit_status_is_decoded_semantics() {
        assert!(ExitStatus::Code(0).success());
        assert!(!ExitStatus::Code(1).success());
        assert!(!ExitStatus::Signaled(9).success());
    }
}
