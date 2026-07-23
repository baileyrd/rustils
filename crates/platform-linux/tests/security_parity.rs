//! Security parity suite (RFC v2 R5+, D15): behavior-spec-derived
//! assertion set run against every backend, the same shape the Fs/Net
//! suites established.

use std::path::Path;

use platform::security::{CredentialStore, CredentialStoreStatus, Csprng, Sandbox, SandboxStatus};

/// `fill_random` fills the whole buffer, and two consecutive calls don't
/// return the same bytes (the one property every named consumer — a
/// nonce, a confounder — actually relies on: real, non-repeating
/// randomness, not any particular distribution).
fn assert_security_behavior(csprng: &dyn Csprng) {
    let mut a = [0u8; 32];
    csprng.fill_random(&mut a).expect("fill_random");
    assert!(
        a.iter().any(|&b| b != 0),
        "buffer was never actually written"
    );

    let mut b = [0u8; 32];
    csprng.fill_random(&mut b).expect("fill_random");
    assert_ne!(a, b, "two consecutive fills returned identical bytes");

    // A zero-length request is a valid no-op, not an error.
    csprng.fill_random(&mut []).expect("empty fill_random");

    // A request larger than a single getrandom(2)/BCryptGenRandom call
    // reliably fills in one go (the >256-byte chunking case).
    let mut large = [0u8; 4096];
    csprng.fill_random(&mut large).expect("large fill_random");
    assert!(large.iter().any(|&b| b != 0));
}

#[test]
fn mock_security_conforms() {
    assert_security_behavior(&platform_mock::MockCsprng::new());
}

#[cfg(target_os = "linux")]
#[test]
fn linux_security_conforms() {
    assert_security_behavior(&platform_linux::LinuxCsprng);
}

/// The mock `Sandbox` has no in-memory equivalent of kernel confinement
/// to fake — see `platform-mock`'s own doc comment — so the only
/// contract this backend has is honesty: report `Unsupported`, never
/// silently claim enforcement. Real Landlock/seccomp enforcement is
/// exercised separately, in `tests/security_sandbox.rs` (irreversible
/// for the calling thread, so it needs subprocess isolation this shared
/// parity-suite binary doesn't give it).
#[test]
fn mock_sandbox_reports_unsupported() {
    let sandbox = platform_mock::MockSandbox;
    let root: &Path = Path::new(".");
    assert_eq!(
        sandbox.confine_filesystem(&[root], &[]).unwrap(),
        SandboxStatus::Unsupported
    );
    assert_eq!(
        sandbox.block_inet_sockets().unwrap(),
        SandboxStatus::Unsupported
    );
}

/// A faithful `CredentialStore` fake (mock, or a real-and-reachable
/// native backend): round-trips a stored secret, distinguishes accounts
/// under the same service, and reports a clean miss for nothing stored.
fn assert_credential_store_behavior(store: &dyn CredentialStore) {
    assert_eq!(store.available(), CredentialStoreStatus::Available);
    assert_eq!(store.get("rustils-test-svc", "alice").unwrap(), None);

    store
        .set("rustils-test-svc", "alice", b"alice-secret")
        .unwrap();
    store.set("rustils-test-svc", "bob", b"bob-secret").unwrap();
    assert_eq!(
        store.get("rustils-test-svc", "alice").unwrap(),
        Some(b"alice-secret".to_vec())
    );
    assert_eq!(
        store.get("rustils-test-svc", "bob").unwrap(),
        Some(b"bob-secret".to_vec())
    );

    store
        .set("rustils-test-svc", "alice", b"new-secret")
        .unwrap();
    assert_eq!(
        store.get("rustils-test-svc", "alice").unwrap(),
        Some(b"new-secret".to_vec())
    );
}

#[test]
fn mock_credential_store_conforms() {
    assert_credential_store_behavior(&platform_mock::MockCredentialStore::new());
}

/// rustils#78's real Linux backend (Secret Service over D-Bus), run in
/// this suite's own environment where no D-Bus session bus is reachable
/// (no `DBUS_SESSION_BUS_ADDRESS`, no daemon running): `available()`
/// reports `Unavailable` — a real mechanism exists on this OS, it's just
/// not reachable right now — and `get`/`set` surface that as a real
/// `Err` rather than a silent `Ok(None)`/`Ok(())`, per the trait's own
/// contract (a clean miss and "the store isn't reachable" are different
/// claims). Live round-trip coverage against a real, reachable Secret
/// Service lives in `tests/secret_service.rs`, which spawns its own
/// `dbus-daemon`/`gnome-keyring-daemon` pair.
#[cfg(target_os = "linux")]
#[test]
fn linux_credential_store_reports_unavailable_with_no_bus_reachable() {
    let store = platform_linux::LinuxCredentialStore;
    assert_eq!(store.available(), CredentialStoreStatus::Unavailable);
    assert!(store.get("svc", "acct").is_err());
    assert!(store.set("svc", "acct", b"secret").is_err());
}
