//! `Csprng` mock: deterministic, not cryptographically secure — trades
//! realism for reproducible tests, the same tradeoff every other mock in
//! this crate makes (RFC v2 §5.1).
//!
//! A fixed-zero or all-`0xFF` fill would let a caller that never actually
//! reads `buf` pass silently, so this generates real (if non-crypto)
//! varying bytes via a small xorshift64* stream, seeded identically every
//! run for reproducibility across test invocations.

use std::cell::Cell;

use platform::error::Result;
use platform::security::Csprng;

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
