//! Parity suite seed (RFC v2 §9): one behavior-spec-derived assertion set,
//! run against every backend available on this host. On Linux CI that is
//! `platform-mock` + `platform-linux`; the Windows leg will add
//! `platform-windows` when its Dir impl lands (R1). The suite living as a
//! generic function over `&dyn Dir` is the point — a backend passes or it
//! doesn't, with no backend-specific test text to drift.

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

#[cfg(target_os = "linux")]
#[test]
fn linux_backend_conforms() {
    let tmp = std::env::temp_dir().join(format!("rustils-parity-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("mk tempdir");
    let root = platform_linux::LinuxDir::open_ambient(&tmp).expect("open ambient");
    assert_fs_behavior(&root);
    std::fs::remove_dir_all(&tmp).ok();
}

/// Process behavior (docs/behavior/process.md) against the native
/// backend. Fixtures are OS-specific (`sh`); the assertions mirror the
/// Windows leg's.
#[cfg(target_os = "linux")]
#[test]
fn linux_process_backend_conforms() {
    use std::collections::BTreeMap;
    use std::ffi::OsString;

    use platform::process::{Command, EnvSpec, ExitStatus, Spawner, Stdio};

    let tmp = std::env::temp_dir().join(format!("rustils-proc-parity-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("mk tempdir");
    let s = platform_linux::LinuxSpawner;

    // Exit codes decode uniformly.
    let c = Command::new("sh", tmp.clone()).arg("-c").arg("exit 7");
    let child = s.spawn(&c).expect("spawn");
    assert!(child.id() > 0);
    assert_eq!(child.wait().expect("wait"), ExitStatus::Code(7));

    // Signal termination decodes as Signaled — the B-5 sentinel: a raw
    // waitpid status word (9 vs 0x0009 vs 35072) must never leak through.
    let c = Command::new("sh", tmp.clone()).arg("-c").arg("kill -9 $$");
    assert_eq!(
        s.spawn(&c).expect("spawn").wait().expect("wait"),
        ExitStatus::Signaled(9)
    );

    // cwd is honored: the child sees the marker only if it starts there.
    std::fs::write(tmp.join("rustils-marker"), b"x").expect("marker");
    let c = Command::new("sh", tmp.clone())
        .arg("-c")
        .arg("test -e rustils-marker");
    assert!(s.spawn(&c).expect("spawn").wait().expect("wait").success());

    // Explicit env starts empty: only the given variables are visible.
    let mut env = BTreeMap::new();
    env.insert(OsString::from("RUSTILS_PARITY"), OsString::from("yes"));
    let mut c = Command::new("sh", tmp.clone())
        .arg("-c")
        .arg("test \"$RUSTILS_PARITY\" = yes && test -z \"$RUSTILS_ABSENT\"");
    c.env = EnvSpec::Explicit(env);
    assert!(s.spawn(&c).expect("spawn").wait().expect("wait").success());

    // Stdio::Null wiring spawns and completes.
    let mut c = Command::new("sh", tmp.clone())
        .arg("-c")
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

/// Groups and kill-tree (divergence 001 pin, Linux side). The sleeper
/// would run 30s; the test completing inside cargo's normal budget IS the
/// kill assertion (wall-clock discipline, extraction map D7).
#[cfg(target_os = "linux")]
#[test]
fn linux_process_group_kill() {
    use platform::process::{Command, ExitStatus, GroupSpec, Spawner};

    let tmp = std::env::temp_dir();
    let s = platform_linux::LinuxSpawner;

    // kill_tree on a NewGroup child: SIGKILL reaches the group; wait
    // reports Signaled(9) — never a raw status word (B-5).
    let c = Command::new("sh", tmp.clone())
        .arg("-c")
        .arg("sleep 30")
        .group(GroupSpec::NewGroup);
    let child = s.spawn(&c).expect("spawn");
    child.kill_tree().expect("kill_tree");
    assert_eq!(child.wait().expect("wait"), ExitStatus::Signaled(9));

    // kill_tree reaches a grandchild: the shell spawns its own child and
    // waits on it; killing the group takes both down promptly.
    let c = Command::new("sh", tmp.clone())
        .arg("-c")
        .arg("sleep 30 & wait")
        .group(GroupSpec::NewGroup);
    let child = s.spawn(&c).expect("spawn");
    std::thread::sleep(std::time::Duration::from_millis(100));
    child.kill_tree().expect("kill_tree");
    assert_eq!(child.wait().expect("wait"), ExitStatus::Signaled(9));

    // kill_single works without a group.
    let c = Command::new("sh", tmp.clone()).arg("-c").arg("sleep 30");
    let child = s.spawn(&c).expect("spawn");
    child.kill_single().expect("kill_single");
    assert_eq!(child.wait().expect("wait"), ExitStatus::Signaled(9));

    // kill_tree without NewGroup is refused, not guessed at.
    let c = Command::new("sh", tmp).arg("-c").arg("exit 0");
    let child = s.spawn(&c).expect("spawn");
    assert_eq!(
        child.kill_tree().expect_err("must refuse").kind,
        platform::error::ErrorKind::Unsupported
    );
    child.wait().expect("wait");
}
