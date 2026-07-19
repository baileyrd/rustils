//! # coreutils — the reference consumer
//!
//! Utilities written *only* against the `platform` trait surface: they
//! compile with zero knowledge of which backend runs them, and their unit
//! tests run against `platform-mock` with no OS interaction. This crate is
//! the consumer that gates the fs/process domains (RFC v2 §3) and the
//! exercise ground for the understanding mandate (M1).
//!
//! Explicitly not a uutils competitor; rush bundles uutils for daily-driver
//! use (rush ADR-005).

#![forbid(unsafe_code)]

pub mod cat;
pub mod ls;
pub mod term_report;

/// Ambient entry point to this OS's native backend — the one place the
/// CLI binaries touch a concrete backend type; everything else in this
/// crate is written against the `platform` traits alone.
#[cfg(any(target_os = "linux", windows))]
pub mod native {
    use std::path::Path;

    use platform::error::Result;
    use platform::fs::Dir;

    #[cfg(target_os = "linux")]
    pub fn open_dir(path: &Path) -> Result<Box<dyn Dir>> {
        Ok(Box::new(platform_linux::LinuxDir::open_ambient(path)?))
    }

    #[cfg(windows)]
    pub fn open_dir(path: &Path) -> Result<Box<dyn Dir>> {
        Ok(Box::new(platform_windows::WindowsDir::open_ambient(path)?))
    }

    #[cfg(target_os = "linux")]
    pub fn spawner() -> Box<dyn platform::process::Spawner> {
        Box::new(platform_linux::LinuxSpawner)
    }

    #[cfg(windows)]
    pub fn spawner() -> Box<dyn platform::process::Spawner> {
        Box::new(platform_windows::WindowsSpawner)
    }

    #[cfg(target_os = "linux")]
    pub fn signal_source() -> Box<dyn platform::events::SignalSource> {
        Box::new(platform_linux::LinuxSignalSource)
    }

    #[cfg(windows)]
    pub fn signal_source() -> Box<dyn platform::events::SignalSource> {
        Box::new(platform_windows::WindowsSignalSource)
    }

    #[cfg(target_os = "linux")]
    pub fn terminal() -> Box<dyn platform::term::Terminal> {
        Box::new(platform_linux::LinuxTerminal::new())
    }

    #[cfg(windows)]
    pub fn terminal() -> Box<dyn platform::term::Terminal> {
        Box::new(platform_windows::WindowsTerminal::new())
    }
}
