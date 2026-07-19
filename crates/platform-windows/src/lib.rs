//! # platform-windows — the Windows backend
//!
//! Layering (RFC v2 §4.1): `ffi` (curated windows-sys surface) → `sys`
//! (safe wrappers; all `unsafe` lives there with documented invariants) →
//! trait impls at the crate root.
//!
//! Tier doctrine (RFC v2 §2, decision D-1): `windows-sys` *is* the raw
//! floor on Windows — metadata-generated bindings are machine-known facts;
//! the hand-rolled value of this crate begins above them: typed handles,
//! lifetimes, error mapping, and (post-R2-hoist) the `winargv` quoting
//! module, which is this crate's security boundary.
//!
//! ## Status: Dir/File landed (R1)
//!
//! The `Dir`/`File` impls run over `NtCreateFile` handle-relative opens
//! (`sys::nt`; the admission rationale lives in `ffi::nt_surface`) with
//! Win32 handle-based APIs for everything after the open. Developed from a
//! Linux host against `cargo check --target x86_64-pc-windows-gnu`; CI's
//! Windows leg is where the tests actually run.

#![cfg(windows)]
#![deny(unsafe_code)] // opted back in, narrowly, inside sys/ modules only

pub mod ffi;
pub mod fs;
pub mod sys;
pub mod util;

pub use fs::WindowsDir;
