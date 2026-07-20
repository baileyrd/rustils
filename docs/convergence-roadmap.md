# Convergence Roadmap

**Status:** Living document, opened 2026-07-19 against `docs/architecture.md`
(RFC v2 Amendment A3) and the D1–D16 donor inventory in
`docs/extraction-map.md`. This is *execution sequencing*, not a new
decision — it does not amend the RFC. Update it the way the extraction
map records landed-notes: mark items done in place, don't delete the
history.

Two kinds of work live here, and they land in different repos:

- **Surface work** — new or extended `platform::*` traits. Lands in
  **this repo**, follows the standing PR workflow, gated by §3 (a named
  consumer must exist before work starts).
- **Convergence work** — a parallel tool swapping its hand-rolled OS
  calls for an existing PAL trait. Lands in **the tool's own repo**.
  Each one gets called out with a "lands in" line; none of these are
  authorized to start just by appearing here — each needs its own
  go-ahead when its phase comes up.

Ordering principle: cheapest-and-already-forced first, design decisions
called out rather than defaulted, nothing built before its consumer is
named. No calendar dates, per RFC §8's discipline — phases are
dependency order, not a schedule.

## Phase map

```
Phase 1  Free convergences (no new surface)         ← start here
Phase 2  Terminal slice 2 (bracketed paste, etc.)
Phase 3  Fs second wave (D11: renameat2, atomic write, memfd)
Phase 4  Track P completion (getdents64, pidfd_open upstream)
Phase 5  Net surface (D16)
Phase 6  Security surface (D15)
Phase 7  PTY surface (D13) — blocked on a named consumer
Phase 8  Tun surface (D14)
Phase 9  Windowing + Registry/Config (nexus-only, converge last)
─────────────────────────────────────────────────────────────
parked   fork/execve vs posix_spawn — owner design decision
```

Phases are not strictly sequential gates — 3 and 4 can interleave with
2, and nothing stops Phase 5 starting before Phase 2 finishes. The
number is priority, not a blocker on later phases.

---

## Phase 1 — Free convergences (no new PAL surface needed)

The highest-value, lowest-cost work available: tools that already have
everything they need sitting in `platform` today. Zero rustils-side
work; each is a self-contained PR in the tool's own repo.

### 1a. nexus → `platform::process` (Process, already built)

**Lands in nexus.** `nexus-terminal/src/job_object.rs` (Windows Job
Objects: CreateJobObjectW/kill-on-close) and `nexus-rush/src/job.rs`
(Unix setpgid×2/tcsetpgrp/SIG_IGN) independently re-derive D1/D2 —
mechanisms already landed here as `GroupSpec::NewGroup` +
`Child::kill_tree`. Swap both for `platform::process::{Spawner,
GroupSpec, Child}`. No new surface, no design question — pure adoption.
Proves Process holds up under a demanding, unrelated consumer.

### 1b. rusty_lines → `platform::term` (Terminal, partial — slice 1 only)

**Lands in rusty_lines.** `src/term_sys.rs`'s three-backend facade
(libc/rusty_libc/rusty_win32) can swap its `is_tty`/`get_attrs`
(window_size)/raw-mode calls for `platform::term::Terminal` today. Its
`poll_readable`/`read_chunk`/echo-off/bracketed-paste/suspend-resume
calls cannot — they need Phase 2. **First slice is worth doing now
anyway**: it's the fastest real-world exercise of the Terminal trait,
and it turns rusty_lines into the concrete forcing consumer for Phase
2's remaining facets instead of a hypothetical one.

### 1c. winargv handback — prerequisite work, not yet a convergence

**Landed here 2026-07-19.** `winargv` is now its own workspace crate
(`crates/winargv`, depending only on `platform` for error types) rather
than a module inside `platform-windows`. `platform-windows` re-exports
it (`pub use winargv;`) so nothing about its own internal use changed —
`process.rs`'s `crate::winargv::build_command_line` and the existing
`tests/winargv_oracle.rs`/fuzz target still resolve unchanged. What
changed: a handback consumer (rusty_naner's `raw_arg`-quoted command
lines, or rush's own winjob.rs) can now depend on `winargv` alone —
zero windows-sys, zero Dir/Spawner/console surface — instead of pulling
in all of `platform-windows` for one quoting module. The actual
handback PRs (rusty_naner, rush) are still open, tracked as their own
convergence work, not implied by this landing.

---

## Phase 2 — Terminal slice 2 (D9, remaining facets)

**Landed 2026-07-19.** Forcing consumer: rusty_lines (via 1b above).
Added to `platform::term::Terminal`:

- `is_raw()` — a **live** OS-state probe (not a cached flag), so a
  consumer can notice drift from something outside its own
  `enter_raw`/`leave_raw` calls.
- `poll_readable(timeout)` / `read_chunk` — VMIN=1/VTIME=0-style
  batched reads, distinct from the multiplexed `wait_any` reactor.
  Live-verified: `rterm --raw-probe` under a real pty polls then reads
  a batched chunk.
- `set_echo(bool) -> Result<bool>` — echo toggle independent of full
  raw mode (password prompts), returning the previous state so a
  caller restores exactly.

**Scoped out on purpose, not deferred** — the other two facets in the
original plan turned out to need no new surface at all:
bracketed paste is protocol bytes over the stream `read_chunk` already
exposes (no OS call, so it stays consumer-expressible, not
PAL-owned); cooked↔raw suspend/resume is exactly a second
`leave_raw()`/`enter_raw()` pair — the existing save/restore contract
already produces the right outcome. `docs/behavior/term.md` records
this as a deliberate scoping decision, not an oversight.

Still excluded (real D9 material, no forcing consumer yet): Unix
job-control terminal handoff (`tcsetpgrp` give/reclaim, SIGTSTP/
SIGCONT — waits on rush interactive or another job-control consumer)
and rusty_naner's console-*acquisition* facet (attach/alloc/redirect
for GUI-subsystem processes — waits on the rusty_naner convergence
being scheduled).

---

## Phase 3 — Fs second wave (D11)

**Landed (first slice) 2026-07-19.** Self-contained, no design
decisions. Added to `platform::fs`:

- `File::sync_all()` — durability (`fsync`/`FlushFileBuffers`),
  finally giving `flush`'s long-standing doc comment its distinct
  explicit companion.
- `Dir::rename`/`rename_no_replace` — same-directory rename, replace
  vs. atomic-refuse-if-exists. Linux: `renameat2` (no libc wrapper at
  this MSRV on glibc x86_64, same situation `pidfd_open` was in — the
  raw-syscall arm; rusty_libc *does* have `renameat2`, so the track-p
  arm is an ordinary split, not another escape hatch). Windows:
  `FILE_RENAME_INFORMATION` via `NtSetInformationFile` with
  `RootDirectory` set to the capability's own handle — the
  handle-relative rename this backend's ambient-path-free model needs.
  Not the Win32 `SetFileInformationByHandle`, the first thing tried: a
  live windows-latest CI run proved that wrapper rejects a non-null
  `RootDirectory` for the classic `FileRenameInfo` class with
  `ERROR_INVALID_PARAMETER` — handle-relative rename turns out to be a
  Win32-layer restriction, not an NT one (second ntdll admission,
  `ffi::nt_surface`).
- `Dir::write_atomic` (default-provided, composes `open`+`write`+
  `sync_all`+`rename` — one implementation for every backend) — the
  headline deliverable, forced by two independent donors (nexus
  `storage/atomic.rs`, rusty_naner's staged install). Strace-verified
  on Linux: `fsync` fires strictly before the publishing `renameat2`.

**Landed (symlink slice) 2026-07-19.** The item this phase originally
deferred. Added to `platform::fs`:

- `Dir::symlink`/`read_link` (`symlinkat`/`readlinkat`) — the target is
  stored verbatim, not validated or resolved; `read_link` round-trips
  the exact bytes. Linux: ordinary `symlinkat`/`readlinkat` libc
  wrappers (unlike `renameat2`, no raw-syscall escape hatch needed).
  Windows: `FSCTL_SET_REPARSE_POINT`/`FSCTL_GET_REPARSE_POINT` over a
  hand-built `REPARSE_DATA_BUFFER` (third ntdll-adjacent admission,
  `ffi::nt_surface` — a struct, not a function this time), using the
  same `addr_of!`-derived-offset technique `rename`'s
  `FILE_RENAME_INFORMATION` construction established. The one thing
  Windows requires that POSIX doesn't — declaring file-vs-directory at
  creation — is a registered divergence (`docs/divergences.md` #004),
  not papered over.

**Landed (faccessat slice) 2026-07-19.** The design pass this phase's
own predecessor said it needed. Added to `platform::fs`:

- `Dir::access` (`faccessat(2)`) — probes `read`/`write`/`execute`
  (`AccessMode`), `Err(PermissionDenied)` if any requested bit is
  refused; an empty mode is a vacuous yes, existence being `metadata`'s
  job. Linux: the plain `faccessat` wrapper with real, not effective,
  uid/gid — deliberately not glibc's `AT_EACCESS` emulation, since
  Track P's `rusty_libc::fs::faccessat` has no flags parameter at all
  and this keeps both configurations answering the identical question.
  Windows: a trial open with the matching access mask, immediately
  closed — the actual operation the probe predicts. The cross-platform
  permission-predicate question the design pass needed to answer:
  Windows has no execute-permission bit on a regular file at all, so
  `execute` is granted unconditionally once existence is confirmed — a
  registered divergence (`docs/divergences.md` #005), pinned by
  dedicated backend-only tests (the two backends' correct answers are
  opposites for the identical setup) rather than a shared assertion.

**Landed (test-predicates slice) 2026-07-19.** `test`'s `-u/-g/-k/-O/-G`
(mode bits, ownership) and `-ef` (same-file identity) — `-x` was already
`Dir::access(rel, AccessMode::execute())`, no new surface needed. Added
to `platform::fs`:

- `Dir::unix_mode` — `setuid`/`setgid`/`sticky` and owning `uid`/`gid`
  where the OS has the concept (real `fstatat` data on Linux); `Ok(None)`
  — not a fabricated zeroed-out `Some` — where it doesn't (Windows has
  no POSIX mode-bit/uid-gid model at all, a registered divergence,
  `docs/divergences.md` #006).
- `Dir::file_id` — an opaque, equality-only per-OS file identity (POSIX
  `(dev, ino)`; Windows `(volume serial, file index)` via
  `GetFileInformationByHandle`) both backends answer in the same
  contract, unlike `unix_mode`: this one has no Windows gap.

**PATH-resolution unification, corrected scope**: this donor item
turned out to already be done — `Spawner::resolve` (mechanism-level
PATH+exec-bit on Linux, PATH+PATHEXT on Windows) has existed in
`platform::process` since the process-surface work landed, and
`WindowsSpawner::spawn` already calls it internally. What remains is
entirely ecosystem-side: rush swapping its own three duplicated
PATH-walking implementations (`command -v`/`type`/completion) over to
call this already-existing API — out of scope here without `rush`'s
actual code in hand to unify against.

## Phase 4 — Track P completion

**Landed 2026-07-19, both parts.**

- **rusty_libc** ([PR #19](https://github.com/baileyrd/rusty_libc/pull/19)):
  `fs::getdents64`/`dirents` (a caller-owned-buffer syscall wrapper plus a
  zero-allocation parsing iterator over the kernel's own `d_reclen`
  chain) and `process::pidfd_open` + `wait::P_PIDFD` (the `waitid`
  pairing needed to actually use it), verified end to end there by a
  real fork + `pidfd_open` + `poll` + `waitid(P_PIDFD, ...)` test.
- **Here**: the rev pin bumped to that merge; `platform-linux::sys::fdio::read_dir`
  now has a `track-p` arm calling `getdents64`/`dirents` directly
  (`fdopendir`/`readdir`'s `DIR*` stream is glibc userspace buffering
  with no raw-syscall equivalent of its own — bypassed entirely rather
  than reimplemented), and `sys::spawn::poll_pids`'s pidfd-opening step
  is now a track-p-split `pidfd_open` helper — the raw `c::syscall`
  escape hatch is gone under `track-p` (still there, unavoidably, in the
  non-track-p arm: no libc wrapper for this syscall exists at this
  workspace's MSRV). Live-verified via strace under `--features
  track-p`: a real `read_dir` fires `getdents64` and correctly
  classifies file-vs-directory entries; a real two-child `wait_any`
  fires `pidfd_open` for each pid and picks the first to finish.

This closes platform-linux's raw-syscall coverage — no remaining gaps
between what `track-p` claims to cover and what it actually routes
through rusty_libc.

## Phase 5 — Net surface (D16)

**Lands here** — the biggest surface by consumer count: shh, rusty_tail,
rusty_rdp, and rusty_llama's optional server all want it, and none of
them need TLS in the trait (all four bring their own wire crypto or
inject TLS separately). Shape: TCP connect/listen + `set_nodelay`, UDP
datagram, Unix sockets (incl. mode + stale-cleanup bind).

Cheapest real convergence to prove it: **rusty_rdp**, whose `net.rs` is
already generic over `Read + Write` — supplying the PAL's stream type
as `S` is close to a no-op. Do that convergence PR immediately after
the surface lands, before shh/rusty_tail (which also need Phase 6/7/8
pieces and would otherwise block the "does this trait work" signal).

**Landed (TCP slice) 2026-07-19.** Scoped to TCP only this slice — UDP
datagram and Unix sockets (mode + stale-cleanup bind) deferred, the same
phased-slicing judgment call the Fs surface made for symlinks/`access`.
Added:

- `platform::net::{Net, TcpStream, TcpListener}` — a stateless
  capability-factory trait, the same shape as `Spawner`/`Dir`.
  `TcpStream`/`TcpListener` are `Send` (unlike `Dir`/`Child`): the
  accept-then-hand-off-to-a-worker-thread pattern is this surface's
  entire reason for existing, caught as a real compile error by a live
  scratch test before the bound was added, not a hypothetical.
- Linux: raw `libc` socket calls (`socket`/`bind`/`listen`/`accept4`/
  `connect`/`getsockname`/`getpeername`/`setsockopt`). Not track-p-gated
  at all — one implementation for both configurations, `fsync`'s
  precedent: sockets were never in rush's required surface per
  rusty_libc's own `DESIGN.md`, so there's nothing to route through it
  here. Strace-verified: a real loopback round trip fires
  `socket`→`setsockopt(SO_REUSEADDR)`→`bind`→`listen`→`getsockname` on
  the listen side and `socket`→`connect`→`setsockopt(TCP_NODELAY)` on
  the client side, with `accept4` on the accepting thread.
- Windows: raw Winsock2 (`WSAStartup`/`socket`/`bind`/`listen`/`accept`/
  `connect`/`getsockname`/`getpeername`/`setsockopt`/`recv`/`send`).
  `WSAStartup` is called lazily, exactly once, via `std::sync::Once`,
  deliberately with no matching `WSACleanup` — the OS tears down Winsock
  state at process exit regardless, and racing `WSACleanup` against
  in-flight sockets on other threads at shutdown is a real hazard
  "never clean up" avoids (mio/tokio/std's own Windows networking make
  the same call). Cross-compile-checked only — no windows-latest CI run
  has exercised this path yet, unlike every other backend piece landed
  so far.
- Mock: an in-memory duplex-channel implementation, the same "real
  behavior, no OS calls" contract `MockDir` has for the filesystem —
  a process-global registry of listening addresses plus a per-connection
  `mpsc` channel pair, with real connection-refused/addr-in-use/
  end-of-stream semantics.

**Landed (Unix sockets slice) 2026-07-20.** The deferred half of the TCP
slice — `platform::net::{UnixStream, UnixListener}` plus
`Net::unix_connect`/`unix_listen`, mirroring `TcpStream`/`TcpListener`'s
shape minus `set_nodelay` (no Nagle buffering on `AF_UNIX` to toggle) and
with `PathBuf` addresses standing in for `SocketAddr` — including a
third, TCP-never-has legal outcome an `AF_UNIX` peer can report:
`Ok(None)` from `peer_addr`/`local_addr` for an unnamed (anonymous)
endpoint. Added:

- Linux: `socket(AF_UNIX, SOCK_STREAM|SOCK_CLOEXEC)` +
  `bind`/`connect`/`accept`/`getsockname`/`getpeername` over a hand-packed
  `sockaddr_un` (the trailing `sun_path` array has no portable fixed
  offset the way `sockaddr_in`'s `sin_port` does, so the offset is
  measured once via `addr_of!`, the same technique `rename`'s
  `FILE_RENAME_INFORMATION` construction established). `unix_listen`
  narrows the freshly bound socket file to `0600` with a `chmod` right
  after `bind` — the mode-0600 half of D16's agreed shape (rusty_tail's
  LocalAPI, shh's agent socket), since a bare `bind` otherwise leaves the
  file at whatever the process umask allows.
- Windows: the same Winsock plumbing the TCP slice already pays for
  (`WSAStartup` once, `SOCKADDR_UN`/`afunix.h`'s 108-byte layout) —
  `socket(AF_UNIX, SOCK_STREAM)` + `bind`/`connect`/`accept`. No
  mode-narrowing step: Winsock's `AF_UNIX` bind has no POSIX-chmod
  equivalent, so the bound file is left at the filesystem's own ACL
  defaults instead of forcing an owner-only mode nothing in Windows'
  model enforces the same way.
- Both backends: real stale-cleanup bind, the other half of D16's agreed
  shape. A `bind` onto a path a live listener already holds and a path
  left behind by a listener that died without unlinking it hit the
  identical `AddrInUse` wall — the kernel/Winsock can't tell the two
  cases apart at bind time — so `unix_listen` resolves the ambiguity
  itself with a throwaway probe connect: `ECONNREFUSED`/`WSAECONNREFUSED`
  means stale (unlink and retry the bind exactly once), a successful
  probe means live (left untouched, `AddrInUse` surfaces normally). No
  caller-side unlinking needed. (An earlier pass of this slice shipped
  without this — caller-must-unlink-first — and got corrected before
  merge once review caught that it silently dropped the exact behavior
  D16 named this slice for.)
- Mock: `MockUnixListener`/`MockUnixStream` extend the registry-plus-
  channel-pair pattern `MockNet`'s TCP side already established, with the
  same real connection-refused/addr-in-use semantics; a listener's `Drop`
  frees its path in the registry so a later `unix_listen` on the same
  path succeeds — mirroring "dropping the first listener frees the
  address for reuse" from the TCP side.

**Landed (Unix-socket parity suite) 2026-07-20.** The one follow-up
this slice's own note above flagged as open: `net_parity.rs`
(kept textually identical across both crates) gained
`assert_unix_behavior` — connect/accept, the unnamed-peer case a plain
`unix_connect` client always hits, refusal once the listener drops,
and stale-cleanup bind reclaiming the leftover path afterward,
strace-verified live on Linux end to end (the real `bind` →
`EADDRINUSE` → probe `connect` → `ECONNREFUSED` → `unlinkat` → `bind`
sequence, not just the unit-level mock/real-backend tests each already
had). `docs/behavior/net.md`'s spec itself already covered Unix
sockets in full before this — only the shared cross-backend assertion
was the gap.

**Landed (UDP datagram slice) 2026-07-20.** The third and final D16
slice — `platform::net::UdpSocket` plus `Net::udp_bind`, named for
rusty_tail's magicsock transport. No listener/stream split unlike
TCP/Unix: one connectionless socket both sends and receives, addressed
per call via `send_to`/`recv_from` rather than a fixed peer from
`connect`/`accept` — and no `set_nodelay` (Nagle's algorithm has
nothing to do with a connectionless protocol). Designed and
implemented directly rather than through another workflow fan-out,
given what the Unix sockets slice's trait-design delegation cost last
time. Added:

- Linux: `socket(AF_INET/AF_INET6, SOCK_DGRAM|SOCK_CLOEXEC)` +
  `bind`/`sendto`/`recvfrom`, reusing the TCP slice's `to_sockaddr`/
  `from_sockaddr`/`local_addr` helpers as-is (`getsockname` and
  sockaddr packing don't care about socket type).
- Windows: the same Winsock plumbing (`WSAStartup` once) — `socket(...,
  SOCK_DGRAM, 0)` + `bind`/`sendto`/`recvfrom`, same helper reuse.
- Mock: a third process-global registry (independent from the TCP and
  Unix ones — real OSes keep `SOCK_STREAM`/`SOCK_DGRAM`/`AF_UNIX` bind
  spaces separate too) keyed by bound address, holding each socket's
  `Sender` half so other sockets' `send_to` can reach it; `recv_from`
  reads from the receiver half. `send_to` to an unbound address is a
  genuine no-op — dropped, not an error — the in-memory analog of a
  real fire-and-forget `sendto`.
- **The one genuinely new behavior across the whole Net surface**:
  `send_to` never fails because nothing is bound at the destination —
  there is no handshake to fail the way `tcp_connect`/`unix_connect`
  have. Strace-verified on Linux: a real `sendto` to a since-closed
  socket's old port returns success (the full byte count), not an
  error.
- `docs/behavior/net.md` and both `net_parity.rs` files (kept textually
  identical) get UDP's own assertion function, separate from TCP's —
  the same judgment call `assert_fs_behavior` made once for
  symlinks/access, made here because UDP's behavior barely overlaps
  with TCP's at all.

This closes out D16's original four-consumer survey — TCP, Unix
sockets, and UDP datagram all landed, and with the Unix-socket parity
suite landed too (see the note above), Net's own parity coverage is
complete across all three slices. Nothing remains open within this
domain.

## Phase 6 — Security surface (D15)

**Lands here**, staged narrow-to-wide:

1. `fill_random` / CSPRNG — trivial, self-contained. Retires
   rusty_rdp's five hand-rolled `/dev/urandom` reads. Do this first;
   it's a same-day PR with an immediate convergence.
2. `CredentialStore` (get/set/available, disabled-mode escape hatch) —
   modeled on nexus's `keyring`-backed vault.
3. Sandbox policy (Landlock + seccomp on Linux, `Unsupported` stubs
   elsewhere initially) — modeled on nexus's `os_sandbox.rs` and shh's
   `privsep.rs` (fork-before-runtime, `NO_NEW_PRIVS`, rlimit,
   credential-drop-with-regain-check). The largest, most
   design-sensitive piece in this phase; do it last and expect an
   RFC-level discussion of the per-OS confinement story (macOS
   seatbelt and Windows restricted tokens have no donor yet).

## Phase 7 — PTY surface (D13)

**Blocked on a named consumer** — this is the one item on this roadmap
that isn't ready to schedule. shh and rusty_term both have working
donor code (openpty+TIOCSCTTY vs ConPTY), but neither is *itself* the
forcing consumer in the §3 sense: shh doesn't need to give up its own
pty.rs to unblock anything, and rusty_term's PTY hosting is scoped
deliberately out of its converged Terminal backend (see D9's oracle
note). Real candidates: a job-control-capable rush-interactive
(Phase 5 rush gate), or rusty_naner hosting rusty_term through a PAL
PTY instead of a raw subprocess launch. **Action: raise with the owner
before starting** — don't build speculatively.

## Phase 8 — Tun / virtual-link surface (D14)

**Lands here**, single named consumer (rusty_tail) — sufficient per the
same single-consumer precedent as rush itself. Linux `TUNSETIFF`
ioctls behind the `TunDevice` trait rusty_tail's own code already
anticipates (`ts-tun/src/lib.rs` comments defer this exact split);
Windows via wintun, `Unsupported` until a Windows consumer names itself.
Lower priority than Phases 1–6 only because it has one consumer instead
of several — not because the donor evidence is weak.

## Phase 9 — Windowing + Registry/Config (nexus-only)

**Lands here** (trait) **+ nexus** (convergence), last and thinnest per
architecture.md's own assessment:

- Registry/Config first — nexus's `persistence.rs` (file-backed JSON +
  `dirs` paths) is a simple personality to formalize.
- Windowing last — nexus's window management is Tauri-mediated, not
  raw OS calls, so the PAL trait would be a thin pass-through with low
  near-term value. rusty_rdp's "display" is wire-encoding, not OS
  windowing, and does not force this surface (architecture.md
  correction, D-survey).

---

## Parked: fork/execve vs posix_spawn

Independent of every phase above — a design decision, not a sequencing
question. The architecture diagram the owner confirmed shows raw
`fork`/`execve` on Layer 1 Linux; current code uses `posix_spawn`
(outsources async-signal-safety to glibc). If resolved toward raw:
`memfd_create` (D11) is the prerequisite — it's the thread-free
here-doc mechanism that makes a raw `clone(SIGCHLD)` fork sound
(single-threaded at every fork point), the exact invariant `posix_spawn`
exists to avoid needing. Do not start this without the owner's explicit
go-ahead; recorded in `docs/architecture.md`'s "Open item" section.

---

## How to use this document

Pick a phase (or an item inside one), get the go-ahead, do the work
through the standing PR workflow, then mark it done here with a
landed-note (mirror `extraction-map.md`'s style: date, PR/commit,
one-line summary) rather than deleting the entry. Convergence items
that land in other repos should still get a landed-note here, so this
stays the one place that shows the whole ecosystem's status at a
glance.
