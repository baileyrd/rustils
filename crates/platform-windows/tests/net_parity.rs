//! Net parity suite (RFC v2 R5+, D16), TCP slice: one behavior-spec-derived
//! assertion set run against every backend, the same shape the Fs suite
//! established. Kept as its own file rather than folded into
//! `parity.rs`: Net is a distinct RFC domain (R5+, gated on named
//! consumers), not a growing corner of the Fs surface.

#![cfg(windows)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use platform::error::ErrorKind;
use platform::net::Net;

/// The shared assertions. Grows with future Net slices (Unix sockets,
/// UDP) only if they turn out to share meaningful behavior with TCP —
/// otherwise they get their own function, the same judgment call
/// `assert_fs_behavior` already made once for symlinks/access.
fn assert_net_behavior(net: &dyn Net) {
    let listener = net
        .tcp_listen(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .expect("listen");
    let addr = listener.local_addr().expect("local_addr");
    assert_ne!(addr.port(), 0, "an ephemeral port was assigned");

    let handle = std::thread::spawn(move || {
        let (mut stream, peer) = listener.accept().expect("accept");
        assert_eq!(peer.ip(), addr.ip());
        let mut buf = [0u8; 16];
        let n = stream.read(&mut buf).expect("server read");
        assert_eq!(&buf[..n], b"ping");
        stream.write(b"pong").expect("server write");
    });

    let mut client = net.tcp_connect(addr).expect("connect");
    assert_eq!(client.peer_addr().expect("peer_addr"), addr);
    let local = client.local_addr().expect("local_addr");
    assert_eq!(local.ip(), addr.ip());
    assert_ne!(local.port(), 0);

    client
        .set_nodelay(true)
        .expect("set_nodelay must not error");

    client.write(b"ping").expect("client write");
    let mut buf = [0u8; 16];
    let n = client.read(&mut buf).expect("client read");
    assert_eq!(&buf[..n], b"pong");
    handle.join().expect("server thread panicked");
    drop(client);

    // Nothing is listening at this port once the listener above drops
    // — connecting must fail, not hang or silently succeed.
    let refused_addr = SocketAddr::new(addr.ip(), addr.port());
    let e = net
        .tcp_connect(refused_addr)
        .err()
        .expect("must refuse: nothing listening");
    assert_eq!(e.kind, ErrorKind::ConnectionRefused);
}

#[test]
fn mock_net_conforms() {
    assert_net_behavior(&platform_mock::MockNet);
}

#[test]
fn windows_net_conforms() {
    assert_net_behavior(&platform_windows::WindowsNet);
}
