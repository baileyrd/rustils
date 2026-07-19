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
//! ## Status: skeleton
//!
//! The `Dir`/`File` impls over `CreateFileW`-relative opens are R1 work
//! per the roadmap and are developed on a Windows host — this crate
//! currently compiles to an empty library elsewhere, and CI's Windows leg
//! is where its tests run. The module layout and the wide-string util are
//! laid down now because every future piece hangs from them.

#![cfg(windows)]
#![deny(unsafe_code)] // opted back in, narrowly, inside sys/ modules only

pub mod ffi;
pub mod sys;
pub mod util;
