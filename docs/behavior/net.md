# Behavior Spec — net (Net / TcpStream / TcpListener)

The parity suite (`crates/platform-linux/tests/net_parity.rs` and
`crates/platform-windows/tests/net_parity.rs`, kept textually identical —
the same convention `parity.rs` established) asserts this spec against
every backend. A backend that cannot honor a line gets a numbered entry in
`../divergences.md` citing the OS limitation — never implementation
convenience.

## Scope (Slice 1 — TCP only)

RFC v2 R5+, decision D16. Four named consumers (shh, rusty_tail,
rusty_rdp, rusty_llama's optional server) define this domain's shape, and
none of them need TLS in the trait — all four bring their own wire crypto
or inject TLS separately, so `Net`/`TcpStream`/`TcpListener` carry no TLS
concept at all. This slice covers only TCP connect/listen/accept/
`set_nodelay`; UDP datagram and Unix domain sockets (mode bits +
stale-cleanup bind) are deferred to future slices, the same phased-slicing
pattern the Fs surface used for symlinks and `access`.

## Specified

- `Net::tcp_connect(addr)` opens a TCP connection to `addr`. Nothing
  listening (or a listener that has since dropped) fails
  `ConnectionRefused`, never hangs.
- `Net::tcp_listen(addr)` binds and starts listening at `addr`. A second
  `tcp_listen` on an address already bound by a live listener in the same
  process fails `AddrInUse`; dropping the first listener frees the
  address for reuse.
- `TcpListener::accept()` blocks until a peer connects, returning the new
  `TcpStream` and the peer's address.
- `TcpStream::read`/`write` are byte-faithful, ordinary blocking I/O — no
  framing, no encoding. A dropped peer is observed as a `read` returning
  `Ok(0)` (end of stream), not an error.
- `TcpStream::set_nodelay` toggles Nagle's algorithm; the call itself
  always succeeds when the stream is valid — the underlying OS-level
  meaning of the flag is not asserted here.
- `TcpStream::peer_addr`/`local_addr` and `TcpListener::local_addr`
  report the real (possibly OS-assigned ephemeral) socket address.
  `tcp_listen` on port `0` gets a real ephemeral port; `local_addr` after
  `listen` never reports port `0`.
- `TcpStream: Send` and `TcpListener: Send` (unlike `Dir`/`Child`, which
  don't cross threads in this codebase): the standard "accept on one
  thread, hand the connection to a worker thread" server pattern is the
  entire reason this surface exists, per the roadmap, so both are
  required to be movable across threads by the trait itself, not left to
  each backend to happen to get right.

## Deliberately unspecified

- Any TLS/crypto behavior — out of scope for this trait by design (see
  Scope above); consumers layer their own wire security over the plain
  `TcpStream`.
- UDP datagram and Unix domain socket semantics — not yet part of this
  surface (future slices).
- Exact OS-level effect of `set_nodelay(false)` re-enabling Nagle's
  algorithm — asserted only as "the call does not error," not as an
  observable timing behavior, since that would make the parity suite
  flaky.
- Backlog size, accept queue behavior under load, and other OS-tunable
  socket options beyond `SO_REUSEADDR` (used internally by `tcp_listen`)
  and `TCP_NODELAY`.
