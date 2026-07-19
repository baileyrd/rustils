//! `Net`/`TcpStream`/`TcpListener` trait impls over the sys layer. No
//! `unsafe` here.

use std::net::SocketAddr;
use std::os::fd::OwnedFd;

use platform::error::Result;
use platform::net::{Net, TcpListener, TcpStream};

use crate::sys::fdio;
use crate::sys::net as sysnet;

/// The Linux backend's [`Net`] capability. Stateless — every operation
/// is a fresh syscall, mirroring how [`crate::LinuxSpawner`] carries no
/// state of its own either.
pub struct LinuxNet;

impl Net for LinuxNet {
    fn tcp_connect(&self, addr: SocketAddr) -> Result<Box<dyn TcpStream>> {
        let fd = sysnet::tcp_connect(addr)?;
        Ok(Box::new(LinuxTcpStream { fd }))
    }

    fn tcp_listen(&self, addr: SocketAddr) -> Result<Box<dyn TcpListener>> {
        let fd = sysnet::tcp_listen(addr)?;
        Ok(Box::new(LinuxTcpListener { fd }))
    }
}

/// A connected TCP stream backed by an `OwnedFd`. Public for std interop
/// (RFC v2 §5.1), the same reasoning `LinuxFile` documents.
pub struct LinuxTcpStream {
    fd: OwnedFd,
}

impl TcpStream for LinuxTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        // A connected socket's fd is read/written exactly like a plain
        // fd — no socket-specific syscall needed for the byte-transfer
        // path itself, only for setup/teardown/options.
        fdio::read(&self.fd, buf)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        fdio::write(&self.fd, buf)
    }

    fn set_nodelay(&self, nodelay: bool) -> Result<()> {
        sysnet::set_nodelay(&self.fd, nodelay)
    }

    fn peer_addr(&self) -> Result<SocketAddr> {
        sysnet::peer_addr(&self.fd)
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        sysnet::local_addr(&self.fd)
    }
}

/// A listening TCP socket backed by an `OwnedFd`.
pub struct LinuxTcpListener {
    fd: OwnedFd,
}

impl TcpListener for LinuxTcpListener {
    fn accept(&self) -> Result<(Box<dyn TcpStream>, SocketAddr)> {
        let (fd, peer) = sysnet::tcp_accept(&self.fd)?;
        Ok((Box::new(LinuxTcpStream { fd }), peer))
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        sysnet::local_addr(&self.fd)
    }
}
