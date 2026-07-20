//! `Csprng` mock: deterministic, not cryptographically secure — trades
//! realism for reproducible tests, the same tradeoff every other mock in
//! this crate makes (RFC v2 §5.1).
//!
//! A fixed-zero or all-`0xFF` fill would let a caller that never actually
//! reads `buf` pass silently, so this generates real (if non-crypto)
//! varying bytes via a small xorshift64* stream, seeded identically every
//! run for reproducibility across test invocations.

use std::cell::Cell;
use std::path::Path;

use platform::error::Result;
use platform::security::{Csprng, Sandbox, SandboxStatus};

/// The mock backend's [`Csprng`] capability. Not thread-safe (`Cell`,
/// like [`crate::net::MockTcpStream`]'s `read_timeout`) — this crate's
/// test doubles have never needed cross-thread sharing.
pub struct MockCsprng {
    state: Cell<u64>,
}

impl Default for MockCsprng {
    fn default() -> Self {
        // Any nonzero seed works for xorshift64*; fixed for reproducibility.
        MockCsprng {
            state: Cell::new(0x9E37_79B9_7F4A_7C15),
        }
    }
}

impl MockCsprng {
    pub fn new() -> Self {
        Self::default()
    }

    fn next_u64(&self) -> u64 {
        let mut x = self.state.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state.set(x);
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

impl Csprng for MockCsprng {
    fn fill_random(&self, buf: &mut [u8]) -> Result<()> {
        for chunk in buf.chunks_mut(8) {
            let bytes = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
        Ok(())
    }
}

/// The mock backend's [`Sandbox`] capability. There is no in-memory
/// equivalent of kernel-level process confinement to fake the way
/// `MockNet`/`MockDir` fake a socket or filesystem — a mock that claimed
/// `Enforced` here would be lying about a security property, worse than
/// not having a mock at all. Every call reports
/// [`SandboxStatus::Unsupported`], the same honest answer a real backend
/// with no confinement mechanism gives (see [`crate::MockCsprng`] for the
/// capability that *is* faithfully mockable).
pub struct MockSandbox;

impl Sandbox for MockSandbox {
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
