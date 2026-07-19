//! Parity suite, Windows leg (RFC v2 §9): the same behavior-spec-derived
//! assertion set as `platform-linux/tests/parity.rs`, run against the
//! Windows backend and the mock. Kept textually identical to the Linux
//! copy's `assert_fs_behavior` on purpose — extraction into a shared crate
//! is the recorded follow-up once a third backend would otherwise mean a
//! third copy (see `docs/behavior/fs.md`).

#![cfg(windows)]

use std::ffi::OsStr;

use platform::error::ErrorKind;
use platform::fs::{Dir, FileType, OpenOptions};

/// The shared assertions. Grows with `docs/behavior/fs.md`.
fn assert_fs_behavior(root: &dyn Dir) {
    // create → write → read round-trip, byte-faithful
    let mut f = root
        .open(OsStr::new("a.bin"), &OpenOptions::create_truncate())
        .expect("create");
    f.write(b"one \xff two").expect("write");
    drop(f);
    let mut f = root
        .open(OsStr::new("a.bin"), &OpenOptions::read())
        .expect("open");
    let mut buf = [0u8; 64];
    let n = f.read(&mut buf).expect("read");
    assert_eq!(&buf[..n], b"one \xff two");

    // metadata: type and length agree with what was written
    let md = root.metadata(OsStr::new("a.bin")).expect("metadata");
    assert_eq!(md.file_type, FileType::File);
    assert_eq!(md.len, 9);

    // missing entries are NotFound with path context
    let e = root.metadata(OsStr::new("missing")).expect_err("must fail");
    assert_eq!(e.kind, ErrorKind::NotFound);
    assert!(e.path.is_some());

    // create_new refuses to clobber
    let e = root
        .open(
            OsStr::new("a.bin"),
            &OpenOptions {
                write: true,
                create_new: true,
                ..Default::default()
            },
        )
        .err()
        .expect("create_new over existing must fail");
    assert_eq!(e.kind, ErrorKind::AlreadyExists);

    // directories: create, list through a child capability, refuse
    // non-empty removal, then remove bottom-up
    root.create_dir(OsStr::new("d")).expect("mkdir");
    let d = root.open_dir(OsStr::new("d")).expect("open_dir");
    d.open(OsStr::new("inner"), &OpenOptions::create_truncate())
        .expect("create");
    let names: Vec<_> = d
        .read_dir()
        .expect("read_dir")
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(names.contains(&OsStr::new("inner").to_os_string()));
    let e = root
        .remove_dir(OsStr::new("d"))
        .expect_err("non-empty removal must fail");
    assert_eq!(e.kind, ErrorKind::DirectoryNotEmpty);
    d.remove_file(OsStr::new("inner")).expect("rm inner");
    root.remove_dir(OsStr::new("d")).expect("rmdir");
    root.remove_file(OsStr::new("a.bin")).expect("rm");
}

#[test]
fn mock_backend_conforms() {
    assert_fs_behavior(&platform_mock::MockDir::root());
}

#[test]
fn windows_backend_conforms() {
    let tmp = std::env::temp_dir().join(format!("rustils-parity-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("mk tempdir");
    let root = platform_windows::WindowsDir::open_ambient(&tmp).expect("open ambient");
    assert_fs_behavior(&root);
    std::fs::remove_dir_all(&tmp).ok();
}

/// Process behavior (docs/behavior/process.md) against the native
/// backend. Fixtures are OS-specific (`cmd`); the assertions mirror the
/// Linux leg's. `Signaled` has no Windows fixture — the spec pins that it
/// is never produced here.
#[test]
fn windows_process_backend_conforms() {
    use std::collections::BTreeMap;
    use std::ffi::OsString;

    use platform::process::{Command, EnvSpec, ExitStatus, Spawner, Stdio};

    let tmp = std::env::temp_dir().join(format!("rustils-proc-parity-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("mk tempdir");
    let s = platform_windows::WindowsSpawner;

    // Exit codes decode uniformly. `cmd` resolves via PATH + PATHEXT.
    let c = Command::new("cmd", tmp.clone()).arg("/c").arg("exit 7");
    let child = s.spawn(&c).expect("spawn");
    assert!(child.id() > 0);
    assert_eq!(child.wait().expect("wait"), ExitStatus::Code(7));

    // cwd is honored: the child sees the marker only if it starts there.
    std::fs::write(tmp.join("rustils-marker.txt"), b"x").expect("marker");
    let c = Command::new("cmd", tmp.clone())
        .arg("/c")
        .arg("if exist rustils-marker.txt (exit 0) else (exit 1)");
    assert!(s.spawn(&c).expect("spawn").wait().expect("wait").success());

    // Explicit env starts empty: only the given variables are visible.
    // SystemRoot rides along — cmd.exe itself misbehaves without it, and
    // inheriting it says nothing about leakage of arbitrary variables.
    let mut env = BTreeMap::new();
    env.insert(OsString::from("RUSTILS_CODE"), OsString::from("42"));
    if let Some(sr) = std::env::var_os("SystemRoot") {
        env.insert(OsString::from("SystemRoot"), sr);
    }
    let c = Command::new("cmd", tmp.clone())
        .arg("/c")
        .arg("exit %RUSTILS_CODE%")
        .env(EnvSpec::Explicit(env));
    assert_eq!(
        s.spawn(&c).expect("spawn").wait().expect("wait"),
        ExitStatus::Code(42)
    );

    // Stdio::Null wiring spawns and completes.
    let mut c = Command::new("cmd", tmp.clone())
        .arg("/c")
        .arg("echo swallowed");
    c.stdout = Stdio::Null;
    assert!(s.spawn(&c).expect("spawn").wait().expect("wait").success());

    // resolve: mechanism-level NotFound with path context.
    let e = s
        .resolve(std::ffi::OsStr::new("rustils-definitely-missing"))
        .expect_err("must fail");
    assert_eq!(e.kind, platform::error::ErrorKind::NotFound);
    assert!(e.path.is_some());

    std::fs::remove_dir_all(&tmp).ok();
}
