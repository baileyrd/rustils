//! `Pty`/`PtyMaster` trait impls over the sys layer (RFC v2 R5+, D13,
//! convergence roadmap Phase 7, rustils#83, part 2/2 following #82's
//! Linux backend). No `unsafe` here.
//!
//! `WindowsPtyMaster` holds two separate handles (`input`: the master
//! writes here; `output`: the master reads here) rather than one, unlike
//! `LinuxPtyMaster`'s single bidirectional fd — ConPTY's master side is
//! genuinely a pair of anonymous pipes, not one descriptor. There is no
//! honest single-handle `AsHandle`/`AsRawHandle` impl to offer the way
//! `LinuxPtyMaster` ships `AsFd`/`AsRawFd`, so this type exposes two
//! named accessors instead ([`WindowsPtyMaster::input_handle`]/
//! [`WindowsPtyMaster::output_handle`]).

use std::ffi::{OsStr, OsString};

use platform::error::{ErrorKind, OsCode, PlatformError, Result};
use platform::process::{Child, Command, GroupSpec, Spawner};
use platform::pty::{Pty, PtyMaster};
use platform::term::WinSize;

use crate::ffi::win32_surface as w;
use crate::process::WindowsChild;
use crate::sys::handle::OwnedWinHandle;
use crate::sys::pty as syspty;

/// The Windows backend's [`Pty`] capability. Stateless, mirroring
/// [`crate::WindowsSpawner`].
pub struct WindowsPty;

impl Pty for WindowsPty {
    fn spawn(&self, cmd: &Command, size: WinSize) -> Result<(Box<dyn PtyMaster>, Box<dyn Child>)> {
        // Same contract #82's Linux backend enforces
        // (`platform::pty::Pty::spawn`'s own doc): a pty-hosted child is
        // unconditionally a fresh session/group, so there is no way to
        // also honor a request to join a different, already-existing
        // group.
        if matches!(cmd.group, GroupSpec::JoinGroup(_)) {
            return Err(PlatformError::new(
                ErrorKind::InvalidInput,
                OsCode::None,
                "Pty::spawn: GroupSpec::JoinGroup is incompatible with a fresh pty session",
            ));
        }

        let resolved = crate::WindowsSpawner.resolve(&cmd.program)?;
        let args: Vec<&OsStr> = cmd.argv.iter().map(OsString::as_os_str).collect();
        // The security boundary: winargv classifies the resolved program
        // and builds the one command line handed to `CreateProcessW`,
        // exactly like `WindowsSpawner::spawn` already does.
        let line = crate::winargv::build_command_line(&resolved, &args)?;

        let (hpc, input, output) = syspty::create_pty(size)?;
        match syspty::spawn_attached(hpc, &line, &cmd.cwd, &cmd.env) {
            Ok((process, job, pid)) => {
                let child = WindowsChild::from_parts(process, job, pid);
                Ok((
                    Box::new(WindowsPtyMaster { hpc, input, output }),
                    Box::new(child),
                ))
            }
            Err(e) => {
                // The pseudo console outlived a failed spawn attempt —
                // tear it down here rather than leaking it.
                syspty::close(hpc, &output);
                Err(e)
            }
        }
    }
}

/// A spawned pty pair's master side. Public for std/reactor interop (RFC
/// v2 §5.1), the same reasoning `LinuxTcpStream`/`LinuxTunDevice`/
/// `LinuxPtyMaster` document — though see this module's doc comment for
/// why the escape hatch here is two named accessors, not `AsHandle`.
pub struct WindowsPtyMaster {
    hpc: w::HPCON,
    input: OwnedWinHandle,
    output: OwnedWinHandle,
}

impl WindowsPtyMaster {
    /// The handle the master writes keystrokes/input to. Not pollable
    /// the way a socket handle is (anonymous pipes don't support
    /// `WaitForMultipleObjects`-style readiness signaling) — a consumer
    /// that needs that would have to bridge this onto a reactor with its
    /// own thread, the same "blocking-thread bridge" divergence D13's
    /// own text already names as expected on Windows.
    pub fn input_handle(&self) -> &OwnedWinHandle {
        &self.input
    }

    /// The handle the master reads the child's output from. Same
    /// non-pollable caveat as [`WindowsPtyMaster::input_handle`].
    pub fn output_handle(&self) -> &OwnedWinHandle {
        &self.output
    }
}

impl PtyMaster for WindowsPtyMaster {
    fn read(&self, buf: &mut [u8]) -> Result<usize> {
        crate::sys::fileio::read(&self.output, buf)
    }

    fn write(&self, buf: &[u8]) -> Result<usize> {
        crate::sys::fileio::write(&self.input, buf)
    }

    fn resize(&self, size: WinSize) -> Result<()> {
        syspty::resize(self.hpc, size)
    }
}

impl Drop for WindowsPtyMaster {
    fn drop(&mut self) {
        // See `sys::pty`'s own doc comment: draining `output` before
        // `ClosePseudoConsole` avoids a real deadlock (conhost's
        // internal writer can block against an un-drained pipe, and
        // `ClosePseudoConsole` blocks until that writer finishes).
        syspty::close(self.hpc, &self.output);
    }
}
