//! Live PTY test (rustils#83): spawns real children attached to a real
//! ConPTY pseudo console, no mock — the same live-verification bar #82's
//! Linux backend was held to. Only actually executes on CI's
//! `windows-latest` leg; this crate's whole backend is developed from a
//! Linux host against `cargo check --target x86_64-pc-windows-gnu`
//! (`crates/platform-windows/src/lib.rs`'s own module doc), so nothing
//! here has been run outside CI.

#![cfg(windows)]

use std::time::{Duration, Instant};

use platform::process::{Command, GroupSpec};
use platform::pty::{Pty, PtyMaster};
use platform::term::WinSize;
use platform_windows::WindowsPty;

fn size() -> WinSize {
    WinSize { rows: 24, cols: 80 }
}

/// Read from `master` in a loop until `needle` appears in the
/// accumulated output or `attempts` reads have happened — mirrors
/// `platform-linux/tests/pty.rs`'s `read_until`, for the same reason: a
/// pty's echo plus a shell's own output can arrive in more than one
/// `read()` call.
fn read_until(master: &dyn PtyMaster, needle: &str, attempts: usize) -> String {
    let mut acc = String::new();
    let mut buf = [0u8; 256];
    for _ in 0..attempts {
        match master.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                if acc.contains(needle) {
                    return acc;
                }
            }
            Err(_) => break,
        }
    }
    acc
}

#[test]
fn spawn_attaches_a_real_child_to_the_pseudo_console() {
    let pty = WindowsPty;
    let cmd = Command::new("cmd", ".")
        .arg("/c")
        .arg("echo hello-from-conpty");
    let (master, child) = pty.spawn(&cmd, size()).expect("spawn");

    let output = read_until(master.as_ref(), "hello-from-conpty", 32);
    assert!(
        output.contains("hello-from-conpty"),
        "expected 'hello-from-conpty' in master output, saw: {output:?}"
    );

    let status = child.wait().expect("wait");
    assert!(status.success());
}

#[test]
fn master_io_round_trips_with_the_spawned_child() {
    let pty = WindowsPty;
    let cmd = Command::new("cmd", ".")
        .arg("/c")
        .arg("set /p REPLY= & echo got:%REPLY%");
    let (master, child) = pty.spawn(&cmd, size()).expect("spawn");

    master.write(b"hello\r\n").expect("write");
    let output = read_until(master.as_ref(), "got:hello", 32);
    assert!(
        output.contains("got:hello"),
        "expected 'got:hello' in master output, saw: {output:?}"
    );

    let status = child.wait().expect("wait");
    assert!(status.success());
}

#[test]
fn eof_is_reported_as_ok_zero_after_the_child_exits() {
    let pty = WindowsPty;
    let cmd = Command::new("cmd", ".").arg("/c").arg("exit 0");
    let (master, child) = pty.spawn(&cmd, size()).expect("spawn");
    let _ = child.wait();

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut buf = [0u8; 64];
    loop {
        match master.read(&mut buf) {
            Ok(0) => break,
            Ok(_) => {
                assert!(Instant::now() < deadline, "never reached EOF within 10s");
            }
            Err(e) => panic!("read must translate ERROR_BROKEN_PIPE to Ok(0), got Err: {e:?}"),
        }
    }
}

#[test]
fn join_group_is_rejected() {
    let pty = WindowsPty;
    let cmd = Command::new("cmd", ".").group(GroupSpec::JoinGroup(1));
    let err = pty
        .spawn(&cmd, size())
        .err()
        .expect("JoinGroup must be rejected before attempting a real spawn");
    assert_eq!(err.kind, platform::error::ErrorKind::InvalidInput);
}

#[test]
fn resize_succeeds_against_a_real_pseudo_console() {
    let pty = WindowsPty;
    let cmd = Command::new("cmd", ".").arg("/c").arg("exit 0");
    let (master, child) = pty.spawn(&cmd, size()).expect("spawn");

    master
        .resize(WinSize {
            rows: 40,
            cols: 120,
        })
        .expect("resize");

    let _ = child.wait();
}

/// The load-bearing test for the teardown ordering
/// (`docs/design-discussion-pty.md`'s EOF-vs-exit lesson,
/// `sys::pty::close`'s drain-before-`ClosePseudoConsole` fix): spawn a
/// child that writes far more output than a pipe's default buffer holds,
/// then drop the master **without ever reading any of it**. Without the
/// drain, `ClosePseudoConsole` can block forever waiting for conhost's
/// writer to finish flushing into a pipe nobody is draining — this test
/// hanging (rather than failing cleanly) is itself the failure signal if
/// the fix regresses.
#[test]
fn dropping_an_undrained_master_does_not_deadlock() {
    let pty = WindowsPty;
    let cmd = Command::new("cmd", ".")
        .arg("/c")
        .arg("for /L %i in (1,1,20000) do @echo line %i");
    let (master, child) = pty.spawn(&cmd, size()).expect("spawn");

    // Give the child a moment to actually produce output and fill the
    // pipe before tearing down undrained.
    std::thread::sleep(Duration::from_millis(200));
    drop(master);
    let _ = child.wait();
}
