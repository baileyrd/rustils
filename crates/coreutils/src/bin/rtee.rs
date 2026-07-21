//! `rtee <file> -- cmd [args…]` — run a command with its stdout captured,
//! copying every byte to both our own stdout and `file`. The reference
//! consumer for `Stdio::Pipe`/`take_stdout` (extraction map step 4): it
//! reads the pipe *while the child runs* — the deadlock-free capture
//! pattern `docs/behavior/process.md` specifies — then waits and
//! propagates the exit status.

use std::io::Write;

use platform::process::{Command, ExitStatus, Stdio};

fn main() -> std::process::ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let (file_arg, argv) = match args.split_first() {
        Some((f, rest)) if rest.first().is_some_and(|s| s == "--") && rest.len() >= 2 => {
            (f, &rest[1..])
        }
        _ => {
            eprintln!("usage: rtee <file> -- cmd [args...]");
            return std::process::ExitCode::from(2);
        }
    };
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("rtee: cannot determine cwd: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let mut out_file = match std::fs::File::create(file_arg) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("rtee: {}: {e}", file_arg.to_string_lossy());
            return std::process::ExitCode::FAILURE;
        }
    };

    let mut cmd = Command::new(&argv[0], cwd).args(argv[1..].iter().cloned());
    cmd.stdout = Stdio::Pipe;
    let spawner = coreutils::native::spawner();
    let mut child = match spawner.spawn(&cmd) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("rtee: {e}");
            return std::process::ExitCode::from(127);
        }
    };
    let mut pipe = match child.take_stdout() {
        Some(p) => p,
        None => {
            eprintln!("rtee: child stdout was not piped");
            return std::process::ExitCode::FAILURE;
        }
    };

    // Drain to EOF while the child runs, THEN wait — the ordering that
    // cannot deadlock regardless of how much the child writes.
    let mut stdout = std::io::stdout().lock();
    let mut buf = [0u8; 8192];
    loop {
        match pipe.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if stdout.write_all(&buf[..n]).is_err() {
                    break;
                }
                if let Err(e) = out_file.write_all(&buf[..n]) {
                    eprintln!("rtee: write: {e}");
                    return std::process::ExitCode::FAILURE;
                }
            }
            Err(e) => {
                eprintln!("rtee: read: {e}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }
    match child.wait() {
        Ok(ExitStatus::Code(code)) => std::process::ExitCode::from((code & 0xff) as u8),
        Ok(ExitStatus::Signaled(sig)) => std::process::ExitCode::from((128 + (sig & 0x7f)) as u8),
        // Child::wait only ever produces Code/Signaled — Stopped/Continued
        // are wait_job/try_wait_job-only (D10).
        Ok(ExitStatus::Stopped(_) | ExitStatus::Continued) => unreachable!(),
        Err(e) => {
            eprintln!("rtee: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
