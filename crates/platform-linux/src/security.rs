//! `Csprng`/`CredentialStore`/`Sandbox` trait impls over the sys layer.
//! No `unsafe` here.

use std::path::Path;

use platform::error::Result;
use platform::security::{CredentialStore, CredentialStoreStatus, Csprng, Sandbox, SandboxStatus};

use crate::sys::security as syssec;

/// The Linux backend's [`Csprng`] capability. Stateless — every call is
/// a fresh `getrandom(2)` syscall, mirroring [`crate::LinuxNet`].
pub struct LinuxCsprng;

impl Csprng for LinuxCsprng {
    fn fill_random(&self, buf: &mut [u8]) -> Result<()> {
        syssec::fill_random(buf)
    }
}

/// The Linux backend's [`CredentialStore`] capability (rustils#76): a
/// stub reporting [`CredentialStoreStatus::Unsupported`] and a clean
/// `Ok(None)`/`Ok(())` for `get`/`set` — the real Secret Service
/// (`org.freedesktop.secrets`) implementation is rustils#77/#78's own
/// slice, not part of this one. `get`/`set` are `Ok`, not `Err`: a
/// caller that only ever checks `available()` before deciding whether to
/// rely on stored secrets (the documented contract) never hits an
/// `Err` here it wasn't expecting — matching how a real backend that's
/// merely `Unavailable` right now would also prefer a clean miss over a
/// surprising failure on an operation the caller was told not to trust.
pub struct LinuxCredentialStore;

impl CredentialStore for LinuxCredentialStore {
    fn available(&self) -> CredentialStoreStatus {
        CredentialStoreStatus::Unsupported
    }

    fn get(&self, _service: &str, _account: &str) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }

    fn set(&self, _service: &str, _account: &str, _secret: &[u8]) -> Result<()> {
        Ok(())
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
