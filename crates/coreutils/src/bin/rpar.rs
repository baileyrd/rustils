//! `rpar cmd [args…] [-- cmd [args…]]…` — run commands concurrently and
//! report each as it finishes, in completion order. The reference
//! consumer for `wait_any`/`try_wait` (extraction map step 3): a
//! miniature parallel executor whose whole job is "block until whichever
//! child finishes next". Exits 0 only if every command did.

use platform::process::{Command, ExitStatus};

fn main() -> std::process::ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let commands: Vec<&[std::ffi::OsString]> = args
        .split(|a| a == "--")
        .filter(|c| !c.is_empty())
        .collect();
    if commands.is_empty() {
        eprintln!("usage: rpar cmd [args...] [-- cmd [args...]]...");
        return std::process::ExitCode::from(2);
    }
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("rpar: cannot determine cwd: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let spawner = coreutils::native::spawner();
    let mut children = Vec::new();
    let mut labels = Vec::new();
    let mut failed = false;
    for argv in &commands {
        let cmd = Command::new(&argv[0], cwd.clone()).args(argv[1..].iter().cloned());
        match spawner.spawn(&cmd) {
            Ok(child) => {
                labels.push(argv[0].to_string_lossy().into_owned());
                children.push(child);
            }
            Err(e) => {
                eprintln!("rpar: {e}");
                failed = true;
            }
        }
    }

    // Completion-order reporting: the backend's multiplexed wait_any
    // (pidfd+poll / WaitForMultipleObjects) names whichever finished;
    // the consuming wait releases it.
    while !children.is_empty() {
        let idx = match spawner.wait_any(&mut children, None) {
            Ok(Some(i)) => i,
            Ok(None) => unreachable!("no timeout was given"),
            Err(e) => {
                eprintln!("rpar: {e}");
                return std::process::ExitCode::FAILURE;
            }
        };
        let label = labels.swap_remove(idx);
        let child = children.swap_remove(idx);
        match child.wait() {
            Ok(status) => {
                let ok = status.success();
                failed |= !ok;
                match status {
                    ExitStatus::Code(c) => println!("[{label}] exited {c}"),
                    ExitStatus::Signaled(s) => println!("[{label}] killed by signal {s}"),
                }
            }
            Err(e) => {
                eprintln!("rpar: {label}: {e}");
                failed = true;
            }
        }
    }
    if failed {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}
