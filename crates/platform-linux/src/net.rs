//! `Net`/`TcpStream`/`TcpListener` trait impls over the sys layer. No
//! `unsafe` here.

use std::net::SocketAddr;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};

use platform::error::Result;
use platform::net::{Net, TcpListener, TcpStream, UdpSocket, UnixListener, UnixStream};

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

    fn unix_connect(&self, path: &Path) -> Result<Box<dyn UnixStream>> {
        let fd = sysnet::unix_connect(path)?;
        Ok(Box::new(LinuxUnixStream { fd }))
    }

    fn unix_listen(&self, path: &Path) -> Result<Box<dyn UnixListener>> {
        let fd = sysnet::unix_listen(path)?;
        Ok(Box::new(LinuxUnixListener { fd }))
    }

    fn udp_bind(&self, addr: SocketAddr) -> Result<Box<dyn UdpSocket>> {
        let fd = sysnet::udp_bind(addr)?;
        Ok(Box::new(LinuxUdpSocket { fd }))
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

/// A connected Unix domain stream socket backed by an `OwnedFd`. Public
/// for the same std-interop reasoning `LinuxTcpStream` documents.
pub struct LinuxUnixStream {
    fd: OwnedFd,
}

impl UnixStream for LinuxUnixStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        // Same reasoning as `LinuxTcpStream::read`: a connected AF_UNIX
        // socket's fd is read exactly like a plain fd.
        fdio::read(&self.fd, buf)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        fdio::write(&self.fd, buf)
    }

    fn peer_addr(&self) -> Result<Option<PathBuf>> {
        sysnet::unix_peer_addr(&self.fd)
    }

    fn local_addr(&self) -> Result<Option<PathBuf>> {
        sysnet::unix_local_addr(&self.fd)
    }
}

/// A listening Unix domain socket backed by an `OwnedFd`.
pub struct LinuxUnixListener {
    fd: OwnedFd,
}

impl UnixListener for LinuxUnixListener {
    fn accept(&self) -> Result<(Box<dyn UnixStream>, Option<PathBuf>)> {
        let (fd, peer) = sysnet::unix_accept(&self.fd)?;
        Ok((Box::new(LinuxUnixStream { fd }), peer))
    }

    fn local_addr(&self) -> Result<Option<PathBuf>> {
        sysnet::unix_local_addr(&self.fd)
    }
}

/// A UDP datagram socket backed by an `OwnedFd`.
pub struct LinuxUdpSocket {
    fd: OwnedFd,
}

impl UdpSocket for LinuxUdpSocket {
    fn send_to(&self, buf: &[u8], addr: SocketAddr) -> Result<usize> {
        sysnet::udp_send_to(&self.fd, buf, addr)
    }

    fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        sysnet::udp_recv_from(&self.fd, buf)
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        sysnet::local_addr(&self.fd)
    }
}
