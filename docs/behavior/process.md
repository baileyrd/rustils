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

## Deliberately unspecified (until the R2 hoist supplies them)

- Pipe wiring, process groups / kill-tree, wait-any (the reactor), PTY —
  contracted shapes per RFC v2 §5.6; their semantics arrive proven from
  rush and are specified here when they land.
- Signal identity mapping across OSes (`Signaled`'s payload is the raw
  OS signal number for now; a portable signal enum is an R2 question).
