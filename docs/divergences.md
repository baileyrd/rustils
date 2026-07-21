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

## 006 — no POSIX mode-bit/ownership model on Windows

- **Linux**: `Dir::unix_mode` returns real `setuid`/`setgid`/`sticky`
  bits and the owning `uid`/`gid` (`fstatat`'s `st_mode`/`st_uid`/
  `st_gid`) — `test -u/-g/-k/-O/-G`'s donor material (D11).
- **Windows**: there is no POSIX mode-bit or uid/gid concept at all —
  NTFS security descriptors (DACLs of per-trustee access-control
  entries keyed by SID) are a wholly different ownership and permission
  model, not a superset or subset representable as mode bits.
  `Dir::unix_mode` returns `Ok(None)` rather than a fabricated
  zeroed-out `Some(UnixMode)`, which would misrepresent "not modeled"
  as "modeled and empty."
- **OS limitation**: there is no `setuid`/`setgid`/sticky-bit analog in
  an NTFS ACL, and Windows security principals are SIDs, not small
  integer uid/gid values — there is no lossless mapping either
  direction.
- **Pinning test**: `windows_unix_mode_is_always_none` in
  `platform-windows/tests/parity.rs`; the mock's own
  `unix_mode_is_a_deterministic_default_not_none` pins the opposite
  choice mock makes (a real `Some`, deliberately not mirroring the
  Windows `None` — the mock still has no permission model, but "not
  modeled" isn't the same claim as "this OS has no such concept").
- **Not a divergence**: `Dir::file_id` (`test -ef`'s donor material) —
  both backends answer this one identically in contract (equality means
  same underlying file), even though the wire representation differs
  ((dev, ino) via `fstatat` vs. (volume serial, file index) via
  `GetFileInformationByHandle`); `FileId` is opaque specifically so that
  difference never surfaces to a consumer.
- **Accepted**: 2026-07-19, with the faccessat slice's sibling
  (`test`-predicates donor material, D11, convergence roadmap).

## 007 — no mode-bit narrowing on a Windows `AF_UNIX` bind

- **Linux**: `Net::unix_listen` narrows the freshly bound socket file to
  `0600` (owner read/write only) via `chmod`, right after `bind` — the
  mode-0600 half of D16's agreed shape (rusty_tail's LocalAPI, shh's
  agent socket), since a bare `bind` otherwise leaves the file at
  whatever the process umask allows.
- **Windows**: Winsock's `AF_UNIX` bind has no POSIX-chmod equivalent to
  narrow the bound file with — the same underlying gap `unix_mode`
  (#006) already registers, applied here to a socket file instead of an
  arbitrary one. `unix_listen` still succeeds; the bound file is left at
  the filesystem's own ACL defaults instead of forced to owner-only.
- **OS limitation**: identical to #006's — no POSIX mode-bit model on
  Windows at all, so there is nothing for `chmod`'s narrowing step to
  target.
- **Not a divergence**: the stale-cleanup-bind half of the same D16
  shape — both backends implement it identically (a throwaway probe
  connect distinguishes a stale leftover file from a live listener's
  path; see `docs/behavior/net.md`'s Unix domain sockets section). Only
  the mode-narrowing half has a real cross-backend gap.
- **Accepted**: 2026-07-20, with the Unix sockets slice (D16, convergence
  roadmap Phase 5).

## 008 — no general signal delivery or numeric process-group join on Windows

- **Linux**: `Child::kill_tree`/`kill_single` deliver any portable
  `Signal` (`Term`/`Int`/`Hup`/`Quit`/`Kill`/`Stop`/`Cont`) via `kill`/
  `killpg`; `GroupSpec::JoinGroup(pgid)` places a spawned child straight
  into an existing process group via `POSIX_SPAWN_SETPGROUP` with that
  pgid, the same race-free at-spawn placement `NewGroup` already uses.
- **Windows**: `kill_tree`/`kill_single` accept only `Signal::Kill`
  (`TerminateJobObject`/`TerminateProcess`, unchanged from this trait's
  pre-`Signal` behavior); every other `Signal` variant is `Unsupported`.
  `GroupSpec::JoinGroup` is `Unsupported` at `spawn` — refused before
  spawning anything, not silently downgraded to `Inherit`/`NewGroup`.
- **OS limitation**: Windows has no general signal-delivery mechanism —
  `TerminateProcess`/`TerminateJobObject` (unconditional termination)
  and `GenerateConsoleCtrlEvent` (console control events, restricted to
  processes sharing the sender's console — already the divergence-003
  identity set) are the only asynchronous notifications the OS can send
  to an arbitrary already-running process; there is no `SIGSTOP`/
  `SIGCONT`/`SIGTERM`/`SIGQUIT` analog to route the other `Signal`
  variants to. Separately, Windows process groups are Job Object
  *handles*, not the small integer pgids POSIX process groups are —
  there is no "start this child already inside numeric group N"
  primitive for `JoinGroup` to call.
- **Pinning tests**: `windows_kill_signal_is_kill_only` /
  `windows_join_group_is_unsupported` /
  `windows_wait_job_is_unsupported` in
  `platform-windows/tests/parity.rs`; the Linux-side positive behavior
  is pinned by `linux_kill_signal_is_portable` /
  `linux_process_group_join` /
  `linux_wait_job_observes_stop_and_continue` in
  `platform-linux/tests/parity.rs`.
- **Accepted**: 2026-07-21, with the `kill_cmd`/`fg_cmd`/`bg_cmd`
  forcing-consumer slice (rustils#44/#46 — `nexus-rush/src/job.rs` via
  `baileyrd/nexus#454`).
