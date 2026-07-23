//! Cryptographically secure randomness (RFC v2 R5+, decision D15), OS
//! credential storage (D15, Phase 6 item 2), and process sandbox policy
//! (D15, Phase 6 item 3) — the Security surface's three slices.
//! `CredentialStore` stayed donor-only for a full session after `Sandbox`
//! landed (see `docs/design-discussion-sandbox.md` for why item 3 went
//! ahead without a confirmed live consumer while item 2 didn't) before
//! landing here too, split across rustils#76/#77/#78 given its size and
//! the real, unverified-in-CI-yet Linux implementation.
//!
//! `Csprng` was unparked only once a named consumer existed to define the
//! shape (RFC v2 §3's consumer gate): rusty_rdp hand-rolls five separate
//! `/dev/urandom` reads for its Kerberos nonces and confounders and its
//! CredSSP exchanges (`krb5::kdc`, `tls`) — a single `fill_random`
//! primitive retired all five. This slice is deliberately narrow — one
//! method, no key derivation, no algorithm choice — because that's all
//! the named consumer needed.
//!
//! Backends draw from the OS CSPRNG directly (Linux: the `getrandom(2)`
//! syscall; Windows: `BCryptGenRandom`) rather than opening `/dev/urandom`
//! as a file — avoiding an `fd` a caller running under `Sandbox`
//! confinement might otherwise have denied.
//!
//! `Sandbox` has no confirmed live consumer as of this writing — see
//! `docs/design-discussion-sandbox.md`'s open questions before assuming
//! nexus (the donor whose shape this mirrors) will actually converge onto
//! it. Built anyway, deliberately, as the owner's explicit call: nexus's
//! `os_sandbox.rs` is the closest thing this repo has to a validated
//! design for this capability, so this trait mirrors its shape exactly
//! rather than inventing a new one — two independently-degradable calls
//! (filesystem confinement via Landlock, network-socket confinement via
//! seccomp), not one combined call, because that's what nexus's own
//! implementation proved necessary and sufficient. shh's privilege-
//! separation pattern (fork + socketpair + credential drop to protect a
//! secret) is a different problem — process-boundary isolation, not
//! capability confinement — and doesn't fit this trait or
//! `platform::process`'s current shape; it stays out of scope here.

use std::path::Path;

use crate::error::Result;

/// A source of cryptographically secure random bytes.
pub trait Csprng {
    /// Fill `buf` entirely with random bytes, blocking if the OS CSPRNG
    /// isn't yet seeded (practically instantaneous after early boot; no
    /// consumer here runs early enough in the boot sequence for that to
    /// matter).
    fn fill_random(&self, buf: &mut [u8]) -> Result<()>;
}

/// How reachable a [`CredentialStore`]'s backing secret store actually is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialStoreStatus {
    /// The backing store is reachable and ready to use.
    Available,
    /// This backend has a real secret-store mechanism, but it isn't
    /// reachable right now (e.g. Linux: no D-Bus session bus, no Secret
    /// Service provider registered on it, or the default collection
    /// can't be unlocked non-interactively) — mirrors
    /// [`SandboxStatus::NotEnforced`]: a real capability that didn't
    /// take effect this time, not "no such concept."
    Unavailable,
    /// This backend has no secret-store mechanism at all — every
    /// backend/platform with nothing to try, and
    /// [`NullCredentialStore`]'s explicit opt-out.
    Unsupported,
}

/// An OS-native (or explicitly disabled) secret store: get/set a secret
/// by `(service, account)`, and check reachability before relying on it
/// (RFC v2 R5+, D15, Phase 6 item 2 — modeled on nexus's `keyring`-backed
/// `CredentialVault`, held pending a consumer decision until this
/// landed). No `delete` — not part of the roadmap's documented scope,
/// deliberately excluded here rather than freelanced (rustils#76); and
/// no key derivation, rotation, or attribute schema beyond
/// `service`/`account` — matching [`Csprng`]'s own "narrow, no more than
/// a named need requires" discipline.
pub trait CredentialStore {
    /// Check whether the backing store is currently reachable, without
    /// attempting a real operation. For the disabled-mode escape hatch,
    /// construct [`NullCredentialStore`] directly rather than probing a
    /// real backend's `available()` — this method tells a *real*
    /// backend's transient reachability apart from permanent unsupport;
    /// it is not itself the opt-out mechanism.
    fn available(&self) -> CredentialStoreStatus;

    /// The stored secret for `(service, account)`, or `None` if nothing
    /// is stored under that name. `Err` only for a real failure (the
    /// store became unreachable mid-call, a malformed stored value,
    /// etc.) — a clean "nothing stored here" is `Ok(None)`, never an
    /// error a caller has to specifically match on.
    fn get(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>>;

    /// Store `secret` under `(service, account)`, replacing any existing
    /// value stored under that same name.
    fn set(&self, service: &str, account: &str, secret: &[u8]) -> Result<()>;
}

/// The disabled-mode escape hatch (Phase 6 item 2): a caller that wants
/// to opt out of OS keyring integration entirely constructs this instead
/// of a real backend, rather than every consumer re-deriving its own
/// "credentials disabled" branch. `available()` is always `Unsupported`;
/// `get` is always `Ok(None)`; `set` is accepted and silently discarded —
/// an explicit, honest no-op the caller chose, not a hidden failure.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullCredentialStore;

impl CredentialStore for NullCredentialStore {
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

/// How thoroughly a [`Sandbox`] call actually took effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxStatus {
    /// The kernel enforced the requested confinement.
    Enforced,
    /// This backend has a real confinement mechanism, but the running
    /// kernel is too old (or otherwise lacks the feature) to enforce it —
    /// the call did not error, but nothing is actually confined. Mirrors
    /// nexus's own `NotEnforced`: a caller that doesn't check this value
    /// runs unconfined without knowing it, the exact risk
    /// `docs/design-discussion-sandbox.md` names as unresolved.
    NotEnforced,
    /// This backend has no confinement mechanism for this capability at
    /// all (every non-Linux backend, for both methods, today).
    Unsupported,
}

/// Process-level capability confinement (Landlock + seccomp on Linux).
/// Every method here is irreversible for the calling thread once it
/// returns `Ok(SandboxStatus::Enforced)` — there is no corresponding
/// "widen back" call, matching the kernel primitives themselves.
///
/// Nexus's own design applies this from a dedicated, deliberately
/// single-threaded helper process that confines itself and then `exec`s
/// the real target — installing Landlock/seccomp after `fork()` in a
/// multithreaded process is unsafe (ruleset/BPF construction allocates,
/// and another thread may hold the allocator lock at fork time). This
/// trait exposes the primitive only; building and shipping that
/// helper-process pattern is the caller's responsibility, not this PAL's
/// (see `docs/design-discussion-sandbox.md` question 2).
pub trait Sandbox {
    /// Deny all filesystem access except read+execute under
    /// `readable_roots` and read+write+create+delete under
    /// `writable_roots`. Call from a single-threaded context (see the
    /// trait doc comment).
    fn confine_filesystem(
        &self,
        readable_roots: &[&Path],
        writable_roots: &[&Path],
    ) -> Result<SandboxStatus>;

    /// Deny opening new `AF_INET`/`AF_INET6`/`AF_PACKET` sockets from the
    /// calling thread onward. Already-open sockets are unaffected —
    /// existing connections keep working, only new raw-internet socket
    /// creation is denied. `AF_UNIX` and every other syscall are
    /// untouched, mirroring nexus's own narrow scope (this is "no new
    /// internet sockets," not a general syscall allowlist).
    fn block_inet_sockets(&self) -> Result<SandboxStatus>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_credential_store_is_an_honest_no_op() {
        let store = NullCredentialStore;
        assert_eq!(store.available(), CredentialStoreStatus::Unsupported);
        assert_eq!(store.get("svc", "acct").unwrap(), None);
        store.set("svc", "acct", b"secret").unwrap();
        // Still nothing stored — `set` discarded it, as documented.
        assert_eq!(store.get("svc", "acct").unwrap(), None);
    }
}
