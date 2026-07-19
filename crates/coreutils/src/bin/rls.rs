//! Thin CLI over `coreutils::ls`, wired to the real backend for this OS.
//! Like `rcat`, exists to prove the full stack end to end; the listing
//! logic is tested against platform-mock in the library.

use std::path::Path;

fn main() -> std::process::ExitCode {
    let arg = std::env::args_os()
        .nth(1)
        .unwrap_or_else(|| std::ffi::OsString::from("."));
    let dir = match coreutils::native::open_dir(Path::new(&arg)) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("rls: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match coreutils::ls::ls(dir.as_ref()) {
        Ok(entries) => {
            for entry in &entries {
                println!("{}", coreutils::ls::render(entry));
            }
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("rls: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
