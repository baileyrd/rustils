//! Thin CLI over `coreutils::term_report`, wired to the real backend —
//! the Terminal surface's end-to-end prover (extraction map D9).
//!
//! `rterm` prints the tty report; `rterm --raw-probe` additionally
//! enters raw mode, reads one byte from stdin, restores, and reports it
//! (interactive use only — it refuses cleanly when stdin is not a tty).

use std::io::Read;

fn main() -> std::process::ExitCode {
    let raw_probe = std::env::args_os().any(|a| a == "--raw-probe");
    let mut term = coreutils::native::terminal();
    print!("{}", coreutils::term_report::report(term.as_ref()));

    if raw_probe {
        let got = coreutils::term_report::with_raw(term.as_mut(), || {
            let mut byte = [0u8; 1];
            std::io::stdin().lock().read_exact(&mut byte).map_err(|e| {
                platform::error::PlatformError::new(
                    platform::error::ErrorKind::Other,
                    e.raw_os_error()
                        .map(platform::error::OsCode::Errno)
                        .unwrap_or(platform::error::OsCode::None),
                    "read",
                )
            })?;
            Ok(byte[0])
        });
        match got {
            Ok(b) => println!("raw probe: read byte 0x{b:02x}"),
            Err(e) => {
                eprintln!("rterm: raw probe: {e}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }
    std::process::ExitCode::SUCCESS
}
