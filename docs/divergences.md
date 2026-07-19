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

## 003 — signal identities are console control events on Windows

- **Linux**: `SignalSource` events are real signals — `SIGINT`,
  `SIGTERM`, `SIGHUP` — delivered to any process.
- **Windows**: the deliverable identities are console control events
  (`CTRL_C_EVENT` → Interrupt, `CTRL_BREAK_EVENT` → Terminate,
  `CTRL_CLOSE_EVENT` → Hangup), delivered only to console processes; a
  detached or service process receives none, and there is no SIGTERM
  analog at all (Ctrl-Break is the nearest deliverable identity).
- **OS limitation**: Windows has no signal mechanism; console control
  events are the only asynchronous termination-adjacent notifications
  the OS delivers to user code.
- **Pinning tests**: `linux_signal_source_defers_and_coalesces`
  (behavioral) and `windows_signal_source_installs` (installation-level;
  the test documents why delivery is not asserted on headless CI).
- **Accepted**: 2026-07-19, with the D6 extraction.

## 004 — a symlink must declare file-vs-directory at creation on Windows

- **Linux**: `Dir::symlink` creates a single kind of object (`symlinkat`);
  the link resolves to whatever `target` turns out to be — a file, a
  directory, or nothing at all — with no distinction at creation time.
- **Windows**: the NT reparse point backing a symlink must be created as
  either a file-type or a directory-type object (`FILE_NON_DIRECTORY_FILE`
  vs. `FILE_DIRECTORY_FILE` on the creating `NtCreateFile`) — there is no
  reparse tag meaning "either." This backend decides by best-effort
  `metadata`-ing `target` relative to the same `Dir` capability: an
  existing directory there makes a directory-type link; anything else (a
  file, a dangling target, an absolute target, or one elsewhere entirely)
  falls back to file-type. A dangling link later satisfied by a directory
  stays file-type on Windows until recreated — real tooling (`mklink`,
  `CreateSymbolicLinkW`) hits the exact same requirement, this is not a
  gap specific to this backend.
- **OS limitation**: `FSCTL_SET_REPARSE_POINT`'s `REPARSE_DATA_BUFFER` has
  no "resolve lazily" mode; the object type is fixed at the `NtCreateFile`
  that creates the reparse point, before the reparse data is even
  attached.
- **Downstream effect**: which removal call works also differs. A
  directory-type link is removed like a directory (`remove_dir`); a
  file-type link, like a file (`remove_file`) — mirroring how `mklink /D`
  targets need `rd`, not `del`. Linux's `remove_file` works uniformly on
  any symlink regardless of what it points at. The parity suite's own
  cleanup tries `remove_file` first, falling back to `remove_dir`, rather
  than pinning which one Windows requires.
- **Pinning tests**: the symlink-to-directory block in each backend's
  `tests/parity.rs` `assert_fs_behavior` (`dirlink`).
- **Accepted**: 2026-07-19, with the symlink slice (D11, convergence
  roadmap).

## 005 — no execute-permission bit for a regular file on Windows

- **Linux**: `Dir::access`'s `execute` bit is a real, independently
  settable permission (`faccessat`'s `X_OK`); a plain data file created
  with the default mode (`0o666`, no execute for anyone) refuses it with
  `PermissionDenied`, regardless of who owns it or what umask was in
  effect (umask only removes bits, and there were none to begin with).
- **Windows**: there is no execute-permission bit on a regular file's
  ACL for `access` to check — execute is a property of file type/
  extension (`.exe`, `.bat`, …), not an access-control entry consumer
  code inspects. `execute` is therefore granted unconditionally once the
  entry is confirmed to exist, the same behavior every practical Windows
  `access()`/`_waccess` implementation gives.
- **OS limitation**: Windows security descriptors have no ACE type
  corresponding to POSIX's execute bit; NTFS execute-ability is
  determined by the loader at execution time (PE header, extension
  associations), not by a bit `access` could query in advance.
- **Pinning tests**: `linux_access_denies_execute_on_a_plain_file` /
  `windows_access_grants_execute_unconditionally` in each backend's
  `tests/parity.rs` — deliberately dedicated, backend-only tests rather
  than a shared assertion, since the two backends' correct behaviors are
  opposites for the identical setup.
- **Accepted**: 2026-07-19, with the faccessat slice (D11, convergence
  roadmap).
