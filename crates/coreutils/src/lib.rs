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
