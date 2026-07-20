//! Net parity suite (RFC v2 R5+, D16): behavior-spec-derived assertion
//! sets run against every backend, the same shape the Fs suite
//! established. Kept as its own file rather than folded into
//! `parity.rs`: Net is a distinct RFC domain (R5+, gated on named
//! consumers), not a growing corner of the Fs surface.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use platform::error::ErrorKind;
use platform::net::Net;

/// TCP's assertion set. Unix sockets and UDP each get their own
/// (`assert_unix_behavior`/`assert_udp_behavior`, below) rather than
/// sharing this one — the same judgment call `assert_fs_behavior`
/// already made once for symlinks/access, made here because neither
/// shares much behavior with TCP: Unix sockets address by path instead
/// of `SocketAddr` and have no `set_nodelay`; UDP has no handshake at
/// all.
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

/// Unix domain sockets' assertion set: connect/accept, the unnamed-peer
/// case a plain `unix_connect` client always hits, refusal once the
/// listener drops, and — the behavior this slice exists for —
/// stale-cleanup bind reclaiming the leftover path afterward.
fn assert_unix_behavior(net: &dyn Net) {
    let path = std::env::temp_dir().join(format!("rustils-net-parity-{}.sock", std::process::id()));
    // A leftover from a previous run (or a prior call in the same test
    // binary) is exactly what stale-cleanup bind exists for — no
    // manual removal needed here, but start from a known state anyway
    // so a failed run's leftover doesn't mask a real bug on a rerun.
    let _ = std::fs::remove_file(&path);

    let listener = net.unix_listen(&path).expect("listen");
    let local = listener.local_addr().expect("local_addr");
    assert_eq!(local.as_deref(), Some(path.as_path()));

    let handle = std::thread::spawn(move || {
        let (mut stream, peer) = listener.accept().expect("accept");
        // A plain `unix_connect` client never binds its own name —
        // the accepted peer is unnamed, a legal `AF_UNIX` state with
        // no TCP equivalent.
        assert_eq!(peer, None, "an unbound connecting client has no peer path");
        let mut buf = [0u8; 16];
        let n = stream.read(&mut buf).expect("server read");
        assert_eq!(&buf[..n], b"ping");
        stream.write(b"pong").expect("server write");
    });

    let mut client = net.unix_connect(&path).expect("connect");
    assert_eq!(
        client.peer_addr().expect("peer_addr").as_deref(),
        Some(path.as_path())
    );
    assert_eq!(client.local_addr().expect("local_addr"), None);

    client.write(b"ping").expect("client write");
    let mut buf = [0u8; 16];
    let n = client.read(&mut buf).expect("client read");
    assert_eq!(&buf[..n], b"pong");
    handle.join().expect("server thread panicked");
    drop(client);

    // Nothing is listening at this path once the listener above drops
    // — connecting must fail, not hang or silently succeed.
    let e = net
        .unix_connect(&path)
        .err()
        .expect("must refuse: nothing listening");
    assert_eq!(e.kind, ErrorKind::ConnectionRefused);

    // Stale-cleanup bind: the path above still names a real (if now
    // stale) socket file — dropping a listener never unlinks it. A
    // fresh `unix_listen` on the same path must reclaim it rather than
    // fail `AddrInUse`.
    let listener2 = net
        .unix_listen(&path)
        .expect("stale-cleanup bind must reclaim the leftover path");
    drop(listener2);

    let _ = std::fs::remove_file(&path);
}

/// UDP's own assertion set, kept separate from `assert_net_behavior`
/// (the judgment call that doc comment already flags): UDP shares
/// almost nothing behaviorally with TCP/Unix streams — no handshake, no
/// `set_nodelay`, and critically, `send_to` to an address nothing is
/// bound to is fire-and-forget, not a failure, the opposite of
/// `tcp_connect`/`unix_connect`'s "nothing listening" case.
fn assert_udp_behavior(net: &dyn Net) {
    let server = net
        .udp_bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .expect("bind");
    let server_addr = server.local_addr().expect("local_addr");
    assert_ne!(server_addr.port(), 0, "an ephemeral port was assigned");

    let client = net
        .udp_bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .expect("bind");
    let client_addr = client.local_addr().expect("local_addr");

    client.send_to(b"ping", server_addr).expect("send_to");
    let mut buf = [0u8; 16];
    let (n, peer) = server.recv_from(&mut buf).expect("recv_from");
    assert_eq!(&buf[..n], b"ping");
    assert_eq!(peer.ip(), client_addr.ip());
    assert_eq!(peer.port(), client_addr.port());

    server.send_to(b"pong", client_addr).expect("send_to");
    let (n, peer) = client.recv_from(&mut buf).expect("recv_from");
    assert_eq!(&buf[..n], b"pong");
    assert_eq!(peer.ip(), server_addr.ip());
    assert_eq!(peer.port(), server_addr.port());

    // Fire-and-forget: sending to an address nothing is bound to must
    // not error — there is no handshake to fail the way TCP/Unix
    // connect has.
    let nobody = net
        .udp_bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .expect("bind");
    let nobody_addr = nobody.local_addr().expect("local_addr");
    drop(nobody);
    client
        .send_to(b"into the void", nobody_addr)
        .expect("send_to must not fail just because nothing is bound there");
}

#[test]
fn mock_net_conforms() {
    assert_net_behavior(&platform_mock::MockNet);
}

#[test]
fn mock_unix_conforms() {
    assert_unix_behavior(&platform_mock::MockNet);
}

#[test]
fn mock_udp_conforms() {
    assert_udp_behavior(&platform_mock::MockNet);
}

#[cfg(target_os = "linux")]
#[test]
fn linux_net_conforms() {
    assert_net_behavior(&platform_linux::LinuxNet);
}

#[cfg(target_os = "linux")]
#[test]
fn linux_unix_conforms() {
    assert_unix_behavior(&platform_linux::LinuxNet);
}

#[cfg(target_os = "linux")]
#[test]
fn linux_udp_conforms() {
    assert_udp_behavior(&platform_linux::LinuxNet);
}
