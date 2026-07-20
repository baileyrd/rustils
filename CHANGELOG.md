# Changelog

Format loosely follows [Keep a Changelog](https://keepachangelog.com/);
version-bump rule is [`docs/versioning.md`](docs/versioning.md) §2 (at
`0.y.z`, any public-API change — additive or breaking — bumps `y`;
`z` is reserved for changes that touch no public item's shape).

This changelog starts with the adoption of that policy. Everything
before it (Fs, Process, Events, Track P, the error model, the parity
regime) landed under no formal version-bump discipline at all — it's
summarized once, below, rather than reconstructed bump-by-bump after
the fact, since nothing external ever pinned to a specific version
during that period to make the reconstruction meaningful.

Three independently-versioned lines, per `docs/versioning.md` §1:
**the PAL group** (`platform`/`platform-linux`/`platform-windows`/
`platform-mock`, sharing one number), **`winargv`**, and **`coreutils`**.

## PAL group (`platform` / `platform-linux` / `platform-windows` / `platform-mock`)

### 0.5.0

- Added `TcpStream::set_read_timeout` — an idle read timeout, forced
  by a real gap found while starting the rusty_rdp convergence
  (`examples/connect.rs` needs it; `platform::net::TcpStream` had no
  equivalent). Scoped to `TcpStream` only (RFC v2 §3 — no consumer
  has asked for it on `UnixStream`/`UdpSocket` yet).
- (Test-only, no version bump on its own, noted here for context:) a
  real pre-existing race in the Unix-socket parity suite was found and
  fixed in the same PR — unrelated to the timeout addition itself.

### 0.4.0

- Added the UDP datagram slice: `Net::udp_bind`, `UdpSocket`
  (`send_to`/`recv_from`/`local_addr`), completing D16's three-slice
  survey (TCP, Unix sockets, UDP) named for rusty_tail's magicsock.
- Unix-socket parity suite landed in a follow-on PR — test-only, no
  bump of its own.

### 0.3.0

- Added the Unix domain socket slice: `Net::unix_connect`/
  `unix_listen`, `UnixStream`, `UnixListener` — mode-`0600` bind and
  automatic stale-cleanup bind (a throwaway probe `connect`
  distinguishes a dead listener's leftover socket file from a live
  one). An early pass of this slice shipped with the wrong
  stale-cleanup contract (caller-must-unlink-first); caught and
  corrected before merge, so it never shipped under a version number.

### 0.2.0

- Added the TCP slice: `Net`, `TcpStream`, `TcpListener` — the first
  half of the Net surface (RFC v2 R5+, D16), named for shh, rusty_tail,
  rusty_rdp, and rusty_llama's optional server. No TLS concept in the
  trait; all four named consumers bring or inject their own wire
  crypto.

### 0.1.0 and everything before this changelog existed

Everything from the initial extraction through Track P completion:
`Fs` (capability `Dir`/`File`, byte `OsStr` boundary), `Process`
(`Command`/`Spawner`/`Child`, decoded `ExitStatus`, groups/
`kill_tree`, pipes), `Events` (deferred `SignalSource`, multiplexed
`wait_any`), the two-axis error model, the parity regime
(`platform-mock` as the third backend, the divergence registry), and
Track P (the `rusty_libc` raw-syscall floor behind the `track-p`
feature). See `docs/convergence-roadmap.md`'s Phase 1–4 entries and
`docs/extraction-map.md` for the real per-decision history — this
changelog doesn't re-derive it.

## `winargv`

### 0.1.0

Versioned independently from the PAL group starting here (previously
shared the workspace version by accident of `version.workspace = true`,
not by any real coupling — see `docs/versioning.md` §1). No functional
change in this bump; MSVCRT/cmd-rules quoting and refuse-unrepresentable
were already complete and fuzz-hardened before this changelog existed.

## `coreutils`

### 0.1.0

Versioned independently from the PAL group starting here, for the same
reason as `winargv` above — no functional change in this bump.
`coreutils` is an internal reference-consumer (RFC v2 §3); nothing
outside this repo depends on it, so its version has no audience beyond
this repo's own history.
