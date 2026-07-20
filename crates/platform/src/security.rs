//! Cryptographically secure randomness (RFC v2 R5+, decision D15) — the
//! first Security surface slice.
//!
//! Unparked only once a named consumer existed to define the shape (RFC
//! v2 §3's consumer gate): rusty_rdp hand-rolls five separate
//! `/dev/urandom` reads for its Kerberos nonces and confounders
//! (`src/krb5/kdc.rs`) — a single `fill_random` primitive retires all
//! five. `CredentialStore` and sandbox policy are later, wider slices of
//! this same decision (see `docs/convergence-roadmap.md`'s Phase 6);
//! this slice is deliberately narrow — one method, no key derivation, no
//! algorithm choice — because that's all the named consumer needs.
//!
//! Backends draw from the OS CSPRNG directly (Linux: the `getrandom(2)`
//! syscall; Windows: `BCryptGenRandom`) rather than opening `/dev/urandom`
//! as a file — avoiding an `fd` a caller running under a restrictive
//! filesystem sandbox (the Phase 6 item 3 Landlock/seccomp policy this
//! surface is heading toward) might otherwise deny.

use crate::error::Result;

/// A source of cryptographically secure random bytes.
pub trait Csprng {
    /// Fill `buf` entirely with random bytes, blocking if the OS CSPRNG
    /// isn't yet seeded (practically instantaneous after early boot; no
    /// consumer here runs early enough in the boot sequence for that to
    /// matter).
    fn fill_random(&self, buf: &mut [u8]) -> Result<()>;
}
