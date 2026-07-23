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
`platform-mock`/`platform-macos`, sharing one number), **`winargv`**,
and **`coreutils`**.

## PAL group (`platform` / `platform-linux` / `platform-windows` / `platform-mock` / `platform-macos`)

### 0.20.0

- Added `platform_windows::{WindowsPty, WindowsPtyMaster}` (rustils#83),
  part 2/2 of the PTY surface (Phase 7, D13) — the Windows ConPTY
  backend for `platform::pty` (part 1/2, `0.19.0`). `CreatePseudoConsole`
  wired to the child at `CreateProcessW` time via
  `STARTUPINFOEXW`/`PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE` — the only way
  to attach a pseudo console at all. Not grouped (no Job Object) —
  `kill_tree` on a pty-hosted `Child` is `Unsupported` on Windows, a
  deliberate scope reduction rather than a settled design choice.
  `STARTUPINFOEXW.dwFlags` also sets `STARTF_USESTDHANDLES` (with null
  std handles): live CI testing found that without it, a *spawning*
  process whose own stdio is itself redirected — exactly `cargo test`
  under any CI runner — has the kernel duplicate its redirected handles
  into the child regardless of `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE`,
  bypassing the pseudo console entirely (a documented Windows
  console-handle-inheritance gap, per `microsoft/terminal` maintainer
  guidance in discussion #15814, not a bug in this crate's spawn
  sequence — which otherwise matches Microsoft's own ConPTY sample
  byte-for-byte). `read`/`write` are ordinary blocking `ReadFile`/
  `WriteFile` on ConPTY's two pipe handles — no background thread for
  I/O; only `Drop` does a bounded `PeekNamedPipe` drain before
  `ClosePseudoConsole`, avoiding a real deadlock (`ClosePseudoConsole`
  blocks until conhost's internal writer finishes, which can block
  against an un-drained pipe). New divergence (`docs/divergences.md`
  #011): a single pollable fd on Linux vs two non-pollable handles on
  Windows — `WindowsPtyMaster` exposes `input_handle`/`output_handle`
  rather than a single `AsHandle`/`AsRawHandle`. CI-verified only (no
  Windows execution available in the implementing session) —
  `platform-windows/tests/pty.rs`, including a dedicated test that drops
  an undrained master against a child producing far more output than a
  pipe's default buffer holds, to actually exercise the teardown fix
  rather than trust it by inspection. See `docs/behavior/pty.md` and
  `docs/design-discussion-pty.md` for the full contract and reasoning.
  **Breaking**: none — an entirely new backend for an already-landed
  trait; nothing existing changed shape. Bumps `y` per
  `docs/versioning.md` §2's "additive counts too" rule (new `pub`
  items).

### 0.19.0

- Added `platform::pty::{Pty, PtyMaster}` (rustils#82), part 1/2 of the
  PTY surface (Phase 7, D13) — built without a confirmed live consumer,
  the owner's explicit call, same posture `CredentialStore`/`Sandbox`'s
  confinement half were built under. One atomic `Pty::spawn(cmd, size)`
  opens a fresh pty pair and spawns `cmd` attached to its slave side —
  not a separate open/attach pair, since Windows's ConPTY structurally
  can't attach to an already-running process. `Ok(0)` at EOF, matching
  `File::read`/`Terminal::read_chunk`'s existing convention.
  `platform_linux::{LinuxPty, LinuxPtyMaster}`: real pty pair +
  `posix_spawn`-based attach — **not** raw `fork`+`TIOCSCTTY`
  (shh's own donor mechanism): `POSIX_SPAWN_SETSID` plus a file action
  that opens the slave by pathname reaches the identical session-
  leader-with-controlling-terminal outcome without reopening the
  async-signal-safety hazard `sys::spawn`'s `posix_spawn`-only design
  exists to close (raw `fork` stays parked behind its own separate,
  still-undecided roadmap decision). Live-verified against
  `/proc/<pid>/stat` kernel ground truth, not just a successful
  `posix_spawn` return. `LinuxPtyMaster` ships `AsFd`/`AsRawFd` on the
  concrete type (Net/Tun precedent). `platform_mock::{MockPty,
  MockPtyMaster}`: scriptable, not a real pty (mirrors `MockTun`).
  See `docs/behavior/pty.md` for the full contract,
  `docs/design-discussion-pty.md` for the design reasoning.
  **Breaking**: none — an entirely new module and new backend types;
  nothing existing changed shape. Bumps `y` per `docs/versioning.md`
  §2's "additive counts too" rule (new `pub` items).
- Windows (ConPTY) not yet landed — issue #83, part 2/2, split out from
  this release given its own real size.

### 0.18.0

- Added `platform_linux::sys::secret_service` (rustils#78) — the Secret
  Service API (`org.freedesktop.secrets`) over `sys::dbus`'s transport
  (rustils#77), part 3/3 of `CredentialStore` (Phase 6 item 2).
  `LinuxCredentialStore` now delegates to it in place of the #76 stub:
  `available()` opens a session, resolves the default collection via
  `ReadAlias`, and unlocks it if locked and unlockable
  non-interactively (no `Prompt` completion — this is a headless
  backend); `get`/`set` search/create items keyed on the
  `service`/`account` attribute pair. Stateless — a fresh D-Bus
  connection and Secret Service session per call, mirroring the
  Windows backend's fresh `CredWriteW`/`CredReadW` per call.
  Reachability failures (no session bus, no provider, no default
  collection, a collection that can't be unlocked headlessly) report
  `Unavailable` from `available()` and a real `Err` from `get`/`set` —
  never a silent `Ok(None)`/`Ok(())`, per the trait's own contract.
  Live-verified against a real `dbus-daemon --session` +
  `gnome-keyring-daemon --unlock --components=secrets` pair spawned as
  a CI test fixture (round-trip, per-account isolation, replace-on-set,
  binary payloads), the same bar #77's transport was held to — CI now
  also installs `gnome-keyring` alongside `dbus`.
  **Breaking**: none — `LinuxCredentialStore`'s trait impl signature is
  unchanged from #76; only its behavior moved from stub to real. Bumps
  `y` because `pub mod secret_service` under `sys` (itself `pub`) is a
  new public item, the same reasoning #77's `pub mod dbus` bump used.

### 0.17.0

- Added `platform_linux::sys::dbus` (rustils#77) — a hand-rolled D-Bus
  client transport, part 2/3 of `CredentialStore` (Phase 6 item 2): no
  existing D-Bus dependency, matching this repo's raw-bindings
  philosophy over the donor's `keyring-rs` wrapper. Little-endian
  message marshaling/unmarshaling for the type-system subset Secret
  Service needs (basic types, array, struct, variant, dict-entry),
  `AF_UNIX` session-bus connect (both real-path and Linux
  abstract-namespace addressing), the SASL `EXTERNAL` handshake, and
  the mandatory post-auth `Hello` registration call (missed on the
  first pass — every other call came back `AccessDenied` until this
  was added, caught by the live integration test, not a round-trip
  unit test). Internal to `platform-linux` only — no `platform::*`
  trait surface change, no `CredentialStore` behavior wired up yet
  (that's rustils#78, built on top of this).
  **Breaking**: none — `sys` is additive-only here, nothing existing
  changed shape (still bumps `y` per `docs/versioning.md` §2's
  "additive counts too" rule, since `pub mod sys` is real public
  surface even though no portable trait uses it yet). Live-verified
  against a real `dbus-daemon --session` spawned as a CI test fixture
  (new CI step: install `dbus` on the `ubuntu-latest` legs), not just
  unit tests — every wire-format alignment/padding rule is also
  asserted byte-for-byte in `wire.rs`'s own tests, not merely
  round-tripped.

### 0.16.0

- Added `platform::security::CredentialStore` (`get`/`set`/`available`)
  and `NullCredentialStore`, the Security surface's second slice (RFC
  v2 R5+, D15, Phase 6 item 2, rustils#76) — built without a confirmed
  live consumer, the owner's explicit call (same posture as `Sandbox`).
  Windows: real Credential Manager (`CredWriteW`/`CredReadW`,
  `CRED_TYPE_GENERIC`, `CRED_PERSIST_LOCAL_MACHINE`) — needed the new
  `Win32_Security_Credentials` `windows-sys` feature. `TargetName` is
  composed from both `service` and `account` (Credential Manager's
  identity key is `TargetName`+`Type` alone, not `UserName`, so two
  accounts under one service would otherwise clobber each other).
  Linux: an `Unsupported` stub for now — the real Secret Service
  implementation (`org.freedesktop.secrets` over a hand-rolled D-Bus
  client, no new dependency) is rustils#77/#78, tracked separately
  given the size. `platform-mock`: a faithful in-memory fake. No
  `delete` — not part of the roadmap's documented scope for this slice.
  **Breaking**: none — a wholly new trait, nothing existing changed
  shape (still bumps `y` per `docs/versioning.md` §2's "additive counts
  too" rule). Live-verified on Windows against real Credential Manager
  state. See `docs/behavior/security.md` for the full contract.

### 0.15.0

- Added `Spawner::adopt(pid) -> Result<Box<dyn GroupHandle>>` and a new
  `GroupHandle` trait (`kill_tree`/`kill_single` only — no `wait`/stdio,
  since an adopted pid was never spawned through this crate) — rustils#47,
  the "attach a Job Object to an externally-spawned pid" gap
  (`nexus-terminal`'s `JobObject::assign_pid`, for PTY sessions
  `portable-pty` spawns rather than this crate). Windows: `OpenProcess`
  + a fresh kill-on-close Job Object (`AssignProcessToJobObject`) — the
  same mechanism `GroupSpec::NewGroup` uses at spawn time, applied after
  the fact. Unix: always `Unsupported` (`docs/divergences.md` #010) —
  POSIX `setpgid(pid, pgid)` can only retarget the caller's own
  not-yet-exec'd child, never true by the time a caller has a pid to
  adopt, so this is a genuine one-directional OS capability gap, not
  attempted speculatively. `platform-mock`: succeeds unconditionally
  (no OS process to fail against), logging calls to the new
  `MockSpawner::adopted` field.
  **Breaking**: new required `Spawner` method — breaking for any
  external `Spawner` implementer (none outside this repo's own three
  backends exist yet). Live-verified on Windows: `kill_tree` on the
  *adopted* handle reaches the *original* spawned child, proving
  `AssignProcessToJobObject` landed on the real process. See
  `docs/behavior/process.md` and `docs/extraction-map.md`'s D2 landed
  note for the full contract.

### 0.14.0

- Added `Dir::set_unix_mode` and a new `Mode` struct
  (`setuid`/`setgid`/`sticky`/`permissions`, no `uid`/`gid` — that's
  `chown`'s job) — coreutils gap backlog #64, `unix_mode`'s write-side
  companion (`fchmodat`-equivalent). Linux: `fchmodat(dirfd, rel, mode,
  0)`, following a terminal symlink (the kernel has no symlink-mode
  concept to target, matching `chmod(1)`'s own behavior). Windows:
  `Err(Unsupported)` (`docs/divergences.md` #009) — never a silent
  no-op, since the caller's entire ask was to change permissions.
  `platform-mock`: accepts the call (`NotFound` still enforced on a
  missing entry) without persisting anything, matching `unix_mode`'s
  own fixed-default stance. Track P: also `Unsupported` for now —
  `rusty_libc` has no `chmod`/`fchmodat` primitive at the pinned rev.
  Landed ahead of a named `coreutils` consumer (no `rchmod` exists) —
  see `docs/coreutils-gap-backlog.md`'s Gap 3 resolution note.
  **Breaking**: new required trait method — breaking for any external
  `Dir` implementer (none outside this repo's own three backends
  exist yet). Live-verified on Linux against a raw `libc::stat` call.
  See `docs/behavior/fs.md` and the convergence roadmap for the full
  contract.

### 0.13.0

- Added `Metadata::nlink: u64`/`modified: SystemTime` and
  `UnixMode::permissions: u16` (coreutils gap backlog #63/#64/#65) —
  forced by this repo's own `coreutils::ls -l` reference consumer, the
  `ls -l` donor material. `nlink`/`modified` are portable (both
  backends genuinely have a link count and mtime, no `Option` needed);
  `permissions` is the standard `rwxrwxrwx` bits, read-only at the time
  — the `chmod`-equivalent write path landed separately in 0.14.0.
  **Breaking**: both are new required fields on existing public
  structs — breaking for any external construction of `Metadata`/
  `UnixMode` (none outside this repo's own three backends and
  `platform-mock` exist yet). Live-verified per backend against a
  second, independent source (Linux: raw `libc::stat`; Windows:
  `std::fs::Metadata::modified()` + a raw
  `GetFileInformationByHandleEx(FileStandardInfo, ...)` call). See
  `docs/behavior/fs.md` and the convergence roadmap's Phase 3 entry
  for the full contract and backend notes.
- Added `platform_linux::{user_name, group_name}` (`getpwuid_r`/
  `getgrgid_r`) — uid/gid → display-name resolution backing
  `coreutils::native`'s `-l` output, deliberately **not** part of
  `platform::fs`/`Dir`/`UnixMode` (a directory-service lookup, not
  filesystem metadata). Linux-only; nothing to resolve on Windows
  (`Dir::unix_mode` is always `None` there).

### 0.12.0

- Added a raw-socket + non-blocking escape hatch to `platform-windows`'s
  concrete Net socket types (`WindowsTcpStream`/`WindowsTcpListener`/
  `WindowsUnixStream`/`WindowsUnixListener`/`WindowsUdpSocket`)
  (rustils#59) — the `platform-windows` half of the gap rustils#41 left
  on Linux. Forced by `rusty_tail`'s `rusty_tokio` hand-rolled async
  runtime scoping a Windows/IOCP reactor backend (`rusty_tokio#6`), the
  same consumer #41/#48 already served on Linux/macOS. Adds
  `AsRawSocket` (raw-handle exposure only, delegating to the private
  `sysnet::OwnedSocket`), `set_nonblocking` (`ioctlsocket(FIONBIO,
  ...)`), and concrete `connect`/`bind`/`accept` constructors returning
  the concrete type directly instead of `Box<dyn Trait>` (`Net`'s own
  trait methods are now thin wrappers over these, mirroring the Linux
  slice exactly). No `AsSocket`/ownership-transfer interop — this
  crate's `OwnedSocket` is its own newtype, not std's
  `std::os::windows::io::OwnedSocket`, and nothing has asked for
  adopting an externally-created socket on Windows the way
  `From<OwnedFd>` does for Unix. See the convergence roadmap's Phase 5
  entry for the full backend notes.

### 0.11.0

- Added the Tun / virtual-link surface (D14, convergence roadmap Phase
  8): `platform::tun::{Tun, TunDevice}`, forced by rusty_tail's
  `ts-tun`, the single named consumer. `Tun::create(name, ipv4,
  prefix_len, mtu)` bundles device creation, IPv4/prefix addressing
  (which installs the connected route), MTU, and bring-up into one
  call, mirroring `ts-tun/src/sys.rs`'s own hand-rolled ioctl sequence
  exactly. Linux: `/dev/net/tun` + `TUNSETIFF`, then
  `SIOCSIFADDR`/`SIOCSIFNETMASK`/`SIOCSIFMTU`/flags-up over a throwaway
  `AF_INET`/`SOCK_DGRAM` socket — live-verified against a real kernel
  (real interface, real installed route, a real kernel-routed outbound
  packet, and a hand-crafted checksummed inbound packet delivered to a
  bound `UdpSocket`), not merely cross-compile-checked.
- The concrete `platform_linux::LinuxTunDevice` additionally exposes
  `AsFd`/`AsRawFd`/`set_nonblocking` on the concrete (non-boxed) type —
  the same raw-fd escape hatch rustils#41/#42 established for `Net`,
  since `ts-tun` needs to register the device's fd with tokio's own
  reactor directly, exactly as `ts-magicsock` did onto
  `platform_linux::LinuxUdpSocket`.
- `platform_windows::WindowsTun::create` reports `ErrorKind::Unsupported`
  explicitly rather than the module being absent — no Windows consumer
  has named itself (`ts-tun` is `#![cfg(target_os = "linux")]` only), so
  there is no donor evidence for a `wintun`-backed shape yet. No
  `platform-macos` `Tun` impl exists at all — same "no consumer, no
  speculative surface" call.
- Added `platform_mock::{MockTun, MockTunDevice}`: does not simulate
  kernel routing (unlike `MockUdpSocket`/`MockTcpStream`, there is no
  peer-socket "other side" to fake for a TUN device — the real
  counterpart is the kernel's own routing table). Scriptable instead:
  `MockTunDevice::queue_inbound` queues bytes for a future `read()`,
  and `written_packets()` returns everything recorded via `write()`.
  Does not block on an empty queue (`read()` returns `Ok(0)`
  immediately) — no real mechanism to block on, the same tradeoff
  `MockCsprng` makes for randomness quality.
- See `docs/behavior/tun.md` for the full behavior contract.

### 0.10.0

- Added `Stdio::File(Box<dyn platform::fs::File>)` (D5, rustils#51):
  wires a spawned child's stdin/stdout/stderr to an already-open `File`
  — the `> file`/`>> file`/`< file`/`2>&1`/`&> file` shell-redirect
  shapes `nexus-rush/src/exec.rs::build_stage` needs, filed as a direct
  follow-up once #43–#46 landed and converting `job.rs`'s
  `spawn_pipeline` onto `Spawner::spawn` hit this gap. Mechanism only:
  a spawn-time `dup2`/`DuplicateHandle`-style wiring that borrows rather
  than consumes the caller's `File`. `Spawner::spawn` fails
  `Unsupported` for a `Stdio::File` value from a different backend.
- Added `File::try_clone(&self) -> Result<Box<dyn File>>` (`dup(2)`/
  `DuplicateHandle`, shared open-file-description including position) —
  the `2>&1`/`&> file` half of the same redirect shape: two
  `Stdio::File` slots need to share one file's position, which two
  independent `Dir::open` calls on the same path cannot give them.
  Also added `File::as_any(&self) -> &dyn Any`, a downcast hook mirroring
  `Child::as_any_mut` that a backend's `Spawner::spawn` needs to recover
  its own concrete `File` type from a `Stdio::File`'s object-safe
  `Box<dyn File>`. Both are **new required methods on an existing
  trait** — breaking for any `File` implementor (none outside this
  repo's own three backends exist yet).
- **Breaking**: `Stdio` is no longer `Copy`/`Clone`/`PartialEq`/`Eq`,
  and `Command` is no longer `Clone` — a `Stdio::File` slot owns an
  open OS handle with no honest value-type-copy meaning. Callers that
  compared `Stdio` with `==` need `matches!` instead (the only such
  caller in this repo, `platform-mock`, was updated).
- **Breaking** (`platform-mock` only): `MockSpawner::spawned`'s element
  type changed from `Command` to a new `SpawnRecord` struct (with a new
  `StdioKind` enum for its `stdin`/`stdout`/`stderr` fields) — the
  direct consequence of `Command` losing `Clone`; existing field-name
  reads (`spawned[0].cwd`, etc.) are source-compatible.
- Per `docs/versioning.md` §2, all of the above land in one `y`-bump
  regardless of which parts are additive vs. breaking, same rule as
  every prior entry here.

### 0.9.0

- Added the job-control slice (rustils#43–#46), converging
  `platform::process`/`platform::term` onto what `nexus-rush/src/job.rs`
  needs (`baileyrd/nexus#454`): `GroupSpec::JoinGroup(pgid)` (join an
  existing process group at spawn, D1's pipeline shape); a portable
  `Signal` enum (`Term`/`Int`/`Hup`/`Quit`/`Kill`/`Stop`/`Cont`) —
  `Child::kill_tree`/`kill_single` now take a `Signal` instead of a
  hardcoded `SIGKILL`; `ExitStatus::Stopped`/`Continued` plus
  `Child::wait_job`/`try_wait_job` (D10, the `WUNTRACED`/`WCONTINUED`
  half of wait); and `platform::term::JobControlTerminal::give_terminal`
  (`tcsetpgrp`), a new Unix-only extension trait implemented only by
  `LinuxTerminal`. Breaking for existing `Child` implementers
  (`kill_tree`/`kill_single`'s signature changed, two new required
  methods) — per `docs/versioning.md` §2 this is a `y`-bump regardless
  of the additive/breaking split, same as `TcpStream::set_read_timeout`
  was. Windows gains divergence-registry entry **008** for what it
  can't do (only `Signal::Kill`; no `GroupSpec::JoinGroup`; no
  `wait_job`/`try_wait_job`). This bump was missed at merge time and is
  being recorded after the fact — no functional change since #49
  landed, just the version/changelog catching up to it.
- `platform-macos` joined the PAL group (rustils#48): a net-only
  backend (`Net`/`TcpStream`/`TcpListener`/`UnixStream`/
  `UnixListener`/`UdpSocket`, plus the rustils#41 `AsFd`/`AsRawFd`/
  `From<OwnedFd>`/`set_nonblocking`/concrete-constructor surface from
  day one), forced by `rusty_tail`'s `rusty_tokio` hand-rolling the
  same socket lifecycle a second time for its macOS/BSD kqueue reactor.
  No change to any existing crate's public API shape — a new
  implementor joining the group's existing `platform::net` traits, not
  a trait-shape change — so this entry is bookkeeping (which
  `platform` this `platform-macos` build implements), not the reason
  for this bump; see the job-control entry above for that. Not yet run
  against real macOS hardware by this workspace's own CI — validated
  via `cargo check`/`clippy --target x86_64-apple-darwin`. See
  `docs/behavior/net.md` and the convergence roadmap's Phase 5 entry
  for the full contract and backend notes.

### 0.9.0

- Job-control slice (rustils#43/#44/#45/#46), forced by `nexus-rush/src/job.rs`
  (`baileyrd/nexus`, converging onto `platform::process`/`platform::term` per
  `baileyrd/nexus#454`):
  - `GroupSpec::JoinGroup(pgid)` — a pipeline stage joins an existing process
    group at spawn (race-free, same as `NewGroup`) instead of leading a fresh
    one. Unix only; `Unsupported` on Windows.
  - `Child::kill_tree`/`kill_single` now take a `Signal` parameter (**breaking**
    — previously no argument, always `SIGKILL`) — a new portable `Signal` enum
    (`Term`/`Int`/`Hup`/`Quit`/`Kill`/`Stop`/`Cont`). Windows accepts only
    `Signal::Kill`; every other variant is `Unsupported` there.
  - `ExitStatus::Stopped(sig)`/`Continued` plus `Child::wait_job`/`try_wait_job`
    (`WUNTRACED`/`WCONTINUED`) — the Ctrl-Z/`fg`/`bg` half of wait. Unix only;
    `Unsupported` on Windows.
  - `platform::term::JobControlTerminal` — a new, separate trait (not folded
    into `Terminal`) providing `give_terminal(pgid)` (`tcsetpgrp`), encoding
    the `SIGTTOU`-ignored precondition into every call. Implemented only by
    `LinuxTerminal` — no Windows equivalent exists to implement it, which is
    exactly why it's its own trait rather than a `Terminal` method every
    backend would have to answer for.
  - New divergence-registry entry 008 records the Windows-side gaps (no
    general signal delivery, no numeric-pgid group join).
  - rustils#47 (Windows: adopt an externally-spawned pid into a Job Object)
    deliberately did **not** get an API here — no forcing consumer yet
    (`JobObject::assign_pid` is dead code in `nexus`) — left open as a
    recorded gap per RFC v2 §3's consumer gate.

### 0.8.0

- Added a raw-fd + non-blocking escape hatch to `platform-linux`'s
  concrete Net socket types (`LinuxTcpStream`/`LinuxTcpListener`/
  `LinuxUnixStream`/`LinuxUnixListener`/`LinuxUdpSocket`): `AsFd`,
  `AsRawFd`, `From<OwnedFd>`, and `set_nonblocking` — plus concrete
  `connect`/`bind`/`accept` constructors returning the concrete type
  directly instead of `Box<dyn Trait>` (`Net`'s own trait methods are
  now thin wrappers over these). Forced by rustils#41: rusty_tail's
  `rusty_tokio` hand-rolled async runtime wants to register a socket
  with its own reactor rather than reimplement socket setup from
  scratch. Inherent-impl-only — the object-safe `platform::net` traits
  are unchanged, matching `LinuxFile`/`LinuxDir`'s existing std-interop
  precedent (`fs.rs`). Linux only; not part of the cross-backend
  `docs/behavior/net.md` spec.

### 0.7.0

- Added the Security surface's third slice: `platform::security::Sandbox`
  (`confine_filesystem` via raw Landlock syscalls, `block_inet_sockets`
  via a hand-written seccomp-BPF filter), mirroring nexus's
  `os_sandbox.rs` shape exactly. Built without a confirmed live
  consumer, an explicit owner call made after an RFC-level design
  discussion (`docs/design-discussion-sandbox.md`) found nexus's and
  shh's donor material solve two different problems — process
  confinement vs. privilege-separation isolation — that don't share a
  trait shape; only the confinement half landed. `CredentialStore`
  (the middle slice) stayed held on the same trip: nexus's existing
  `CredentialVault` has no live gap to converge on. `x86_64`/Linux
  only for now; every other backend reports `SandboxStatus::Unsupported`
  rather than silently claiming enforcement.

### 0.6.0

- Added the Security surface's first slice: `platform::security::Csprng`,
  `fill_random`, forced by rusty_rdp's five hand-rolled `/dev/urandom`
  reads (`src/krb5/kdc.rs`). Deliberately narrow — one method, no key
  derivation, no algorithm choice. Linux draws from the raw
  `getrandom(2)` syscall, Windows from `BCryptGenRandom` with the system
  preferred RNG — neither opens `/dev/urandom` as a file, avoiding an
  `fd` a later filesystem sandbox policy (this same Phase 6's largest
  remaining slice) might otherwise deny.

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
