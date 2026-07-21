//! The exact libc items this backend is permitted to touch.
//!
//! Anything not re-exported here is out of bounds for `sys/` — widening
//! this list is a reviewed decision, mirroring `platform-linux`'s own
//! `libc_surface` discipline (RFC v2 §6).
//!
//! Net-only slice (rustils#48): no `fs`/`process`/`term` surface here,
//! so no `openat`/`posix_spawn`/`termios` family — only what
//! `Net`/`TcpStream`/`TcpListener`/`UnixStream`/`UnixListener`/
//! `UdpSocket` need.

pub use libc::{
    accept, bind, c_char, c_int, chmod, connect, fcntl, getpeername, getsockname, listen, mode_t,
    read, recvfrom, sendto, setsockopt, sockaddr, sockaddr_in, sockaddr_in6, sockaddr_storage,
    sockaddr_un, socket, socklen_t, suseconds_t, time_t, timeval, unlinkat, write, AF_INET,
    AF_INET6, AF_UNIX, AT_FDCWD, FD_CLOEXEC, F_GETFL, F_SETFD, F_SETFL, IPPROTO_TCP, O_NONBLOCK,
    SOCK_DGRAM, SOCK_STREAM, SOL_SOCKET, SOMAXCONN, SO_RCVTIMEO, SO_REUSEADDR, TCP_NODELAY,
};

// Deliberately NOT admitted, unlike `platform-linux`'s surface: `SOCK_CLOEXEC`,
// `SOCK_NONBLOCK`, `accept4`. Darwin has none of the three (this crate's own
// `lib.rs` doc comment has the detail) — `sys::net` builds close-on-exec via
// `fcntl(F_SETFD, FD_CLOEXEC)` after `socket`/`accept` instead, which is why
// `FD_CLOEXEC`/`F_SETFD` are admitted above alongside the `F_GETFL`/`F_SETFL`
// pair `platform-linux` already needed for its `set_nonblocking` escape hatch
// (rustils#41, ported here as part of this crate's first slice rather than a
// follow-up, per the issue's own request).
