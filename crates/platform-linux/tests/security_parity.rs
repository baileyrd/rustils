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

/// rustils#76's Linux stub: the real Secret Service implementation is
/// #77/#78's own slice, not this one. Reports `Unsupported` and a clean
/// `Ok(None)`/`Ok(())`, never an `Err`, for `get`/`set` — a caller that
/// checks `available()` first never hits a surprising failure.
#[cfg(target_os = "linux")]
#[test]
fn linux_credential_store_stub_reports_unsupported() {
    let store = platform_linux::LinuxCredentialStore;
    assert_eq!(store.available(), CredentialStoreStatus::Unsupported);
    assert_eq!(store.get("svc", "acct").unwrap(), None);
    store.set("svc", "acct", b"secret").unwrap();
    // Still nothing stored — this stub discards `set`, as documented.
    assert_eq!(store.get("svc", "acct").unwrap(), None);
}
