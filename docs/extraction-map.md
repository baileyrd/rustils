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
   leg (`tests/winargv_oracle.rs`). The §9.5 argv-echo fuzz job and the
   rush handback remain open.
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
4. **Stdio/handle model** (D5) — decide std-slot-swap vs STARTUPINFO
   lists on the record.
5. **Track P at R4** via D4.

Each step: port semantics + tests, not code links; rush's suites are the
oracle; the consumer gate still applies — coreutils (or rush adapters)
must call what lands.
