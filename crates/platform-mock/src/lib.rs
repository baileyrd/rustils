//! # platform-mock — the in-memory backend
//!
//! Implements the `platform` traits with no OS interaction: a virtual
//! filesystem tree and a scripted process table. This crate is why the
//! trait surface must be instance-based and object-safe (RFC v2 §5.1) —
//! it is injected wherever a real backend would be, and consumer logic is
//! unit-tested against it deterministically and in milliseconds.
//!
//! The v1 scaffold *stated* "backends can be mocked or swapped" as a design
//! goal while its static-method traits made that impossible. This crate is
//! that goal made structural.

#![forbid(unsafe_code)]

pub mod fs;
pub mod process;
pub mod signals;

pub use fs::MockDir;
pub use process::MockSpawner;
pub use signals::MockSignalSource;
