//! In-memory TCP implementation (RFC v2 R5+, D16). No real sockets: a
//! process-global registry of listening addresses, and a duplex byte
//! channel per accepted connection — the same "real behavior, no OS
//! calls" contract [`crate::MockDir`] has for the filesystem.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use platform::error::{ErrorKind, OsCode, PlatformError, Result};
use platform::net::{Net, TcpListener, TcpStream};

type Registry = Mutex<HashMap<SocketAddr, Sender<(MockTcpStream, SocketAddr)>>>;

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Ephemeral port allocator for `tcp_listen(.., port: 0)` and every
/// `tcp_connect`'s synthesized client address — mirrors a real OS's
/// ephemeral range existing, without claiming to match its actual
/// bounds (no consumer needs that precision from the mock).
fn next_ephemeral_port() -> u16 {
    static NEXT: AtomicU16 = AtomicU16::new(40000);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn err(kind: ErrorKind, op: &'static str) -> PlatformError {
    PlatformError::new(kind, OsCode::None, op)
}

/// The mock backend's [`Net`] capability.
pub struct MockNet;

impl Net for MockNet {
    fn tcp_connect(&self, addr: SocketAddr) -> Result<Box<dyn TcpStream>> {
        let listener_tx = {
            let reg = registry().lock().expect("mock lock");
            reg.get(&addr)
                .cloned()
                .ok_or_else(|| err(ErrorKind::ConnectionRefused, "connect"))?
        };
        let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), next_ephemeral_port());
        let (c2s_tx, c2s_rx) = mpsc::channel();
        let (s2c_tx, s2c_rx) = mpsc::channel();
        let server_end = MockTcpStream {
            tx: s2c_tx,
            rx: c2s_rx,
            local: addr,
            peer: client_addr,
            read_buf: Vec::new(),
            read_pos: 0,
        };
        let client_end = MockTcpStream {
            tx: c2s_tx,
            rx: s2c_rx,
            local: client_addr,
            peer: addr,
            read_buf: Vec::new(),
            read_pos: 0,
        };
        // The listener side may have dropped (no one accepting anymore)
        // — the same "nothing there" outcome a real refused connection
        // gives.
        listener_tx
            .send((server_end, client_addr))
            .map_err(|_| err(ErrorKind::ConnectionRefused, "connect"))?;
        Ok(Box::new(client_end))
    }

    fn tcp_listen(&self, addr: SocketAddr) -> Result<Box<dyn TcpListener>> {
        let addr = if addr.port() == 0 {
            SocketAddr::new(addr.ip(), next_ephemeral_port())
        } else {
            addr
        };
        let (tx, rx) = mpsc::channel();
        let mut reg = registry().lock().expect("mock lock");
        if reg.contains_key(&addr) {
            return Err(err(ErrorKind::AddrInUse, "listen"));
        }
        reg.insert(addr, tx);
        Ok(Box::new(MockTcpListener {
            addr,
            rx: Mutex::new(rx),
        }))
    }
}

/// An in-memory duplex TCP stream: chunks written on one end arrive,
/// in order, as `read` results on the other.
pub struct MockTcpStream {
    tx: Sender<Vec<u8>>,
    rx: Receiver<Vec<u8>>,
    local: SocketAddr,
    peer: SocketAddr,
    read_buf: Vec<u8>,
    read_pos: usize,
}

impl TcpStream for MockTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.read_pos >= self.read_buf.len() {
            match self.rx.recv() {
                Ok(chunk) => {
                    self.read_buf = chunk;
                    self.read_pos = 0;
                }
                // The peer dropped its stream: end of stream, like a
                // real closed socket's read returning 0.
                Err(_) => return Ok(0),
            }
        }
        let n = buf.len().min(self.read_buf.len() - self.read_pos);
        buf[..n].copy_from_slice(&self.read_buf[self.read_pos..self.read_pos + n]);
        self.read_pos += n;
        Ok(n)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.tx
            .send(buf.to_vec())
            .map_err(|_| err(ErrorKind::BrokenPipe, "write"))?;
        Ok(buf.len())
    }

    fn set_nodelay(&self, _nodelay: bool) -> Result<()> {
        // No buffering to disable in an in-memory channel.
        Ok(())
    }

    fn peer_addr(&self) -> Result<SocketAddr> {
        Ok(self.peer)
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.local)
    }
}

/// An in-memory listening "socket": a registry entry plus the channel
/// `accept` blocks on.
pub struct MockTcpListener {
    addr: SocketAddr,
    rx: Mutex<Receiver<(MockTcpStream, SocketAddr)>>,
}

impl TcpListener for MockTcpListener {
    fn accept(&self) -> Result<(Box<dyn TcpStream>, SocketAddr)> {
        let rx = self.rx.lock().expect("mock lock");
        match rx.recv() {
            Ok((stream, peer)) => Ok((Box::new(stream), peer)),
            Err(_) => Err(err(ErrorKind::Other, "accept")),
        }
    }

    fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.addr)
    }
}

impl Drop for MockTcpListener {
    fn drop(&mut self) {
        // Free the address so a later `tcp_listen` on the same addr in
        // the same test process doesn't spuriously see AddrInUse.
        registry().lock().expect("mock lock").remove(&self.addr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_accept_and_echo_round_trip() {
        let net = MockNet;
        let listener = net
            .tcp_listen(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .expect("listen");
        let addr = listener.local_addr().expect("local_addr");
        assert_ne!(addr.port(), 0, "an ephemeral port was assigned");

        let handle = std::thread::spawn(move || {
            let (mut stream, _peer) = listener.accept().expect("accept");
            let mut buf = [0u8; 16];
            let n = stream.read(&mut buf).expect("read");
            assert_eq!(&buf[..n], b"ping");
            stream.write(b"pong").expect("write");
        });

        let mut client = net.tcp_connect(addr).expect("connect");
        assert_eq!(client.peer_addr().unwrap(), addr);
        client
            .set_nodelay(true)
            .expect("set_nodelay is a no-op, not an error");
        client.write(b"ping").expect("write");
        let mut buf = [0u8; 16];
        let n = client.read(&mut buf).expect("read");
        assert_eq!(&buf[..n], b"pong");
        handle.join().expect("server thread");
    }

    #[test]
    fn connect_with_nothing_listening_is_connection_refused() {
        let net = MockNet;
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 59999);
        // `Box<dyn TcpStream>` isn't `Debug`, so `expect_err` (which
        // needs the `Ok` side to be) doesn't fit — `.err().expect(..)`
        // sidesteps that the same way `fs.rs`'s own tests do.
        let e = net.tcp_connect(addr).err().expect("nothing is listening");
        assert_eq!(e.kind, ErrorKind::ConnectionRefused);
    }

    #[test]
    fn listen_twice_on_the_same_addr_is_addr_in_use() {
        let net = MockNet;
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 59998);
        let first = net.tcp_listen(addr).expect("first listen");
        let e = net.tcp_listen(addr).err().expect("already listening");
        assert_eq!(e.kind, ErrorKind::AddrInUse);
        drop(first);
        // Dropping the first listener frees the address for reuse.
        net.tcp_listen(addr).expect("listen again after drop");
    }

    #[test]
    fn closing_the_peer_reports_end_of_stream() {
        let net = MockNet;
        let listener = net
            .tcp_listen(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .expect("listen");
        let addr = listener.local_addr().expect("local_addr");
        let handle = std::thread::spawn(move || {
            let (_stream, _peer) = listener.accept().expect("accept");
            // Dropped immediately: the client's next read must see EOF.
        });
        let mut client = net.tcp_connect(addr).expect("connect");
        handle.join().expect("server thread");
        let mut buf = [0u8; 16];
        // The server thread has already exited and dropped its stream
        // by the time `join` returns, so this read is deterministic,
        // not a race against the drop.
        let n = client.read(&mut buf).expect("read after peer close");
        assert_eq!(n, 0);
    }
}
