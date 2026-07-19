# Behavior Spec — fs (Dir / File)

The parity suite (`crates/platform-linux/tests/parity.rs` today; extracted
to a shared crate when a third backend lands) asserts this spec against
every backend. A backend that cannot honor a line gets a numbered entry in
`../divergences.md` citing the OS limitation — never implementation
convenience.

## Specified

- `open` with `create_new` on an existing entry fails `AlreadyExists`.
- `open` with `truncate` on an existing file leaves it zero-length.
- Reads and writes are byte-faithful: no encoding validation or conversion
  anywhere in the stack; names are `OsStr` and may be non-UTF-8 on unix.
- `metadata` on a missing entry fails `NotFound` and carries the path.
- `remove_dir` on a non-empty directory fails `DirectoryNotEmpty`.
- `remove_file` on a directory fails `IsADirectory`; `remove_dir` on a file
  fails `NotADirectory`.
- Child capabilities from `open_dir` observe the live tree (handle
  semantics), not a snapshot.
- `sync_all` blocks until a file's writes are durable (`fsync` /
  `FlushFileBuffers`) — distinct from `flush`, which has nothing to do
  on either backend since a synchronous `write` has no userspace buffer.
- `rename`/`rename_no_replace` operate on two names **within the same
  directory capability** (both relative to `self`) — cross-directory
  rename is not exposed; a consumer needing it opens the common
  ancestor. `rename` replaces an existing `to` atomically (concurrent
  readers never observe `to` absent); `rename_no_replace` refuses with
  `AlreadyExists` instead, and the check-and-rename is atomic in the
  kernel — no consumer-visible TOCTOU window (D11, convergence roadmap
  Phase 3).
- `write_atomic` (default-provided on `Dir`, RFC v2 §5.3): durably
  publishes `contents` at `rel`, never leaving a partially-written or
  missing file observable at that name, even across a crash between the
  write and the publishing rename. Sequence: write to a same-directory
  temp name → `sync_all` the temp file (durability *before* publish,
  not after) → close → `rename` over `rel`; the temp file is
  best-effort removed if the write/sync step fails. Composed entirely
  from `open`/`write`/`sync_all`/`rename` — one implementation, shared
  by every backend including future ones.
- **Live-verified** (strace, not parity-pinned — timing/ordering, not a
  value assertion): `write_atomic` on Linux fires `fsync` strictly
  *before* the publishing `renameat2`, and `rename_no_replace` carries
  `RENAME_NOREPLACE`.
- `symlink`/`read_link` (`symlinkat`/`readlinkat`, D11's symlink slice):
  `symlink` creates `link_name` as a link storing `target` **verbatim**
  — `target` is opaque content, not validated, resolved, or required to
  exist, and refuses `AlreadyExists` if `link_name` already names an
  entry. `read_link` returns exactly the bytes `symlink` was given, an
  exact round trip regardless of whether `target` is relative or
  absolute. `metadata` on a symlink is lstat-style (already true before
  this slice, via `AT_SYMLINK_NOFOLLOW`/`FILE_OPEN_REPARSE_POINT`): it
  classifies the link itself as `FileType::Symlink` without following
  it, even when the target is dangling. `open` (unchanged) follows
  symlinks transparently, the same as it always has.
- Windows requires declaring file-vs-directory at symlink-creation time,
  with a consumer-visible effect on which removal call applies —
  `docs/divergences.md` #004, not asserted as uniform behavior here.
- `access` (`faccessat(2)`, D11's faccessat slice): probes whether every
  bit set in `AccessMode` (`read`/`write`/`execute`) is permitted for
  `rel`, `Err(PermissionDenied)` if any requested bit is refused. An
  empty `mode` is a vacuous `Ok(())` — including for a name that doesn't
  exist, since existence is `metadata`'s job, not this one's; both
  backends special-case this explicitly rather than letting an
  all-`false` mode fall through to a real probe (on Linux, `mode == 0`
  is `faccessat`'s own `F_OK`, a different check than "vacuous yes"; the
  bug of *not* special-casing it was caught by the parity suite itself
  before this landed). Follows a terminal symlink, like `open` and
  unlike `metadata`. Uses **real**, not effective, uid/gid on Linux —
  the plain `faccessat` syscall's own semantics, not glibc's userspace-
  only `AT_EACCESS` emulation, kept consistent with what Track P's
  `rusty_libc::fs::faccessat` can support (no flags parameter at all).
  On Windows, `read`/`write` are answered by a trial open with the
  matching access mask, immediately closed — the actual operation this
  probe predicts, not a separate ACL query.
- Windows has no execute-permission bit at all for a regular file —
  `execute` is granted unconditionally once existence is confirmed —
  `docs/divergences.md` #005, pinned by dedicated backend-only tests
  rather than a shared assertion (the two backends' correct answers are
  opposites for the identical setup).
- `unix_mode`/`file_id` (`test`'s `-u/-g/-k/-O/-G/-ef` donor material,
  D11's faccessat-slice sibling): `unix_mode` returns real
  `setuid`/`setgid`/`sticky` bits and owning `uid`/`gid` where the OS
  has the concept at all (`Ok(None)` where it doesn't — Windows,
  `docs/divergences.md` #006 — not a fabricated zeroed-out value).
  `file_id` is an opaque, equality-only per-OS file identity (POSIX
  `(dev, ino)`; Windows `(volume serial, file index)` via
  `GetFileInformationByHandle`) every backend answers identically in
  contract: the same entry queried twice yields equal ids, two distinct
  entries yield different ones. Neither follows a terminal symlink,
  matching `metadata`.

## Deliberately unspecified

- `read_dir` ordering. Backends differ (mock: name order as an accident of
  BTreeMap; Linux: directory order). Consumers sort — see `coreutils::ls`.
- Fine-grained NTFS ACL semantics beyond `access`'s coarse
  read/write/execute probe and `unix_mode`'s `Ok(None)` — this spec
  stops at "can I do this" and "does this OS have mode bits at all,"
  not "what does this file's full ACL say."
