//! `Csprng`/`Sandbox` trait impls over the sys layer. No `unsafe` here.

use std::path::Path;

use platform::error::Result;
use platform::security::{Csprng, Sandbox, SandboxStatus};

use crate::sys::security as syssec;

/// The Windows backend's [`Csprng`] capability. Stateless — every call
/// is a fresh `BCryptGenRandom` call, mirroring [`crate::WindowsNet`].
pub struct WindowsCsprng;

impl Csprng for WindowsCsprng {
    fn fill_random(&self, buf: &mut [u8]) -> Result<()> {
        syssec::fill_random(buf)
    }
}

/// The Windows backend's [`Sandbox`] capability. No confinement
/// mechanism exists here yet (restricted tokens/AppContainer have no
/// donor and don't obviously fit an arbitrary, non-packaged process —
/// see `docs/design-discussion-sandbox.md`); every call reports
/// [`SandboxStatus::Unsupported`], never silently claims enforcement.
pub struct WindowsSandbox;

impl Sandbox for WindowsSandbox {
    fn confine_filesystem(
        &self,
        _readable_roots: &[&Path],
        _writable_roots: &[&Path],
    ) -> Result<SandboxStatus> {
        Ok(SandboxStatus::Unsupported)
    }

    fn block_inet_sockets(&self) -> Result<SandboxStatus> {
        Ok(SandboxStatus::Unsupported)
    }
}
