//! TCP sockets (RFC v2 R5+, decision D16) — the first Net slice.
//!
//! Unparked only once named consumers existed to define the shape (RFC
//! v2 §3's consumer gate): shh, rusty_tail, rusty_rdp, and rusty_llama's
//! optional server all want TCP connect/listen + `set_nodelay`; none of
//! them need TLS in this trait — all four bring their own wire crypto or
//! inject TLS separately, so there is no TLS surface here or planned.
//! Unix domain sockets and UDP datagrams are separate, later slices of
//! the same D16 survey — deliberately not bundled into this one.
//!
//! `std::net::SocketAddr`/`IpAddr` are used directly in this trait's
//! signatures: unlike `std::fs`/`std::net::TcpStream` themselves, they
//! perform no I/O and own no OS handle — pure value types, the same
//! standing `OsStr`/`OsString`/`PathBuf` already have elsewhere in this
//! crate (RFC v2 §5.2's byte-oriented boundary is about *paths*, not
//! every std type). Backends still do their own socket I/O and error
//! mapping from scratch (D-1/D-2's tier doctrine) — nothing here routes
//! through `std::net`'s own sockets.

use std::net::SocketAddr;

use crate::error::Result;

/// A connected TCP stream. Object-safe; backends return `Box<dyn
/// TcpStream>`. `Send`: the standard TCP server pattern is accept on one
/// thread, hand the connection to a worker thread — every named
/// consumer here (shh, rusty_tail, rusty_rdp, rusty_llama's server)
/// does exactly that, unlike `Dir`/`Child`, which this codebase has
/// never needed to move across threads.
pub trait TcpStream: Send {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    fn write(&mut self, buf: &[u8]) -> Result<usize>;

    /// Toggle Nagle's algorithm (`TCP_NODELAY`). Meaningful for every
    /// backend here — TCP is this trait's only stream kind in this
    /// slice, so unlike a hypothetical shared stream abstraction
    /// spanning Unix-domain sockets too, there is no no-op case to
    /// document.
    fn set_nodelay(&self, nodelay: bool) -> Result<()>;

    /// The remote address this stream is connected to.
    fn peer_addr(&self) -> Result<SocketAddr>;

    /// The local address this stream is bound to.
    fn local_addr(&self) -> Result<SocketAddr>;
}

/// A listening TCP socket. Object-safe. `Send` for the same reason
/// [`TcpStream`] is — a common pattern is spawning the whole accept
/// loop onto its own background thread.
pub trait TcpListener: Send {
    /// Block until a connection arrives, returning the accepted stream
    /// and the peer's address.
    fn accept(&self) -> Result<(Box<dyn TcpStream>, SocketAddr)>;

    /// The address this listener is bound to — the OS-assigned port
    /// when `tcp_listen` was given port `0`.
    fn local_addr(&self) -> Result<SocketAddr>;
}

/// A backend capable of creating TCP streams and listeners. Object-safe.
pub trait Net {
    /// Connect to `addr`, blocking until the connection completes or
    /// fails.
    fn tcp_connect(&self, addr: SocketAddr) -> Result<Box<dyn TcpStream>>;

    /// Bind and listen at `addr` (port `0` picks an ephemeral port —
    /// query it back via [`TcpListener::local_addr`]). The backlog is
    /// backend-chosen (the OS maximum); no consumer named a need to
    /// tune it.
    fn tcp_listen(&self, addr: SocketAddr) -> Result<Box<dyn TcpListener>>;
}
