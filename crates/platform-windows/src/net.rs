//! `Net`/`TcpStream`/`TcpListener`/`UnixStream`/`UnixListener` trait
//! impls over the sys layer. No `unsafe` here.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use platform::error::Result;
use platform::net::{Net, TcpListener, TcpStream, UnixListener, UnixStream};

use crate::sys::net as sysnet;

/// The Windows backend's [`Net`] capability. Stateless — every
/// operation is a fresh Winsock call, mirroring
/// [`crate::WindowsSpawner`].
pub struct WindowsNet;

impl Net for WindowsNet {
    fn tcp_connect(&self, addr: SocketAddr) -> Result<Box<dyn TcpStream>> {
        let sock = sysnet::tcp_connect(addr)?;
        Ok(Box::new(WindowsTcpStream { sock }))
    }

    fn tcp_listen(&self, addr: SocketAddr) -> Result<Box<dyn TcpListener>> {
        let sock = sysnet::tcp_listen(addr)?;
        Ok(Box::new(WindowsTcpListener { sock }))
    }

    fn unix_connect(&self, path: &Path) -> Result<Box<dyn UnixStream>> {
        let sock = sysnet::unix_connect(path)?;
        Ok(Box::new(WindowsUnixStream { sock }))
    }

    fn unix_listen(&self, path: &Path) -> Result<Box<dyn UnixListener>> {
        let sock = sysnet::unix_listen(path)?;
        Ok(Box::new(WindowsUnixListener { sock }))
    }
}

/// A connected TCP stream backed by an owned Winsock socket.
pub struct WindowsTcpStream {
    sock: sysnet::OwnedSocket,
}

impl TcpStream for WindowsTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        sysnet::read(&self.sock, buf)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        sysnet::write(&self.sock, buf)
    }

    fn set_nodelay(&self, nodelay: bool) -> Result<()> {
        sysnet::set_nodelay(&self.sock, nodelay)
    }

    fn peer_addr(&self) -> Result<SocketAddr> {
        sysnet::peer_addr(&self.sock)
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        sysnet::local_addr(&self.sock)
    }
}

/// A listening TCP socket backed by an owned Winsock socket.
pub struct WindowsTcpListener {
    sock: sysnet::OwnedSocket,
}

impl TcpListener for WindowsTcpListener {
    fn accept(&self) -> Result<(Box<dyn TcpStream>, SocketAddr)> {
        let (sock, peer) = sysnet::tcp_accept(&self.sock)?;
        Ok((Box::new(WindowsTcpStream { sock }), peer))
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        sysnet::local_addr(&self.sock)
    }
}

/// A connected Unix domain stream socket backed by an owned Winsock
/// socket.
pub struct WindowsUnixStream {
    sock: sysnet::OwnedSocket,
}

impl UnixStream for WindowsUnixStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        sysnet::read(&self.sock, buf)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        sysnet::write(&self.sock, buf)
    }

    fn peer_addr(&self) -> Result<Option<PathBuf>> {
        sysnet::unix_peer_addr(&self.sock)
    }

    fn local_addr(&self) -> Result<Option<PathBuf>> {
        sysnet::unix_local_addr(&self.sock)
    }
}

/// A listening Unix domain socket backed by an owned Winsock socket.
pub struct WindowsUnixListener {
    sock: sysnet::OwnedSocket,
}

impl UnixListener for WindowsUnixListener {
    fn accept(&self) -> Result<(Box<dyn UnixStream>, Option<PathBuf>)> {
        let (sock, peer) = sysnet::unix_accept(&self.sock)?;
        Ok((Box::new(WindowsUnixStream { sock }), peer))
    }

    fn local_addr(&self) -> Result<Option<PathBuf>> {
        sysnet::unix_local_addr(&self.sock)
    }
}
