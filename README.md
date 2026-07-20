# rustils

A hand-rolled, Rust-native platform personality layer for Windows and
Linux: strongly-typed, capability-style APIs over the NT and Linux kernels,
built above raw bindings (`windows-sys`, `libc`) with all `unsafe` confined
to audited backend `sys/` modules.

**Governing document: [`docs/rfc-v2.md`](docs/rfc-v2.md).** Read it before
adding anything — in particular §3 (the consumer gate: no API without a
named, working consumer) and §5 (binding API design requirements).

## Dual mandate

- **M1 — Understanding.** This project exists partly to learn the machine.
  Hand-rolling is a feature; each hard-won lesson lands as a note in
  [`docs/learning/`](docs/learning/).
- **M2 — Foundation.** The layer must be real: consumed by working
  software, parity-tested across OSes, and stable enough to build on.
  First external consumer under contract: the **rush** shell
  (RFC §7 — mechanisms hoist here at rush's Phase 2 gate).

## Layout

```
crates/
  platform          portable traits + types (api layer; forbid(unsafe))
  platform-mock     in-memory backend — the injectable test double
  platform-linux    libc floor; ffi (curated surface) → sys (all unsafe) → impls
  platform-windows  windows-sys floor; same layering (Dir impl = R1, on Windows CI)
  winargv           MSVCRT + cmd-rules command-line quoting — standalone for handback
  coreutils         reference consumer (rcat, ls) — tested against the mock
docs/
  rfc-v2.md              the governing RFC
  architecture.md        target-state layer map + ecosystem repo placement
  convergence-roadmap.md phased migration/convergence sequencing
  behavior/              per-API behavior specs the parity suite asserts
  divergences.md         numbered cross-backend divergence registry
  learning/              M1 write-ups
```

## Verify

```
cargo test --workspace     # unit + mock + parity (mock and native backends)
cargo build && ./target/debug/rcat some-file
```

## Status

Release **R0/R1 (partial)** per the RFC roadmap: workspace, error model,
capability-fs trait surface, mock backend, Linux `Dir` over the `openat`
family, Windows `Dir` over `NtCreateFile` handle-relative opens (the
ntdll admission rationale lives in `platform-windows/src/ffi/nt_surface.rs`),
parity suite on both OS legs, std-interop on all handle types (RFC §5.1),
reference consumers (`rcat`, `rls`) wired to both native backends, CI
(fmt, clippy `-D warnings`, tests on ubuntu+windows × stable+MSRV, mingw
cross-compile pre-check, Miri on the pure crates, unsafe-scope gate,
cargo-deny). Process semantics are specced
([`docs/behavior/process.md`](docs/behavior/process.md)) with the mock as
the anchor; the native spawn/quoting/groups/reactor mechanisms are ported
from rush and its satellite crates per the extraction map
([`docs/extraction-map.md`](docs/extraction-map.md), RFC §7 Amendment A1)
— proven donors mined deliberately, not designed here from scratch.
Extraction step 1 is in: `winargv` (MSVCRT quoting + cmd-rules batch
quoting with refuse-unrepresentable — closes the BatBadBut class),
oracle-tested against `CommandLineToArgvW` on the Windows leg. Step 2 is in: native `Spawner`/`Child` on both OSes (`posix_spawn`;
`CreateProcessW` over `winargv`), decoded `ExitStatus` parity-pinned,
`rrun` as the consumer, and first-class groups — `GroupSpec::NewGroup`
with `kill_tree`/`kill_single` (`setpgid`-at-spawn; suspended-spawn into
a kill-on-close Job Object), with the registry's first divergence
entries (001/002) recorded. Step 3 and R3 are in: `Child::try_wait`, `wait_any` (portable +
multiplexed — pidfd+`poll` / `WaitForMultipleObjects` with the
64-handle cap absorbed), and the D6 `SignalSource` (deferred, one-store
handlers; console-ctrl mapping on Windows, divergence 003) — `rpar`
assembles the full §5.6 reactor from them. Step 4 is
in: `Stdio::Pipe` capture/feeding with inheritance control on every
backend (consumed by `rtee`), with the STARTUPINFO-vs-slot-swap
decision recorded in the extraction map. Linux Track P (raw syscalls
behind the `track-p` feature, via a pinned `rusty_libc` dependency,
D-12) covers every migrated family in `platform-linux/src/sys`: fdio
(read/write, the openat family, statx), the reactor's pipe2/poll, and
process control (kill, wait4, the signal trampoline) — parity-verified
in both configurations on every CI run. A full-ecosystem donor survey
(`docs/extraction-map.md` D9–D16) then unparked the **Terminal**
surface: is_tty, window size, and raw-mode enter/leave over termios
(Linux) and console modes (Windows), with `rusty_term` as the design
oracle. Convergence roadmap Phase 2 (`docs/convergence-roadmap.md`)
added slice 2 — a live `is_raw` probe, `poll_readable`/`read_chunk`,
and `set_echo` — all consumed and live-verified by `rterm`, with
bracketed paste and suspend/resume deliberately needing no further
surface (they're already expressible with what landed). PTY hosting,
resize-notification, and job-control handoff remain gated follow-ons.
Phase 3 grew the `Fs` surface (D11): `File::sync_all`,
`Dir::rename`/`rename_no_replace` (Linux `renameat2`; Windows
handle-relative `FILE_RENAME_INFORMATION` via `NtSetInformationFile`),
and a default-provided `Dir::write_atomic` composed from both —
strace-verified to fsync before it publishes. A follow-on slice added
`Dir::symlink`/`read_link` (Linux `symlinkat`/`readlinkat`; Windows
`FSCTL_SET_REPARSE_POINT`/`FSCTL_GET_REPARSE_POINT` over a hand-built
`REPARSE_DATA_BUFFER`), with the one thing Windows requires that POSIX
doesn't — declaring file-vs-directory at creation — registered as a
divergence rather than papered over (`docs/divergences.md` #004). A
further slice added `Dir::access` (`faccessat`, real not effective
uid/gid on Linux; a trial open on Windows): Windows has no
execute-permission bit on a regular file at all, so `execute` is
granted unconditionally once existence is confirmed, pinned as a second
registered divergence (`docs/divergences.md` #005) with dedicated
backend-only tests rather than a forced-uniform assertion. A final
slice rounded out `test`'s donor predicates with `Dir::unix_mode`
(`-u/-g/-k/-O/-G`, real mode bits + ownership on Linux, honest `None` —
not fabricated — on Windows) and `Dir::file_id` (`-ef`, an opaque
same-file identity every backend answers identically). The
PATH-resolution half of that donor item turned out to already exist as
`Spawner::resolve`; what's left is ecosystem-side (rush adopting it),
out of scope here.

Phase 4 (Track P completion) closed platform-linux's last two
raw-syscall gaps: `rusty_libc` gained `getdents64`/`dirents` and
`pidfd_open`/`P_PIDFD` (`rusty_libc` PR #19), and here, `read_dir`'s
`track-p` arm now calls `getdents64` directly instead of glibc's
`fdopendir`/`readdir` `DIR*` stream (which has no raw-syscall
equivalent of its own to reimplement), and `poll_pids`'s pidfd-opening
step lost its raw `c::syscall` escape hatch under `track-p` — it's a
real wrapper call now, live-verified via strace (a real `read_dir`
firing `getdents64`, a real two-child `wait_any` firing `pidfd_open`
for each pid).

Phase 5 landed the `Net` surface (D16), all three named slices. TCP
shipped `Net`, `TcpStream`, `TcpListener` for
connect/listen/accept/`set_nodelay`. Unix domain stream sockets added
`Net::unix_connect`/`unix_listen`, `UnixStream`, `UnixListener` —
mode-`0600` bind and automatic stale-socket-file cleanup (a throwaway
probe connect tells a dead listener's leftover file apart from a live
one; Linux narrows via `chmod`, a registered divergence since Windows'
`AF_UNIX` bind has no mode-bit equivalent to narrow —
`docs/divergences.md` #007). UDP datagram sockets closed the surface
out with `Net::udp_bind`/`UdpSocket` for rusty_tail's magicsock — no
listener/stream split, one connectionless socket addressed per call via
`send_to`/`recv_from`, and the one genuinely new behavior across the
whole surface: `send_to` never fails just because nothing is bound at
the destination (no handshake to fail the way TCP/Unix connect have),
strace-verified on a real closed-socket send. No TLS concept anywhere
in any slice — the four named consumers (shh, rusty_tail, rusty_rdp,
rusty_llama's optional server) all bring or inject their own wire
crypto. Linux uses raw `libc` socket calls, not track-p-gated (sockets
were never in rush's required surface, so there's nothing to route
through `rusty_libc` — `fsync`'s precedent); Windows uses raw Winsock2
with a lazily-started, deliberately never-cleaned-up `WSAStartup`
(matching mio/tokio/std's own Windows networking); the mock backend is
an in-memory implementation with real
connection-refused/addr-in-use/end-of-stream/fire-and-forget semantics.
Strace-verified on Linux throughout. See `docs/behavior/net.md`.

## License

MIT — matching the sibling crates (`rush`, `rusty_win32`, `rusty_libc`,
`rusty_lines`) so code flows both directions (extraction in, handback
out) under one license. See [`LICENSE`](LICENSE).
