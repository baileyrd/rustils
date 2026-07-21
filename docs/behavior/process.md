# Behavior Spec — process (Command / Spawner / Child)

The semantics the parity suite asserts for every backend implementing
`Spawner`/`Child`. Backends: `platform-mock` (scripted; unit tests),
`platform-linux` (`posix_spawn`), and `platform-windows`
(`CreateProcessW` over `winargv`) — the native pair extracted per
`../extraction-map.md` step 2 (first slice: spawn/wait/resolve; groups,
kill-tree, pipes, and wait-any follow). The parity tests live in each
backend crate's `tests/parity.rs` with OS-specific fixtures and mirrored
assertions.

## Specified

- `Command.argv` is a list of discrete arguments end to end. Any joining
  or quoting an OS requires is backend-internal and never caller-visible;
  a backend that cannot represent an argument list faithfully must refuse
  to spawn, not approximate (the BatBadBut class — RFC v2 §5.4).
- `cwd` is always explicit. There is no inherit-ambient-cwd variant;
  consumers own cwd policy.
- `EnvSpec::Inherit` passes the parent environment unchanged;
  `EnvSpec::Explicit` starts empty — nothing leaks from the parent.
- `ExitStatus` is decoded uniformly: `Code(n)` for a normal exit,
  `Signaled(n)` for signal termination. A raw `waitpid` status word never
  crosses the API boundary (pins scaffold bug B-5 — the parity suite's
  permanent sentinel). `Signaled` is never produced on Windows.
- `ExitStatus::success()` is true for `Code(0)` only.
- `Child::wait` consumes the child: double-wait — and with it the
  wait-after-close bug class (B-4) — is unrepresentable.
- `Spawner::resolve` applies mechanism-level lookup only (PATH + exec bit
  on unix; PATH + PATHEXT on Windows). Shell policy — builtin precedence,
  shebang emulation — lives in consumers.
- `resolve` of an unknown program fails `NotFound` with the program as
  path context.

- `GroupSpec::NewGroup` places the child in a fresh group *before its
  first instruction executes* (`POSIX_SPAWN_SETPGROUP`; suspended-spawn →
  Job-Object assign → resume) — there is no window in which the child or
  a fast-spawned grandchild escapes the group.
- `GroupSpec::JoinGroup(pgid)` places the child straight into an
  *existing* group, the same race-free way — D1's pipeline shape, where
  stage 2..n join the first stage's pgid instead of each leading its
  own. Unix only: Windows has no numeric process-group id a spawn can
  join (Job Objects are handle-based), so `Spawner::spawn` fails
  `Unsupported` for `JoinGroup` on that backend — divergence **008**.
- `Child::kill_tree` requires `NewGroup` or `JoinGroup` and fails
  `Unsupported` otherwise (the only alternative target is the parent's
  own group, which is never what the caller meant). `kill_single`
  always works (subject to the `Signal` restriction below).
- `Child::kill_tree`/`kill_single` take a portable `Signal`
  (`Term`/`Int`/`Hup`/`Quit`/`Kill`/`Stop`/`Cont`) — D1's `kill`/`fg`/
  `bg` builtins need more than a hardcoded SIGKILL on an already-running
  child. `Signal::Kill` is the only identity guaranteed on every
  backend; Windows fails every other `Signal` with `Unsupported` — there
  is no OS mechanism to deliver an arbitrary signal to a process here —
  per divergence **008**.
- A killed child must still be `wait`ed; the status it reports is
  OS-divergent — `Signaled(9)` vs `Code(1)` — per divergence **001**.
- Dropping an un-waited `NewGroup`/`JoinGroup` child: tree terminates on
  Windows (kill-on-close), keeps running on unix — divergence **002**.
- `Child::wait_job`/`try_wait_job` are the `WUNTRACED`/`WCONTINUED` half
  of wait (D10): they observe `ExitStatus::Stopped(sig)` (Ctrl-Z) and
  `ExitStatus::Continued` (`SIGCONT`) transitions in addition to the
  plain `Code`/`Signaled` pair — `wait`/`try_wait` never request those
  flags and so can never produce `Stopped`/`Continued`. Neither method
  consumes `self`: a `Stopped`/`Continued` result is not terminal, so
  the caller keeps the child and may call again; a terminal result IS
  stashed exactly like `try_wait` does, so a later `wait()`/`try_wait()`
  returns it directly. Unix only — `Unsupported` on Windows, which has
  no job-control stop/continue analog (already characterized as part of
  divergence-registry-adjacent D8).
- `Child::try_wait` never blocks and never loses a status: once it
  reports `Some`, every re-poll and the eventual consuming `wait` report
  the same status (unix `WNOHANG` reaps the zombie; the backend stashes
  the decoded result).
- `wait_any` returns the index of *a* terminated child or `None` on
  timeout; an empty set is `InvalidInput`. Which index wins when several
  have terminated is unspecified. The free function is the portable
  10ms poll-over-`try_wait` tick; `Spawner::wait_any` (same contract)
  is the backend multiplexer — pidfd+`poll` on Linux,
  `WaitForMultipleObjects` on Windows with the 64-handle limit absorbed
  internally — falling back to the portable loop for foreign children
  or a pre-pidfd kernel.
- `Stdio::Pipe` + `take_stdin`/`take_stdout`/`take_stderr`: each yields
  `Some` exactly once. Reads on a captured end return 0 at end-of-file —
  which arrives when every write-side copy has closed (on Windows,
  `ERROR_BROKEN_PIPE` on a pipe read *is* EOF and is decoded as 0, not
  an error). Dropping the stdin end delivers EOF to the child. **The
  deadlock contract**: drain captured output to EOF (or drop it) before
  blocking in `wait` — a child blocked writing a full pipe never exits —
  and never let a parent-side copy of a write end leak into another
  child (the backends guarantee their own ends don't: CLOEXEC on unix,
  explicit non-inheritance on Windows).
- `Stdio::File(file)` (rustils#51, D5): the child's slot gets a duplicate
  of `file`'s OS handle (Linux: `dup2` via `posix_spawn_file_actions_adddup2`;
  Windows: an inheritable `DuplicateHandle`) — `file` itself, and whatever
  its owner does with it afterward, is completely unaffected; the caller
  keeps their own independent copy, the same "duplicate, don't transfer"
  shape `Stdio::Pipe`'s ends already have relative to each other. `file`
  must be the same backend `Spawner::spawn` is called on — a `Box<dyn
  crate::fs::File>` from a different backend fails `InvalidInput` at
  spawn time (checked via `crate::fs::File::as_any`'s downcast, never
  silently accepted or silently ignored).

## Deliberately unspecified (until the R2 hoist supplies them)

- PTY — a distinct Process×Terminal surface (D13), gated on an
  emulator/mux consumer.
- `ExitStatus::Signaled`/`Stopped`'s payload is still the raw OS signal
  number, not the portable `Signal` — `kill_tree`/`kill_single` grew a
  portable *sender*-side identity (rustils#46) without also converging
  the *received*-signal payload; that stays a future question, not
  something this slice needed to answer.
- Job-control terminal handoff (`tcsetpgrp` give/reclaim) is a
  `platform::term` extension trait (`JobControlTerminal`, Unix-only),
  not part of process at all — see `docs/behavior/term.md`.
