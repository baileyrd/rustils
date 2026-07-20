//! Raw TCP socket primitives (RFC v2 R5+, D16). Not track-p-gated — see
//! `ffi::libc_surface`'s doc comment for why (sockets were never in
//! rush's required surface, so `rusty_libc` has nothing to route
//! through here yet).

#![allow(unsafe_code)]

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::os::fd::{FromRawFd, OwnedFd};

use platform::error::{ErrorKind, OsCode, PlatformError, Result};

use crate::ffi::libc_surface as c;

fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

fn kind_of(errno: i32) -> ErrorKind {
    match errno {
        libc::ECONNREFUSED => ErrorKind::ConnectionRefused,
        libc::ECONNRESET => ErrorKind::ConnectionReset,
        libc::ECONNABORTED => ErrorKind::ConnectionAborted,
        libc::ENOTCONN => ErrorKind::NotConnected,
        libc::EADDRINUSE => ErrorKind::AddrInUse,
        libc::EADDRNOTAVAIL => ErrorKind::AddrNotAvailable,
        libc::ETIMEDOUT => ErrorKind::TimedOut,
        libc::EACCES | libc::EPERM => ErrorKind::PermissionDenied,
        libc::EINVAL => ErrorKind::InvalidInput,
        libc::EAGAIN => ErrorKind::WouldBlock,
        libc::EINTR => ErrorKind::Interrupted,
        _ => ErrorKind::Other,
    }
}

fn net_err(op: &'static str) -> PlatformError {
    let e = errno();
    PlatformError::new(kind_of(e), OsCode::Errno(e), op)
}

/// Pack a [`SocketAddr`] into a kernel-layout `sockaddr_storage` and the
/// length of the variant actually filled in (`sockaddr_in` or
/// `sockaddr_in6`) — the pair `connect`/`bind` want.
fn to_sockaddr(addr: SocketAddr) -> (c::sockaddr_storage, c::socklen_t) {
    // SAFETY: not actually unsafe to construct — `sockaddr_storage` is
    // plain-old-data for which all-zeroes is a valid (if meaningless)
    // value; only the variant `ss_family` selects is ever read back.
    let mut storage: c::sockaddr_storage = unsafe { std::mem::zeroed() };
    let len = match addr {
        SocketAddr::V4(v4) => {
            let sin = c::sockaddr_in {
                sin_family: c::AF_INET as u16,
                sin_port: v4.port().to_be(),
                sin_addr: libc::in_addr {
                    // `s_addr` is a raw 32-bit blob whose in-memory bytes
                    // must equal the address octets in order —
                    // `from_ne_bytes` reproduces exactly that byte
                    // pattern on any host, not a byte-order conversion.
                    s_addr: u32::from_ne_bytes(v4.ip().octets()),
                },
                sin_zero: [0; 8],
            };
            // SAFETY: `storage` is a valid, large-enough, suitably
            // aligned buffer for any sockaddr variant on this platform
            // (that is `sockaddr_storage`'s documented purpose); writing
            // a `sockaddr_in` into its start and reading it back as one
            // is exactly how the kernel itself interprets the buffer
            // once `ss_family` says `AF_INET`.
            unsafe {
                std::ptr::write(
                    (&mut storage as *mut c::sockaddr_storage).cast::<c::sockaddr_in>(),
                    sin,
                );
            }
            std::mem::size_of::<c::sockaddr_in>()
        }
        SocketAddr::V6(v6) => {
            let sin6 = c::sockaddr_in6 {
                sin6_family: c::AF_INET6 as u16,
                sin6_port: v6.port().to_be(),
                sin6_flowinfo: v6.flowinfo(),
                sin6_addr: libc::in6_addr {
                    s6_addr: v6.ip().octets(),
                },
                sin6_scope_id: v6.scope_id(),
            };
            // SAFETY: see the V4 arm above; the same reasoning applies
            // to the `sockaddr_in6` variant.
            unsafe {
                std::ptr::write(
                    (&mut storage as *mut c::sockaddr_storage).cast::<c::sockaddr_in6>(),
                    sin6,
                );
            }
            std::mem::size_of::<c::sockaddr_in6>()
        }
    };
    (storage, len as c::socklen_t)
}

/// Unpack a kernel-filled `sockaddr_storage` (from `accept`/
/// `getpeername`/`getsockname`) back into a [`SocketAddr`].
fn from_sockaddr(storage: &c::sockaddr_storage) -> Result<SocketAddr> {
    match i32::from(storage.ss_family) {
        c::AF_INET => {
            // SAFETY: `ss_family == AF_INET` means the kernel filled
            // this buffer as a `sockaddr_in`, which is no larger than
            // `sockaddr_storage`; reading it back as that type is the
            // same reinterpretation `to_sockaddr` writes as.
            let sin = unsafe { &*(storage as *const c::sockaddr_storage).cast::<c::sockaddr_in>() };
            let ip = Ipv4Addr::from(sin.sin_addr.s_addr.to_ne_bytes());
            Ok(SocketAddr::V4(SocketAddrV4::new(
                ip,
                u16::from_be(sin.sin_port),
            )))
        }
        c::AF_INET6 => {
            // SAFETY: see the V4 arm above, for `sockaddr_in6`.
            let sin6 =
                unsafe { &*(storage as *const c::sockaddr_storage).cast::<c::sockaddr_in6>() };
            let ip = Ipv6Addr::from(sin6.sin6_addr.s6_addr);
            Ok(SocketAddr::V6(SocketAddrV6::new(
                ip,
                u16::from_be(sin6.sin6_port),
                sin6.sin6_flowinfo,
                sin6.sin6_scope_id,
            )))
        }
        _ => Err(PlatformError::new(
            ErrorKind::Other,
            OsCode::None,
            "unrecognized address family",
        )),
    }
}

fn new_tcp_socket(addr: SocketAddr) -> Result<OwnedFd> {
    let family = match addr {
        SocketAddr::V4(_) => c::AF_INET,
        SocketAddr::V6(_) => c::AF_INET6,
    };
    // SAFETY: plain integer arguments, no memory referenced.
    let fd = unsafe { c::socket(family, c::SOCK_STREAM | c::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        return Err(net_err("socket"));
    }
    // SAFETY: `fd` is a freshly returned, valid, otherwise-unowned
    // descriptor; wrapped exactly once.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// `socket` + `connect`, blocking until the connection completes or
/// fails.
pub fn tcp_connect(addr: SocketAddr) -> Result<OwnedFd> {
    use std::os::fd::AsRawFd;
    let fd = new_tcp_socket(addr)?;
    let (storage, len) = to_sockaddr(addr);
    // SAFETY: `storage` holds a valid `sockaddr_in`/`sockaddr_in6` for
    // exactly the first `len` bytes (`to_sockaddr`'s contract); `fd` is
    // a freshly created, valid socket.
    let r = unsafe {
        c::connect(
            fd.as_raw_fd(),
            (&storage as *const c::sockaddr_storage).cast::<c::sockaddr>(),
            len,
        )
    };
    if r < 0 {
        return Err(net_err("connect"));
    }
    Ok(fd)
}

/// `socket` + `SO_REUSEADDR` + `bind` + `listen(SOMAXCONN)`.
pub fn tcp_listen(addr: SocketAddr) -> Result<OwnedFd> {
    use std::os::fd::AsRawFd;
    let fd = new_tcp_socket(addr)?;
    let reuse: c::c_int = 1;
    // SAFETY: `&reuse` is a valid `c_int`-sized buffer outliving the
    // call; `fd` is a valid, freshly created socket.
    let r = unsafe {
        c::setsockopt(
            fd.as_raw_fd(),
            c::SOL_SOCKET,
            c::SO_REUSEADDR,
            (&reuse as *const c::c_int).cast(),
            std::mem::size_of::<c::c_int>() as c::socklen_t,
        )
    };
    if r < 0 {
        return Err(net_err("setsockopt(SO_REUSEADDR)"));
    }

    let (storage, len) = to_sockaddr(addr);
    // SAFETY: see `tcp_connect`.
    let r = unsafe {
        c::bind(
            fd.as_raw_fd(),
            (&storage as *const c::sockaddr_storage).cast::<c::sockaddr>(),
            len,
        )
    };
    if r < 0 {
        return Err(net_err("bind"));
    }
    // SAFETY: `fd` is a valid, bound socket.
    let r = unsafe { c::listen(fd.as_raw_fd(), c::SOMAXCONN) };
    if r < 0 {
        return Err(net_err("listen"));
    }
    Ok(fd)
}

/// `accept4` with `SOCK_CLOEXEC`, returning the accepted connection and
/// the peer's address.
pub fn tcp_accept(listen_fd: &OwnedFd) -> Result<(OwnedFd, SocketAddr)> {
    use std::os::fd::AsRawFd;
    // SAFETY: `storage`/`len` are valid, exclusively borrowed out-params
    // the kernel fills; `listen_fd` is a valid, listening socket.
    let (fd, storage) = unsafe {
        let mut storage: c::sockaddr_storage = std::mem::zeroed();
        let mut len = std::mem::size_of::<c::sockaddr_storage>() as c::socklen_t;
        let fd = c::accept4(
            listen_fd.as_raw_fd(),
            (&mut storage as *mut c::sockaddr_storage).cast::<c::sockaddr>(),
            &mut len,
            c::SOCK_CLOEXEC,
        );
        (fd, storage)
    };
    if fd < 0 {
        return Err(net_err("accept4"));
    }
    // SAFETY: `fd` is a freshly returned, valid, otherwise-unowned
    // descriptor; wrapped exactly once.
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    let peer = from_sockaddr(&storage)?;
    Ok((owned, peer))
}

/// `setsockopt(IPPROTO_TCP, TCP_NODELAY, ...)`.
pub fn set_nodelay(fd: &OwnedFd, nodelay: bool) -> Result<()> {
    use std::os::fd::AsRawFd;
    let value: c::c_int = c::c_int::from(nodelay);
    // SAFETY: `&value` is a valid `c_int`-sized buffer outliving the
    // call; `fd` is caller-owned.
    let r = unsafe {
        c::setsockopt(
            fd.as_raw_fd(),
            c::IPPROTO_TCP,
            c::TCP_NODELAY,
            (&value as *const c::c_int).cast(),
            std::mem::size_of::<c::c_int>() as c::socklen_t,
        )
    };
    if r < 0 {
        return Err(net_err("setsockopt(TCP_NODELAY)"));
    }
    Ok(())
}

/// `getpeername`.
pub fn peer_addr(fd: &OwnedFd) -> Result<SocketAddr> {
    use std::os::fd::AsRawFd;
    // SAFETY: `storage`/`len` are valid, exclusively borrowed out-params
    // the kernel fills; `fd` is a valid, connected socket.
    let storage = unsafe {
        let mut storage: c::sockaddr_storage = std::mem::zeroed();
        let mut len = std::mem::size_of::<c::sockaddr_storage>() as c::socklen_t;
        let r = c::getpeername(
            fd.as_raw_fd(),
            (&mut storage as *mut c::sockaddr_storage).cast::<c::sockaddr>(),
            &mut len,
        );
        if r < 0 {
            return Err(net_err("getpeername"));
        }
        storage
    };
    from_sockaddr(&storage)
}

/// `getsockname`.
pub fn local_addr(fd: &OwnedFd) -> Result<SocketAddr> {
    use std::os::fd::AsRawFd;
    // SAFETY: see `peer_addr`.
    let storage = unsafe {
        let mut storage: c::sockaddr_storage = std::mem::zeroed();
        let mut len = std::mem::size_of::<c::sockaddr_storage>() as c::socklen_t;
        let r = c::getsockname(
            fd.as_raw_fd(),
            (&mut storage as *mut c::sockaddr_storage).cast::<c::sockaddr>(),
            &mut len,
        );
        if r < 0 {
            return Err(net_err("getsockname"));
        }
        storage
    };
    from_sockaddr(&storage)
}
