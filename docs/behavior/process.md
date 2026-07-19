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
- `Child::kill_tree` requires `NewGroup` and fails `Unsupported`
  otherwise (the only alternative target is the parent's own group,
  which is never what the caller meant). `kill_single` always works.
- A killed child must still be `wait`ed; the status it reports is
  OS-divergent — `Signaled(9)` vs `Code(1)` — per divergence **001**.
- Dropping an un-waited `NewGroup` child: tree terminates on Windows
  (kill-on-close), keeps running on unix — divergence **002**.
- `Child::try_wait` never blocks and never loses a status: once it
  reports `Some`, every re-poll and the eventual consuming `wait` report
  the same status (unix `WNOHANG` reaps the zombie; the backend stashes
  the decoded result).
- `wait_any` returns the index of *a* terminated child or `None` on
  timeout; an empty set is `InvalidInput`. Which index wins when several
  have terminated is unspecified. (Seed implementation: a 10ms
  poll-over-`try_wait` tick — the contract stays fixed when the R3
  reactor replaces the internals with pidfd+poll /
  `WaitForMultipleObjects`.)

## Deliberately unspecified (until the R2 hoist supplies them)

- Pipe wiring, process groups / kill-tree, wait-any (the reactor), PTY —
  contracted shapes per RFC v2 §5.6; their semantics arrive proven from
  rush and are specified here when they land.
- Signal identity mapping across OSes (`Signaled`'s payload is the raw
  OS signal number for now; a portable signal enum is an R2 question).
