# Behavior Spec — net (Net / TcpStream / TcpListener)

The parity suite (`crates/platform-linux/tests/net_parity.rs` and
`crates/platform-windows/tests/net_parity.rs`, kept textually identical —
the same convention `parity.rs` established) asserts this spec against
every backend. A backend that cannot honor a line gets a numbered entry in
`../divergences.md` citing the OS limitation — never implementation
convenience.

## Scope (Slice 1 — TCP, Slice 1.5 — Unix domain sockets)

RFC v2 R5+, decision D16. Four named consumers (shh, rusty_tail,
rusty_rdp, rusty_llama's optional server) define this domain's shape, and
none of them need TLS in the trait — all four bring their own wire crypto
or inject TLS separately, so `Net`/`TcpStream`/`TcpListener` carry no TLS
concept at all. This slice covers TCP connect/listen/accept/
`set_nodelay`.

Unix domain stream sockets ride along in the same D16 survey, as a
follow-on slice: `Net::unix_connect`/`unix_listen`, `UnixStream`,
`UnixListener`, mirroring `TcpStream`/`TcpListener`'s shape with
`PathBuf` addresses in place of `SocketAddr`, minus `set_nodelay` (no
Nagle buffering on `AF_UNIX` to toggle) and with mode-bit narrowing +
stale-cleanup bind semantics on `unix_listen` that `tcp_listen` has no
analog for. LocalAPI/agent-socket consumers (rusty_tail, shh) are the
named shape for this half of the slice. UDP datagram sockets remain a
separate, later slice of the same D16 survey — deliberately not bundled
into this one, the same phased-slicing pattern the Fs surface used for
symlinks and `access`.

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

### Unix domain sockets

- `Net::unix_connect(path)` connects to the Unix domain socket bound at
  `path`. A path with a live listener succeeds; a path that still names
  a real (if stale — see stale-cleanup bind semantics below) socket file
  but has nothing accepting on it fails `ConnectionRefused`, mirroring
  `tcp_connect`'s "nothing listening" case, never hangs.
- `Net::unix_listen(path)` binds and starts listening at `path`. A
  `path` already bound by a live listener — in this process or another —
  fails `AddrInUse`. A path left behind by a listener that died without
  unlinking it is reclaimed automatically instead: the kernel/Winsock
  `bind` call alone can't distinguish "a live listener still owns this
  path" from "a stale file is sitting here" (both report the identical
  collision), so `unix_listen` resolves the ambiguity itself with a
  throwaway probe connect — refused means stale, so the leftover file is
  unlinked and the bind retried exactly once; a successful probe means a
  live listener owns it, left untouched, `AddrInUse` surfaces as normal.
  Callers never need to unlink a path themselves. Unlike `tcp_listen`'s
  port `0`, there is no ephemeral-path equivalent.
- `UnixListener::accept()` blocks until a peer connects, returning the
  new `UnixStream` and the peer's bound path, or `None` when the peer
  connected from an unnamed (anonymous) `AF_UNIX` socket — a legal state
  with no TCP equivalent.
- `UnixStream::read`/`write` are byte-faithful, ordinary blocking I/O,
  exactly like `TcpStream`'s — no framing, no encoding. A dropped peer
  is observed as `read` returning `Ok(0)`, not an error.
- `UnixStream::peer_addr`/`local_addr` and `UnixListener::local_addr`
  return `Option<PathBuf>` rather than a bare `PathBuf`: `None` covers
  the unnamed-socket case above, since `AF_UNIX` (unlike TCP) permits a
  connected or even listening socket with no path bound to it.
- Mode bits: on a backend with a POSIX mode-bit model, `unix_listen`
  narrows the just-bound socket file to `0600` (owner read/write only)
  immediately after `bind`, rather than leaving it at whatever the
  process umask allows — the shape the LocalAPI/agent consumers
  (rusty_tail, shh) asked for. A backend with no POSIX-chmod equivalent
  for an `AF_UNIX` bind (Windows) has no narrowing step to perform and
  leaves the bound file at the filesystem's own ACL defaults instead;
  `unix_listen` still succeeds there — a registered divergence
  (`../divergences.md` #007), the same shape as the existing `unix_mode`
  divergence (#006).
- Error mapping mirrors `tcp_connect`/`tcp_listen`'s kinds wherever the
  underlying condition is the same: a bind collision (live or stale
  path) maps to `AddrInUse`; connecting to a socket file that exists but
  has nothing accepting maps to `ConnectionRefused`; a path the caller
  lacks permission to bind or connect to maps to `PermissionDenied`.
  These three are what each backend's own errno/WSA-code table already
  maps today; the shared parity suite (still TCP-only — see Scope) does
  not yet exercise the Unix-socket paths to pin them across backends.
- `UnixStream: Send` and `UnixListener: Send`, for the identical
  accept-on-one-thread/hand-off-to-a-worker-thread reason
  `TcpStream`/`TcpListener` are.
- No `set_nodelay` counterpart on `UnixStream`: `TCP_NODELAY` disables
  Nagle's algorithm, which only exists on TCP's byte stream over a
  network — `AF_UNIX` sockets are a local, in-kernel byte pipe with no
  Nagle buffering to toggle.

## Deliberately unspecified

- Any TLS/crypto behavior — out of scope for this trait by design (see
  Scope above); consumers layer their own wire security over the plain
  `TcpStream`.
- UDP datagram socket semantics — not yet part of this surface (a
  future slice).
- The exact `ErrorKind` `unix_connect` reports for a `path` that never
  named any socket file at all, as opposed to a stale-but-present one —
  asserted only as "fails, not hangs," not pinned to one `ErrorKind`
  across backends, since each backend's own errno/WSA-code table is
  free to draw that "nothing there" vs. "something there but not
  listening" line differently.
- The effective access control on a `unix_listen` socket file for a
  backend with no POSIX-chmod equivalent to narrow it with — that
  backend's own filesystem ACL defaults govern, not this trait.
- Whether the socket file a listener bound to is removed when the
  `UnixListener` is dropped or its owning process dies without explicit
  cleanup — not this trait's concern either way; `unix_listen`'s
  stale-cleanup bind already makes a leftover path self-healing on the
  next `unix_listen` regardless of how it was left behind.
- Exact OS-level effect of `set_nodelay(false)` re-enabling Nagle's
  algorithm — asserted only as "the call does not error," not as an
  observable timing behavior, since that would make the parity suite
  flaky.
- Backlog size, accept queue behavior under load, and other OS-tunable
  socket options beyond `SO_REUSEADDR` (used internally by `tcp_listen`)
  and `TCP_NODELAY`.
