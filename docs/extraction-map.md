# Extraction map: rush → rustils

Recorded 2026-07-19. Supersedes the earlier `r2-hoist-donor.md` (which
covered `rusty_win32` only) after a full review of
[`baileyrd/rush`](https://github.com/baileyrd/rush) and its satellite
crates. Companion to **RFC v2 Amendment A1** (see `rfc-v2.md` §7), which
re-grounds the R2 "hoist" on the facts below.

## Why this document exists

rush predates rustils. The RFC's §7 was written against a planning
document for an *alternate* rush that was never built; the real rush took
its own path and, in doing so, built — and battle-tested on real CI —
nearly everything rustils's roadmap planned to receive at R2/R3/R4. The
hoist is therefore not "wait for rush's Phase 2 gate"; it is an
**extraction project this repo can start at any time**, porting semantics
and tests (not linking code) from the donors below, re-floored on this
repo's tier doctrine (§2) and API standards (§5), with rush's suites as
the conformance oracle.

## Donor inventory

### D1 — `rush/src/job.rs`: Unix process groups & terminal control

A glibc-style job-control implementation with the hard invariants learned
and commented in place:

- Double-`setpgid` (parent *and* child) so terminal hand-off cannot race
  group creation.
- `tcsetpgrp` give/reclaim, sound only because SIGTTOU is ignored in the
  shell — a precondition the rustils API must encode, not assume.
- Exit-status decode through the full `WIFEXITED`/`WIFSIGNALED`/
  `WIFSTOPPED`/`WIFCONTINUED` set (this repo's B-5 sentinel, live).
- A strictly async-signal-safe `pre_exec` set (`setpgid` + `signal` only)
  for exec'd children; forked in-process stages are sound only under a
  single-threaded-at-fork invariant (see D4).

Stays rush-side (policy): job tables, `%n` specs, `$!` = last-stage pid,
`128+sig` conventions, `jobs`/`fg`/`bg`/`wait`/`disown` builtins.

### D2 — `rush/src/winjob.rs` + `rusty_win32`: Windows jobs & spawn

- Suspended-spawn → assign-to-Job-Object → resume: membership guaranteed
  before the child executes one instruction. The `GroupSpec` mechanism.
- Kill-on-close semantics with the `disown` lesson: the limit must be
  reversed *before* dropping the handle, or the process dies anyway.
- `wait_any` over `WaitForMultipleObjects` with a documented fallback at
  the 64-handle cap — the limit §5.6 requires the reactor to absorb.
- Explicit environment-block spawn (a shell tracking its own variable
  table cannot rely on inheritance).
- The std-slot swap spawn-inheritance model: mark inheritable only the
  slots this spawn touched; restore immediately (CreateProcessW snapshots
  at spawn). Alternative at extraction time: STARTUPINFO handle lists —
  decide deliberately, with rush's comments as the record of why the swap
  model works and where it is fragile.

### D3 — `winjob.rs::build_command_line`/`quote_arg`: the winargv seed

A tested reimplementation of the std library's MSVCRT quoting algorithm
(2n+1 backslashes before an embedded quote; trailing-backslash doubling;
quote-if-empty-or-whitespace). Direct seed for §5.4's `winargv` module.

**Known gap rustils must close — and hand back:** rush resolves `.BAT`/
`.CMD` via PATHEXT and quotes them with MSVCRT rules, but cmd.exe parses
batch arguments under different rules (the BatBadBut class). rush's
*foreground* path is protected by `std::process::Command`'s own guard;
its *background* path is not. rustils's contract — escape under cmd rules
or **refuse** unrepresentable arguments — is stronger than what exists.
This is the clearest case where extraction pays the donor back: build it
here, fuzz it against an argv-echo oracle on Windows CI (§9.5), and rush
adopts it.

### D4 — `rush/src/sys.rs` + `rusty_libc` + `docs/LIBC_DEPENDENCY_ANALYSIS.md`: the Track P blueprint (R4)

Track P already exists in prototype. The facade pattern (identical
surface, per-target backend selection, compile-error on no backend) is
the shape; the analysis doc is the map: ~25-syscall surface, and the
soundness landmines with resolutions —

- x86_64 `SA_RESTORER` signal-return trampoline (hand-written asm; wrong
  = crash on first delivered signal; aarch64 uses the vDSO instead).
- Kernel vs glibc `termios` layout (NCCS 19 vs 32) — silent stack
  corruption if the glibc shape is assumed.
- aarch64's removed syscalls (`fork`/`dup2`/`pipe`/`poll` →
  `clone`/`dup3`/`pipe2`/`ppoll`).
- The errno contract: raw syscalls must not write glibc's TLS errno;
  rush's `LAST_ERRNO` thread-local stash is the realized pattern.
- Fork vs malloc-lock deadlock, solved beyond the doc's minimum: memfd-
  backed here-docs remove the helper thread entirely, making a raw
  `clone(SIGCHLD)` fork sound (single-threaded at every fork point).

At R4, evaluate adopting `rusty_libc` as the Track P backend outright
versus re-deriving it; either way this material is the curriculum (M1).

> **Landed (first slice, D-12 supersedes the R4 wait):** O-2 resolved the
> evaluation early — `rusty_libc` IS the backend, as a rev-pinned git
> dependency behind platform-linux's `track-p` feature (off by default;
> rusty_libc's 1.88 MSRV sits above the workspace's 1.75 floor, so only
> the ubuntu+stable CI leg turns it on). First replaced family:
> `sys::fdio::read`/`write`, with the whole platform-linux suite re-run
> under `--features track-p` as the equivalence test and the errno-contract
> lesson written up (`docs/learning/002-…`). Remaining families migrate
> call-by-call in later slices.

### D5 — `rush/src/exec.rs` + `winstdio.rs`: stdio & fd mechanisms

- The `FdAction`/`pre_exec` fd-surgery engine: fd 3+ wiring applied in
  source order so later actions may reference earlier ones (`3>f 4>&3`).
- `winstdio`'s swap-save-restore-drop guard, with the startup-stdin
  snapshot distinguishing "fd 0 redirected" from "shell's own stdin".
- Parent-side pipe-end lifetime lessons (a lingering write end starves
  the reader of EOF — the deadlock class, documented at each site).

### D6 — `rush/src/trap.rs`: the signal-deferral core

Handler = one atomic store; consumption at safe points via `swap(0)`.
Plus the two-tier install policy (TERM/HUP always, others on trap
registration). The `events` domain's signal source starts here. Policy
that stays rush-side: `$?` preservation, re-entrancy guard, subshell
trap-visibility snapshots.

### D7 — rush test machinery: the parity regime's missing pieces

- Black-box conformance shape: `binary -c src` → assert only on
  (stdout, stderr, exit status). This is the form R2's exit criterion
  ("rush's conformance suite green on this layer") should take.
- Subprocess-per-test — sidesteps shared-fd races across test threads.
- Stdin fed from a thread — dodges the fixed-16KB pipe-buffer deadlock
  (macOS) that reads as a hung CI job, not a failed test.
- Instant-exit stand-ins (`cmd /c exit N`) + wall-clock budget as the
  assertion — race-free job-control tests.
- Fuzz only pure stages, no-panic contract; never stages that spawn or
  touch the filesystem.
- Document what is *deliberately not asserted* and why (see rush's
  Windows disown test for the exemplar).
- The PTY harness is reusable in shape; harden its fixed-sleep
  synchronization (prompt-string sync) rather than copying it.

### D8 — Ready-made divergence-registry entries

rush has already characterized, with OS-limitation citations and CI
evidence, entries `docs/divergences.md` is waiting for. When the process
domain lands here, record (with pinning tests): no `fork()` on Windows;
no `fg`/`bg`/Ctrl-Z (no `tcsetpgrp` equivalent); no fd table beyond the
three std slots; completion by polling (no SIGCHLD analog); the ambient-
Job-Object caveat that makes detach-from-job unreliable under CI runners.

### D9 — The Terminal cluster: five donors, one surface (second wave, 2026-07-19; full-ecosystem survey same day)

The strongest signal in the second-wave survey, then confirmed by the
full-ecosystem sweep: **five donors independently hand-rolled the same
terminal personality** — rusty_libc, rusty_win32, rush, then
**rusty_lines** (`src/term_sys.rs`: a three-backend facade over
rusty_libc/libc/rusty_win32 that is nearly a ready-made spec for the
trait) and **shh** (`src/tty/{unix,windows}.rs`). No surface has ever
had stronger consumer-gate evidence.

**rusty_term is the design oracle, not a backend.** It is a full
terminal *emulator* (~47k LOC) whose OS slice (`src/backend/`, ~1.4k
LOC) is already factored as a portable trait over two per-OS unsafe
files — CI-proven on both OSes. Build the rustils trait fresh and port
its *semantics + tests*; do not depend on it (tokio + edition-2024
would invert the layer stack). It converges later by swapping its
backend internals for the PAL trait. What it teaches beyond the
original sketch: raw mode on Windows is **two streams** (stdin VT-input
+ stdout VT-processing); the save/restore lifecycle IS the contract;
**resize notification is a divergence** (SIGWINCH stream vs Windows
timer-poll — no event exists); the pollable-pty-fd vs blocking-thread
asymmetry mirrors pidfd-vs-WFMO.

**Facets beyond raw-mode/winsize/isatty** (each lands when a consumer
forces it):
- *Line-editing support* (rusty_lines): bracketed-paste guard +
  envelope decode, cooked↔raw suspend/resume (`$EDITOR` handoff),
  `is_raw` re-assert + signal-free self-healing (200ms tick instead of
  SIGWINCH — a deliberate design alternative to record), VMIN=1/VTIME=0
  chunked reads, byte→Key decode (portable pure logic).
- *Console acquisition* (rusty_naner `naner-core/console.rs`): the
  GUI-subsystem attach-vs-alloc-vs-redirected personality —
  AttachConsole/AllocConsole/CONOUT$ reopen/enable-VT with a
  `ConsoleState` enum. A wholly separate facet the sketch missed.

- **Raw mode**: `rusty_libc/termios.rs` (`make_raw`, kernel-shape
  `Termios` NCCS=19, `tcgetattr`/`tcsetattr_with`, `tcflush`/`tcdrain`)
  and `rusty_win32/console.rs` (`Get/SetConsoleMode` with the
  `ENABLE_VIRTUAL_TERMINAL_INPUT`/`_PROCESSING` bits) — the two halves
  of one trait.
- **Window size**: `rusty_libc/tty.rs` (`TIOCGWINSZ`) and
  `rusty_win32/console.rs` (`GetConsoleScreenBufferInfo`, viewport not
  scrollback) — cleanly portable.
- **isatty**: `rush/sys.rs` + `builtins.rs` (`test -t`; Unix syscall vs
  Windows `IsTerminal`).
- **Job-control terminal handoff** (Unix): `rush/job.rs`
  `give_terminal`/`reclaim_terminal` over `tcsetpgrp`, sound only with
  SIGTTOU ignored — the *mechanism* half of D1 (the job tables stay
  policy); plus SIGTSTP/SIGCONT suspend-resume plumbing.
- **Test harness for free**: `rusty_win32/console.rs`
  `write_char_events`/`WriteConsoleInputW` — synthetic keystrokes, the
  Windows analog of writing into a pty master; drives a raw-mode reader
  end-to-end in CI.

Consumers per architecture.md: rusty_naner, rush interactive (Phase 5),
rusty_lines' host. Windows fg/bg absence is already characterized (D8).

### D10 — Wait-status completion: `waitid`/WNOWAIT + stopped/continued

- `rusty_libc/wait.rs`: `waitid` with `WNOWAIT` and a structured
  `Siginfo` (`P_ALL/P_PID/P_PGID`, `CLD_*`) — peek-without-reap,
  strictly richer than the adopted `wait4`; what a job table uses to
  inspect a child and still reap it later.
- `rush/sys.rs` + `job.rs`: `WUNTRACED`/`WCONTINUED` flags and
  `WIFSTOPPED`/`WIFCONTINUED` decode — the Ctrl-Z/fg/bg half of the
  status set; the landed `ExitStatus` covers exit+signal only.
  Unix-only (Windows divergence already in D8's list).

### D11 — Fs second wave: mutation layer, predicates, memfd

- `rusty_libc/fs.rs`: the directory-mutation and symlink layer —
  `renameat2` (`RENAME_NOREPLACE`/`EXCHANGE`), `symlinkat`/`readlinkat`,
  `faccessat` — only the read/stat side was adopted in Track P.
- `rush/builtins.rs`: `test`'s file-mode predicates (`-x/-u/-g/-k`,
  owner uid/gid, same-file by dev+ino) and the PATH-resolution logic
  duplicated across `command -v`/`type`/completion — a *unification*
  onto `Spawner::resolve`, not just an extraction.
- **`memfd_create`** (`rush/sys.rs` `memfd_heredoc` +
  `rusty_libc/fd.rs`): the thread-free here-doc — the load-bearing
  invariant that makes a raw `clone(SIGCHLD)` fork sound
  (single-threaded at every fork point). Cited as D4 *rationale*, never
  surfaced as an API. **Lands first if the fork/execve decision goes
  raw.**
- `rusty_libc/fd.rs`: `fcntl`/`dup` family (CLOEXEC and NONBLOCK
  toggling, `F_DUPFD_CLOEXEC`, per-fd pipe-capacity get/set),
  `pread`/`pwrite`; `rusty_win32/handle.rs` pipe/dup/inheritability as
  the Windows counterparts. Feeds D5's remaining fd-3+ engine.

### D12 — Small process/events donors (each waits for its consumer)

- Single-fd `poll_readable` (`rush/sys.rs`, zero-timeout poll — the
  `read -t 0` primitive; distinct from the multiplexed reactor).
- `umask`, rlimit/ulimit (`rush/sys.rs`+`builtins.rs`,
  `rusty_libc/rlimit.rs`/`umask.rs`) — self-contained, park until a
  builtin-shaped consumer.
- uid/gid getters (`rusty_libc/process.rs`) — brushes the gated
  Security surface.
- `rusty_win32/process.rs` `environment_block` (double-NUL UTF-16,
  order-preserving) — reusable spawn primitive.
- **Time** (`rusty_libc/time.rs`+`vdso.rs`; `rusty_win32/time.rs`): the
  two donors deliberately share a `Timespec` shape — a portable Time
  trait is pre-aligned and nearly free; park until a consumer (e.g. a
  `time` builtin) arrives.

### D13 — PTY: a Process×Terminal capability (full-ecosystem survey)

Two donors: **shh** (`connect/pty.rs`: openpty + slave handoff +
`TIOCSCTTY` session setup in pre_exec + `TIOCSWINSZ` resize + async
master with EIO→EOF) and **rusty_term** (`backend/`: openpty/fork/exec
on Unix; **ConPTY** — CreatePseudoConsole + PROC_THREAD_ATTRIBUTE — on
Windows, incl. the EOF-vs-exit teardown deadlock lesson). Deliberately
NOT part of Terminal slice 1: PTY *hosting* is its own surface, gated
on an emulator/mux consumer. Divergences to register when it lands:
ConPTY vs openpty; pollable master fd vs blocking-thread bridge.

### D14 — Tun/virtual-link surface (rusty_tail)

rusty_tail is a Tailscale-style mesh VPN (not a log follower — the
architecture doc's original placement was wrong and is corrected). Its
`ts-tun/src/sys.rs` hand-rolls /dev/net/tun + TUNSETIFF + SIOCSIF*
ioctls behind an anticipated-but-unbuilt `TunDevice` trait (its own
comments defer a wintun backend). A new gated surface with its named
consumer already in hand.

### D15 — Security surface donors (nexus, shh, rusty_rdp)

The gated Security surface now has concrete donor material:
- **nexus** `nexus-security/os_sandbox.rs`: Landlock fs confinement +
  seccomp inet-block + the helper-exec model dodging post-fork
  allocation (the D4 landmine again); `credential.rs`: OS-keyring
  vault trait with a disabled mode.
- **shh** `privsep.rs`: fork-before-runtime, socketpair monitor,
  `prctl(NO_NEW_PRIVS)`, setrlimit, credential drop with regain-root
  check.
- **rusty_rdp**: hand-rolled /dev/urandom entropy reads ×5 — a PAL
  CSPRNG/`fill_random` primitive would retire them.

### D16 — Net surface shape (shh, rusty_tail, rusty_rdp, rusty_llama)

Four consumers now define Net's shape without guessing: TCP
connect/listen + set_nodelay (shh, rdp, llama's optional server), UDP
datagram (rusty_tail magicsock), Unix sockets incl.
mode-0600-with-stale-cleanup bind (rusty_tail LocalAPI, shh agent).
**No TLS obligation**: shh and rusty_tail hand-roll wire crypto over
plain TCP; rdp's rustls is optional and injected. rusty_rdp converges
cheapest — its `net.rs` is already generic over `Read + Write`.

### Cross-cutting notes from the full-ecosystem survey

- **nexus re-derived already-landed rustils work**: its
  `job_object.rs` (kill-on-close Job Objects) and `nexus-rush/job.rs`
  (setpgid×2/tcsetpgrp/SIG_IGN) duplicate D1/D2 — the cheapest, most
  valuable convergence is swapping them for `platform::Process`, which
  exists today.
- **winargv has a second live consumer**: rusty_naner's hand-quoted
  `raw_arg` command lines (launcher.rs) are the BatBadBut class D3
  exists to kill.
- **Atomic durable write** appears twice (nexus `storage/atomic.rs`:
  temp→fsync→rename→fsync-parent with retry classes; rusty_naner's
  download→tmp→rename staged install) — a ready Fs primitive, backed
  by renameat2 (D11) / MoveFileEx.
- **AsyncFd-over-raw-fd** (nonblocking fcntl → readiness reactor →
  guarded I/O with error→EOF) appears independently in rusty_tail's
  TUN and shh's PTY — a reactor-adoption primitive for the Events
  domain.
- **rusty_llama** memory-maps its model (`loader.rs`) — the one mmap
  in the ecosystem; an Fs read-only-map capability candidate (single
  consumer today, so parked).
- **rusty_lsp is the counter-example that validates the gate**: zero
  platform crates; it converges by doing essentially nothing.
- **rusty_whisper / rusty_regx confirmed** as classified: pure compute
  (no mmap, no audio-device capture — do NOT add an audio surface on
  whisper's account) and pure library respectively.

## Not extracted (shell policy, stays in rush)

Expansion, globbing, aliases, trap registry semantics, pipefail /
`PIPESTATUS`, 127/126 mapping, `$!`/`%n` conventions, the self-re-exec
subshell protocol. rustils makes these *expressible*; it does not own
them.

## Suggested sequence

1. **`winargv`** with cmd-rules escaping and refuse-unrepresentable
   (D3) — highest security value; fuzzed per §9.5; hand back to rush.
   **Landed:** `platform-windows/src/winargv.rs` (pure `&[u16]` core,
   tested on both CI legs + Miri) with a `CommandLineToArgvW` round-trip
   oracle incl. an exhaustive adversarial-alphabet sweep on the Windows
   leg (`tests/winargv_oracle.rs`).
   **§9.5 fuzz job landed:** `fuzz/` (own workspace) fuzzes arbitrary
   argv through the builder and an *independent model* of the MSVCRT
   splitting rules (differential — builder and parser cannot share a
   bug); the Windows oracle anchors the model to the real OS, and the
   model's deterministic tests replicate the oracle table. Nightly
   schedule in `.github/workflows/fuzz.yml`. Only the rush handback
   remains from this step.
2. **Spawn + groups** behind the `Spawner` trait: Unix (D1) and Windows
   suspended-spawn/jobs (D2), with `behavior/process.md` grown to match
   and D8's divergence entries recorded.
   **First slice landed:** `LinuxSpawner` (`posix_spawn` — allocation
   entirely pre-call, no fork critical region in this crate) and
   `WindowsSpawner` (`CreateProcessW` with the command line built
   exclusively by `winargv`), consuming `wait` with decoded status
   (Signaled pinned on the Linux leg), mechanism-level `resolve`
   (PATH+execbit / PATH+PATHEXT), explicit-env and Stdio Null wiring,
   `rrun` as the gating consumer, parity tests on both legs.
   **Second slice landed:** `GroupSpec::NewGroup` + `Child::kill_tree`/
   `kill_single` — `POSIX_SPAWN_SETPGROUP` at-spawn placement on Linux;
   D2's suspended-spawn → assign-to-kill-on-close-Job → resume sequence
   on Windows — with divergence entries 001 (killed-status form) and 002
   (drop-unwaited semantics) recorded and parity-pinned. Remaining from
   D2 for later: disown-style detach (clear-kill-on-close), which waits
   for a consumer that needs it.
3. **Wait-any / reactor seed** (D2's `wait_any` + D6's signal source),
   absorbing the 64-handle limit internally per §5.6.
   **Seed landed:** `Child::try_wait` (WNOHANG / zero-timeout wait, with
   the reaped-status stash) and portable `wait_any` (poll-over-try_wait,
   10ms tick — the same coarser stand-in rush ran before adopting
   `WaitForMultipleObjects`, deliberately contract-first), consumed by
   `rpar` and parity-pinned on both legs.
   **R3 internals landed:** `Spawner::wait_any` (default = the portable
   loop) overridden natively — pidfd_open + `poll` on Linux (raw
   syscall; no libc wrapper at the MSRV baseline; `Unsupported` on
   pre-5.3 kernels falls back to the portable loop), and
   `WaitForMultipleObjects` on Windows with the 64-handle cap absorbed
   in `sys::proc::wait_many` (≤64: one true blocking wait; beyond:
   64-chunk zero-timeout sweeps on a 10ms tick). Parity sweeps 70
   children on both legs — past the Windows cap by construction.
   **D6 signal source landed, completing R3**: `platform::events::
   SignalSource` (single-slot, coalescing, take-at-safe-points — the
   donor's `PENDING_SIGNAL` shape verbatim in spirit), with
   `LinuxSignalSource` (signal(2) handler = one atomic store),
   `WindowsSignalSource` (`SetConsoleCtrlHandler` mapping console
   control events — divergence 003), and the mock. `rpar` assembles the
   §5.6 reactor from the pieces: multiplexed wait ∪ deferred signals ∪
   timeout tick, killing its children and exiting 130/143 on
   Interrupt/Terminate. Policy (trap tables, `$?` preservation,
   re-entrancy guards) stays rush-side per D6's own classification.
4. **Stdio/handle model** (D5) — decide std-slot-swap vs STARTUPINFO
   lists on the record.
   **Landed, with the decision recorded:** `Stdio::Pipe` +
   `Child::take_stdin/stdout/stderr` on all three backends, consumed by
   `rtee`. **Decision: STARTUPINFO handle lists, not rush's std-slot
   swap.** rush swapped the process-global std slots because
   `rusty_win32::spawn_suspended` exposes no per-spawn handle override;
   this backend owns its `CreateProcessW` call, so per-spawn
   `STARTF_USESTDHANDLES` avoids mutating process-global state and the
   restore ordering it forces (winstdio's swap-save-restore-drop guard).
   D5's transferable lessons are kept: only this spawn's handles are
   inheritable (fresh pipes created inheritable-child-end-only via
   `SetHandleInformation`; Inherit slots get inheritable *duplicates*
   closed after the snapshot), CLOEXEC/non-inheritance on every parent
   end so no leaked write end starves a reader of EOF, and
   `ERROR_BROKEN_PIPE`-is-EOF on pipe reads. Remaining from D5: fd-3+/
   extra-handle wiring (`FdAction`-style), which waits for a consumer.
5. **Track P at R4** via D4.

Each step: port semantics + tests, not code links; rush's suites are the
oracle; the consumer gate still applies — coreutils (or rush adapters)
must call what lands.
