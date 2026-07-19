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
    // Drop this read handle now: "a.bin" gets renamed forward to b.bin,
    // then c.bin, then d.bin later in this function (rename just relinks
    // the name, same underlying file), and on Windows a lingering open
    // handle keeps whatever name the file currently has visible in
    // directory enumeration even after remove_file marks it for
    // deletion. Shadowing `f` below does not drop this binding early —
    // only an explicit drop, or end of scope, does.
    drop(f);

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
    // Drop the capability before removing the directory it names: on
    // Windows, delete-on-close semantics mean a directory entry stays
    // visible in enumeration until every open handle to it — including
    // this one — closes, not just the handle `remove_dir` itself opens
    // and releases. Linux would tolerate the leak (unlink/rmdir detach
    // the name immediately regardless of open fds), but the capability
    // model says: done with it, drop it.
    drop(d);
    root.remove_dir(OsStr::new("d")).expect("rmdir");

    // rename: replaces by default, no-replace refuses (D11, roadmap
    // Phase 3). "a.bin" (still 9 bytes from the round-trip above) moves
    // to "b.bin"; the old name is gone, the new one reads back intact.
    root.rename(OsStr::new("a.bin"), OsStr::new("b.bin"))
        .expect("rename");
    assert_eq!(
        root.metadata(OsStr::new("a.bin")).unwrap_err().kind,
        ErrorKind::NotFound
    );
    assert_eq!(root.metadata(OsStr::new("b.bin")).unwrap().len, 9);

    // rename replaces an existing destination by default...
    root.open(OsStr::new("c.bin"), &OpenOptions::create_truncate())
        .expect("create c");
    root.rename(OsStr::new("b.bin"), OsStr::new("c.bin"))
        .expect("rename replaces c.bin");
    assert_eq!(root.metadata(OsStr::new("c.bin")).unwrap().len, 9);
    assert_eq!(
        root.metadata(OsStr::new("b.bin")).unwrap_err().kind,
        ErrorKind::NotFound
    );

    // ...but rename_no_replace refuses when the destination is a
    // DIFFERENT existing entry, atomically (no partial move: "c.bin"
    // and "e.bin" are both untouched). Deliberately not "rename c.bin
    // onto its own name" — that degenerate case is a real, harmless
    // cross-OS divergence (Linux's renameat2(RENAME_NOREPLACE) refuses
    // it with EEXIST; Windows' NtSetInformationFile treats renaming a
    // file onto its own current name as a no-op success), not the
    // atomic-refuse-if-exists contract this assertion means to pin.
    root.open(OsStr::new("e.bin"), &OpenOptions::create_truncate())
        .expect("create e");
    let e = root
        .rename_no_replace(OsStr::new("c.bin"), OsStr::new("e.bin"))
        .expect_err("must refuse: e.bin already exists");
    assert_eq!(e.kind, ErrorKind::AlreadyExists);
    assert_eq!(root.metadata(OsStr::new("c.bin")).unwrap().len, 9);
    assert_eq!(root.metadata(OsStr::new("e.bin")).unwrap().len, 0);
    root.remove_file(OsStr::new("e.bin")).expect("rm e.bin");

    root.rename_no_replace(OsStr::new("c.bin"), OsStr::new("d.bin"))
        .expect("no existing destination: must succeed");
    root.remove_file(OsStr::new("d.bin")).expect("rm d.bin");

    // write_atomic: publishes contents under `rel`, leaves no temp file
    // behind, and a second call (replace) fully overwrites the first.
    root.write_atomic(OsStr::new("atomic.txt"), b"first")
        .expect("write_atomic 1");
    let mut f = root
        .open(OsStr::new("atomic.txt"), &OpenOptions::read())
        .expect("open atomic");
    let mut buf = [0u8; 64];
    let n = f.read(&mut buf).expect("read atomic");
    assert_eq!(&buf[..n], b"first");
    drop(f);

    root.write_atomic(OsStr::new("atomic.txt"), b"second, shorter overwrite")
        .expect("write_atomic 2");
    let mut f = root
        .open(OsStr::new("atomic.txt"), &OpenOptions::read())
        .expect("re-open atomic");
    let n = f.read(&mut buf).expect("re-read atomic");
    assert_eq!(&buf[..n], b"second, shorter overwrite");
    drop(f);

    // symlink/read_link (symlink slice): the target is stored verbatim,
    // not validated or resolved — a dangling target is fine to create.
    // metadata (lstat-style, never follows) classifies the link itself
    // as Symlink; read_link round-trips the exact bytes given.
    root.symlink(OsStr::new("nowhere/dangling"), OsStr::new("link1"))
        .expect("symlink");
    assert_eq!(
        root.metadata(OsStr::new("link1")).unwrap().file_type,
        FileType::Symlink
    );
    assert_eq!(
        root.read_link(OsStr::new("link1")).unwrap(),
        OsStr::new("nowhere/dangling").to_os_string()
    );
    // Like `open` with `create_new`, `symlink` refuses to clobber an
    // existing name rather than silently replacing it.
    let e = root
        .symlink(OsStr::new("whatever"), OsStr::new("link1"))
        .expect_err("must refuse: link1 already exists");
    assert_eq!(e.kind, ErrorKind::AlreadyExists);
    // `read_link` on a non-symlink refuses, mirroring POSIX `EINVAL`.
    let e = root
        .read_link(OsStr::new("atomic.txt"))
        .expect_err("not a symlink");
    assert_eq!(e.kind, ErrorKind::InvalidInput);
    root.remove_file(OsStr::new("link1")).expect("rm link1");

    // A symlink to an existing directory (Windows divergence,
    // `Dir::symlink`'s doc comment: the NT reparse point must declare
    // file-vs-directory at creation, decided here by `target` existing
    // as a directory relative to `self`). Cleanup tries `remove_file`
    // first, falling back to `remove_dir` — the same "try file, then
    // directory" shape `rename`'s own implementation already uses,
    // since which one Windows requires for a directory-type link isn't
    // pinned by this suite.
    root.create_dir(OsStr::new("realdir"))
        .expect("mkdir realdir");
    root.symlink(OsStr::new("realdir"), OsStr::new("dirlink"))
        .expect("symlink to dir");
    assert_eq!(
        root.metadata(OsStr::new("dirlink")).unwrap().file_type,
        FileType::Symlink
    );
    assert_eq!(
        root.read_link(OsStr::new("dirlink")).unwrap(),
        OsStr::new("realdir").to_os_string()
    );
    if root.remove_file(OsStr::new("dirlink")).is_err() {
        root.remove_dir(OsStr::new("dirlink")).expect("rm dirlink");
    }
    root.remove_dir(OsStr::new("realdir")).expect("rm realdir");

    let names: Vec<_> = root
        .read_dir()
        .expect("read_dir")
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(
        names,
        vec![OsStr::new("atomic.txt").to_os_string()],
        "no leftover temp file, no leftover a/b/c.bin"
    );
    root.remove_file(OsStr::new("atomic.txt")).expect("rm");
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

/// try_wait + wait_any (extraction map step 3 seed, Linux side). The
/// 30s sleeper finishing inside cargo's budget IS the assertion that
/// wait_any picked the quick child, not the sleeper.
#[cfg(target_os = "linux")]
#[test]
fn linux_wait_any_and_try_wait() {
    use platform::process::{wait_any, Child, Command, ExitStatus, Spawner};

    let tmp = std::env::temp_dir();
    let s = platform_linux::LinuxSpawner;

    // try_wait: None while running; Some after; wait returns the stashed
    // status (WNOHANG reaped the zombie — losing it would hang or error).
    let c = Command::new("sh", tmp.clone()).arg("-c").arg("exit 4");
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
    let sleeper = s
        .spawn(&Command::new("sh", tmp.clone()).arg("-c").arg("sleep 30"))
        .expect("spawn");
    let quick = s
        .spawn(&Command::new("sh", tmp.clone()).arg("-c").arg("exit 9"))
        .expect("spawn");
    let mut children: Vec<Box<dyn Child>> = vec![sleeper, quick];
    let idx = s
        .wait_any(&mut children, None)
        .expect("wait_any")
        .expect("no timeout");
    assert_eq!(idx, 1, "the quick child finishes first");
    assert_eq!(
        children.remove(1).wait().expect("wait"),
        ExitStatus::Code(9)
    );

    // Timeout: the sleeper alone times out promptly.
    let waited = s
        .wait_any(&mut children, Some(std::time::Duration::from_millis(50)))
        .expect("wait_any");
    assert_eq!(waited, None);

    // Empty set is refused like the OS primitives refuse it — by both
    // the portable loop and the backend multiplexer.
    let mut none: Vec<Box<dyn Child>> = Vec::new();
    assert_eq!(
        wait_any(&mut none, None).expect_err("must refuse").kind,
        platform::error::ErrorKind::InvalidInput
    );
    assert_eq!(
        s.wait_any(&mut none, None).expect_err("must refuse").kind,
        platform::error::ErrorKind::InvalidInput
    );

    // Release the sleeper.
    children[0].kill_single().expect("kill");
    children.remove(0).wait().expect("wait");
}

/// Many children through the multiplexer (R3): 70 processes collected to
/// completion. On Linux this exercises a 70-fd poll set; the count is
/// chosen to force the Windows leg past WaitForMultipleObjects's
/// 64-handle cap into the chunked sweep.
#[cfg(target_os = "linux")]
#[test]
fn linux_wait_any_many_children() {
    use platform::process::{Child, Command, ExitStatus, Spawner};

    let tmp = std::env::temp_dir();
    let s = platform_linux::LinuxSpawner;
    let mut children: Vec<Box<dyn Child>> = (0..70)
        .map(|i| {
            s.spawn(
                &Command::new("sh", tmp.clone())
                    .arg("-c")
                    .arg(format!("exit {}", i % 8)),
            )
            .expect("spawn")
        })
        .collect();
    let mut statuses = Vec::new();
    while !children.is_empty() {
        let idx = s
            .wait_any(&mut children, None)
            .expect("wait_any")
            .expect("no timeout");
        statuses.push(children.remove(idx).wait().expect("wait"));
    }
    assert_eq!(statuses.len(), 70);
    assert!(statuses
        .iter()
        .all(|st| matches!(st, ExitStatus::Code(c) if *c < 8)));
}

/// Pipes (extraction map step 4, Linux side): capture, stdin feeding with
/// EOF-by-drop, and pipe-read EOF when the child exits.
#[cfg(target_os = "linux")]
#[test]
fn linux_process_pipes() {
    use platform::process::{Command, Spawner, Stdio};

    let tmp = std::env::temp_dir();
    let s = platform_linux::LinuxSpawner;

    // stdout capture: bytes arrive, then EOF at child exit.
    // \377 (octal) — POSIX printf; \x escapes are a bashism dash lacks.
    let mut c = Command::new("sh", tmp.clone())
        .arg("-c")
        .arg("printf 'cap\\377 tured'");
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
    assert_eq!(got, b"cap\xff tured");
    assert!(child.wait().expect("wait").success());

    // stdin feeding: write, drop for EOF, child echoes through its own
    // piped stdout.
    let mut c = Command::new("cat", tmp);
    c.stdin = Stdio::Pipe;
    c.stdout = Stdio::Pipe;
    let mut child = s.spawn(&c).expect("spawn");
    let mut stdin = child.take_stdin().expect("piped stdin");
    let mut stdout = child.take_stdout().expect("piped stdout");
    stdin.write(b"round trip").expect("write");
    drop(stdin); // EOF — without this, cat never exits (D5 contract)
    let mut got = Vec::new();
    loop {
        let n = stdout.read(&mut buf).expect("read");
        if n == 0 {
            break;
        }
        got.extend_from_slice(&buf[..n]);
    }
    assert_eq!(got, b"round trip");
    assert!(child.wait().expect("wait").success());
}

/// Deferred signals (D6, Linux side): a child delivers SIGTERM to this
/// test process; the handler's one atomic store is consumed at the next
/// safe point. Also pins take-consumes and burst coalescing semantics.
#[cfg(target_os = "linux")]
#[test]
fn linux_signal_source_defers_and_coalesces() {
    use platform::events::{SignalEvent, SignalSource};
    use platform::process::{Command, Spawner};

    let tmp = std::env::temp_dir();
    let s = platform_linux::LinuxSpawner;
    let signals = platform_linux::LinuxSignalSource;
    signals
        .install(&[SignalEvent::Terminate, SignalEvent::Hangup])
        .expect("install");
    assert_eq!(signals.take(), None);

    // A real delivery: the child TERMs its parent (this process), which
    // must survive (deferral, not default disposition) and observe the
    // event at this safe point.
    let c = Command::new("sh", tmp.clone())
        .arg("-c")
        .arg("kill -TERM $PPID");
    assert!(s.spawn(&c).expect("spawn").wait().expect("wait").success());
    let mut seen = None;
    for _ in 0..100 {
        if let Some(e) = signals.take() {
            seen = Some(e);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(seen, Some(SignalEvent::Terminate));
    assert_eq!(signals.take(), None, "take consumes");

    // Burst: HUP then TERM before a take — single slot, latest wins.
    let c = Command::new("sh", tmp)
        .arg("-c")
        .arg("kill -HUP $PPID; kill -TERM $PPID");
    assert!(s.spawn(&c).expect("spawn").wait().expect("wait").success());
    let mut seen = None;
    for _ in 0..100 {
        if let Some(e) = signals.take() {
            seen = Some(e);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // Two rapid deliveries may coalesce to the latest (single slot) or,
    // if this process consumed between them, arrive as just the first —
    // what is pinned: at least one arrives and it is one of the two.
    assert!(matches!(
        seen,
        Some(SignalEvent::Terminate) | Some(SignalEvent::Hangup)
    ));
}

/// The redirected-streams contract (`docs/behavior/term.md`): under a
/// test harness no std stream is a terminal, and the surface must say
/// so honestly — false everywhere, Err for size, refuse raw mode, and
/// keep leave_raw an Ok no-op.
#[cfg(target_os = "linux")]
#[test]
fn linux_terminal_honest_when_redirected() {
    use platform::term::{TermStream, Terminal};
    let mut t = platform_linux::LinuxTerminal::new();
    // cargo's test harness captures the streams; none is a tty.
    assert!(!t.is_tty(TermStream::Stdin));
    assert!(!t.is_tty(TermStream::Stdout));
    assert!(!t.is_tty(TermStream::Stderr));
    assert!(t.window_size().is_err(), "no tty: size must be Err");
    assert!(!t.is_raw(), "no tty: is_raw's live probe must say false");
    assert!(t.enter_raw().is_err(), "no tty: raw mode must refuse");
    t.leave_raw().expect("leave without enter is an Ok no-op");
    assert!(t.set_echo(false).is_err(), "no tty: set_echo must refuse");
}
