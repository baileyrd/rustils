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
