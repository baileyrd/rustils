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

## Not in this slice (D11, recorded, deferred)

`symlink`/`read_link` (`symlinkat`/`readlinkat`) and `faccessat`-style
permission probing are real D11 donor material but deferred out of this
Phase 3 slice: Windows symlink creation needs real reparse-point
construction (no ambient path to hand `CreateSymbolicLinkW`, so it
would need the same handle-relative + `SeCreateSymbolicLinkPrivilege`/
Developer-Mode story rusty_naner's archive extraction already hit) and
deserves its own careful pass rather than being bolted onto the
rename/atomic-write work. A future slice, not an oversight.

## Deliberately unspecified

- `read_dir` ordering. Backends differ (mock: name order as an accident of
  BTreeMap; Linux: directory order). Consumers sort — see `coreutils::ls`.
- Permission/mode semantics beyond PermissionDenied classification (R1
  scope, jointly with the Windows Dir implementation).
