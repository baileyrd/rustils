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

/// Groups and kill-tree (divergence 001 pin, Windows side). The sleeper
/// (`ping -n 30` ≈ 29s) completing inside cargo's normal budget IS the
/// kill assertion (wall-clock discipline, extraction map D7); instant-
/// exit stand-ins keep the refusal case race-free.
#[test]
fn windows_process_group_kill() {
    use platform::process::{Command, ExitStatus, GroupSpec, Spawner, Stdio};

    let tmp = std::env::temp_dir();
    let s = platform_windows::WindowsSpawner;

    let sleeper = |group: GroupSpec| {
        let mut c = Command::new("ping", tmp.clone())
            .arg("-n")
            .arg("30")
            .arg("127.0.0.1")
            .group(group);
        c.stdout = Stdio::Null;
        c
    };

    // kill_tree on a NewGroup child: TerminateJobObject reaches the job;
    // wait reports Code(1) — Windows has no signal to encode (divergence
    // 001, the cross-OS pin opposite Linux's Signaled(9)).
    let child = s.spawn(&sleeper(GroupSpec::NewGroup)).expect("spawn");
    child.kill_tree().expect("kill_tree");
    assert_eq!(child.wait().expect("wait"), ExitStatus::Code(1));

    // kill_tree reaches a grandchild: cmd spawns ping as its own child;
    // killing the job takes both down promptly.
    let mut c = Command::new("cmd", tmp.clone())
        .arg("/c")
        .arg("ping -n 30 127.0.0.1 >NUL")
        .group(GroupSpec::NewGroup);
    c.stdout = Stdio::Null;
    let child = s.spawn(&c).expect("spawn");
    std::thread::sleep(std::time::Duration::from_millis(100));
    child.kill_tree().expect("kill_tree");
    assert_eq!(child.wait().expect("wait"), ExitStatus::Code(1));

    // kill_single works without a group.
    let child = s.spawn(&sleeper(GroupSpec::Inherit)).expect("spawn");
    child.kill_single().expect("kill_single");
    assert_eq!(child.wait().expect("wait"), ExitStatus::Code(1));

    // kill_tree without NewGroup is refused, not guessed at.
    let c = Command::new("cmd", tmp).arg("/c").arg("exit 0");
    let child = s.spawn(&c).expect("spawn");
    assert_eq!(
        child.kill_tree().expect_err("must refuse").kind,
        platform::error::ErrorKind::Unsupported
    );
    child.wait().expect("wait");
}

/// try_wait + wait_any (extraction map step 3 seed, Windows side) —
/// mirrors the Linux leg's assertions with cmd/ping fixtures.
#[test]
fn windows_wait_any_and_try_wait() {
    use platform::process::{wait_any, Child, Command, ExitStatus, Spawner, Stdio};

    let tmp = std::env::temp_dir();
    let s = platform_windows::WindowsSpawner;

    // try_wait: None while running; Some after; wait returns the stash.
    let c = Command::new("cmd", tmp.clone()).arg("/c").arg("exit 4");
    let mut child: Box<dyn Child> = s.spawn(&c).expect("spawn");
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            assert_eq!(status, ExitStatus::Code(4));
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(child.try_wait().expect("repoll"), Some(ExitStatus::Code(4)));
    assert_eq!(child.wait().expect("wait"), ExitStatus::Code(4));

    // wait_any: returns the quick child's index, sleeper still running.
    let mut sleeper_cmd = Command::new("ping", tmp.clone())
        .arg("-n")
        .arg("30")
        .arg("127.0.0.1");
    sleeper_cmd.stdout = Stdio::Null;
    let sleeper = s.spawn(&sleeper_cmd).expect("spawn");
    let quick = s
        .spawn(&Command::new("cmd", tmp.clone()).arg("/c").arg("exit 9"))
        .expect("spawn");
    let mut children: Vec<Box<dyn Child>> = vec![sleeper, quick];
    let idx = wait_any(&mut children, None)
        .expect("wait_any")
        .expect("no timeout");
    assert_eq!(idx, 1, "the quick child finishes first");
    assert_eq!(
        children.remove(1).wait().expect("wait"),
        ExitStatus::Code(9)
    );

    // Timeout: the sleeper alone times out promptly.
    let waited =
        wait_any(&mut children, Some(std::time::Duration::from_millis(50))).expect("wait_any");
    assert_eq!(waited, None);

    // Empty set is refused like the OS primitives refuse it.
    let mut none: Vec<Box<dyn Child>> = Vec::new();
    assert_eq!(
        wait_any(&mut none, None).expect_err("must refuse").kind,
        platform::error::ErrorKind::InvalidInput
    );

    // Release the sleeper.
    children[0].kill_single().expect("kill");
    children.remove(0).wait().expect("wait");
}

/// Pipes (extraction map step 4, Windows side): capture with EOF at child
/// exit (BROKEN_PIPE decoded as end-of-file), and stdin feeding with
/// EOF-by-drop through findstr.
#[test]
fn windows_process_pipes() {
    use platform::process::{Command, Spawner, Stdio};

    let tmp = std::env::temp_dir();
    let s = platform_windows::WindowsSpawner;

    // stdout capture: bytes arrive, then EOF at child exit.
    let mut c = Command::new("cmd", tmp.clone())
        .arg("/c")
        .arg("echo captured");
    c.stdout = Stdio::Pipe;
    let mut child = s.spawn(&c).expect("spawn");
    let mut pipe = child.take_stdout().expect("piped stdout");
    assert!(child.take_stdout().is_none(), "take is once");
    let mut got = Vec::new();
    let mut buf = [0u8; 64];
    loop {
        let n = pipe.read(&mut buf).expect("read");
        if n == 0 {
            break;
        }
        got.extend_from_slice(&buf[..n]);
    }
    // cmd's echo appends \r\n.
    assert_eq!(got, b"captured\r\n");
    assert!(child.wait().expect("wait").success());

    // stdin feeding: findstr "^" echoes every stdin line after EOF.
    let mut c = Command::new("findstr", tmp).arg("^");
    c.stdin = Stdio::Pipe;
    c.stdout = Stdio::Pipe;
    let mut child = s.spawn(&c).expect("spawn");
    let mut stdin = child.take_stdin().expect("piped stdin");
    let mut stdout = child.take_stdout().expect("piped stdout");
    stdin.write(b"round trip\r\n").expect("write");
    drop(stdin); // EOF — findstr exits only once stdin closes (D5)
    let mut got = Vec::new();
    loop {
        let n = stdout.read(&mut buf).expect("read");
        if n == 0 {
            break;
        }
        got.extend_from_slice(&buf[..n]);
    }
    assert_eq!(got, b"round trip\r\n");
    assert!(child.wait().expect("wait").success());
}
