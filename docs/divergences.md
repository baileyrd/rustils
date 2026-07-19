# Cross-Backend Divergence Registry

Numbered, append-only. Each entry: behavior per backend, the OS limitation
forcing it, the test pinning it, and the review that accepted it. Rule
(RFC v2 §9): a divergence may cite only an OS limitation, never
implementation convenience.

## 001 — status of a killed child

- **Linux**: `Child::kill_tree`/`kill_single` deliver `SIGKILL`; the
  subsequent `wait` reports `ExitStatus::Signaled(9)`.
- **Windows**: termination is `TerminateJobObject`/`TerminateProcess`
  with a caller-chosen exit code (this backend passes 1); `wait` reports
  `ExitStatus::Code(1)`.
- **OS limitation**: Windows has no signal concept — a terminated
  process's only observable is its exit code, and no code value is
  reserved to mean "killed". Synthesizing `Signaled` (or a 128+9-style
  code) on Windows would fabricate a mechanism the OS does not have.
- **Pinning tests**: `linux_process_group_kill` /
  `windows_process_group_kill` in each backend's `tests/parity.rs`.
- **Accepted**: 2026-07-19, with the groups/kill-tree extraction slice
  (extraction map D2/D8; rush's `winjob` reports `128+15` for its own
  kills — that is shell policy layered on this same mechanism, not a
  contradiction).

## 002 — dropping an un-waited `NewGroup` child

- **Linux**: the process keeps running; it is reparented and reaped by
  init if never waited (a leaked pid, nothing more).
- **Windows**: the child's Job Object is kill-on-close and the `Child`
  owns the only handle — dropping it terminates the whole tree.
- **OS limitation**: a Windows Job with kill-on-close is the only
  primitive that makes `kill_tree` reach grandchildren reliably; the
  close-at-drop side effect is inseparable from holding that guarantee.
  (rush's `disown` lesson — extraction map D2 — is the reversal
  mechanism, deliberately deferred until a consumer needs detach.)
- **Pinning tests**: the Windows behavior is exercised implicitly by
  every grouped parity test's drop path; an explicit survive-vs-die pin
  arrives with the detach API (whose absence is what makes an explicit
  test of the current behavior redundant with 001's kill test).
- **Accepted**: 2026-07-19, same slice.
