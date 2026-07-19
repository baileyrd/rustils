//! Curated windows-sys import surface — the exact APIs this backend may
//! touch. Widening this list is a reviewed decision (same discipline as
//! the Linux backend's `libc_surface`).

pub mod nt_surface;
pub mod win32_surface;
