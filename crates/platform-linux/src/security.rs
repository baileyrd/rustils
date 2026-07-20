//! `Csprng`/`Sandbox` trait impls over the sys layer. No `unsafe` here.

use std::path::Path;

use platform::error::Result;
use platform::security::{Csprng, Sandbox, SandboxStatus};

use crate::sys::security as syssec;

/// The Linux backend's [`Csprng`] capability. Stateless — every call is
/// a fresh `getrandom(2)` syscall, mirroring [`crate::LinuxNet`].
pub struct LinuxCsprng;

impl Csprng for LinuxCsprng {
    fn fill_random(&self, buf: &mut [u8]) -> Result<()> {
        syssec::fill_random(buf)
    }
}

/// The Linux backend's [`Sandbox`] capability: Landlock for
/// [`Sandbox::confine_filesystem`], seccomp-BPF for
/// [`Sandbox::block_inet_sockets`]. Stateless — every call is a fresh
/// set of syscalls against the calling thread, mirroring
/// [`crate::LinuxNet`]/[`LinuxCsprng`].
pub struct LinuxSandbox;

impl Sandbox for LinuxSandbox {
    fn confine_filesystem(
        &self,
        readable_roots: &[&Path],
        writable_roots: &[&Path],
    ) -> Result<SandboxStatus> {
        syssec::confine_filesystem(readable_roots, writable_roots)
    }

    fn block_inet_sockets(&self) -> Result<SandboxStatus> {
        syssec::block_inet_sockets()
    }
}
