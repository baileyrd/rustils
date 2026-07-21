//! # platform-macos — the macOS backend (net-only slice, rustils#48)
//!
//! Layering (RFC v2 §4.1, mirroring `platform-linux`): `ffi` (raw
//! bindings, curated in `ffi::libc_surface`) → `sys` (safe wrappers;
//! **all `unsafe` in this crate lives there**, each block with a
//! documented invariant) → the trait impls at the crate root, which
//! contain no `unsafe`.
//!
//! ## Scope: net only
//!
//! Forced by a real gap, not speculation: building `rusty_tokio`'s
//! kqueue reactor backend for macOS/BSD had no `platform-macos` to sit
//! on, so its socket lifecycle (`src/io/socket/macos.rs`) got hand-rolled
//! against raw `libc` a second time — the exact duplication `platform`'s
//! Net slice already solved once for Linux. `Net`/`TcpStream`/
//! `TcpListener`/`UnixStream`/`UnixListener`/`UdpSocket` is therefore all
//! this crate implements; `fs`/`process`/`security`/`term`/`signals`
//! are out of scope until a consumer forces them the same way (RFC v2
//! §3), the same discipline every other surface in this workspace
//! follows.
//!
//! ## BSD vs. Linux: the three real syscall differences
//!
//! Darwin (unlike Linux, and unlike some other BSDs — this crate only
//! claims `target_os = "macos"`, not generic BSD) has none of:
//!
//! - `SOCK_CLOEXEC`/`SOCK_NONBLOCK` socket-type flags at `socket(2)` —
//!   `fcntl(F_SETFD, FD_CLOEXEC)` after creation stands in for the
//!   former; `set_nonblocking` (the rustils#41 escape hatch, ported
//!   here too) already covers the latter via `fcntl(F_SETFL)`.
//! - `accept4(2)` — plain `accept(2)`, then the same post-creation
//!   `fcntl(F_SETFD, FD_CLOEXEC)` on the returned fd.
//! - A `sockaddr_in`/`sockaddr_in6`/`sockaddr_un` with no leading length
//!   byte — Darwin's carry `sin_len`/`sin6_len`/`sun_len`. Built via
//!   `zeroed()` + field assignment (`sys::net`) rather than a full
//!   struct literal, so the extra field never needs naming.
//!
//! Not yet cross-compiled against a real macOS SDK from this Linux
//! workspace (no linker for it here) — validated via `cargo check`/
//! `clippy --target x86_64-apple-darwin`, mirroring how
//! `platform-windows` is developed from a Linux host today. Real OS
//! testing is CI's job once a macOS runner leg exists.

#![cfg(target_os = "macos")]
#![deny(unsafe_code)] // opted back in, narrowly, inside sys/ modules only

pub mod ffi;
pub mod sys;

mod net;

pub use net::{
    MacosNet, MacosTcpListener, MacosTcpStream, MacosUdpSocket, MacosUnixListener, MacosUnixStream,
};
