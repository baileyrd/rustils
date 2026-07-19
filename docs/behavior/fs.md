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

## Deliberately unspecified

- `read_dir` ordering. Backends differ (mock: name order as an accident of
  BTreeMap; Linux: directory order). Consumers sort — see `coreutils::ls`.
- Permission/mode semantics beyond PermissionDenied classification (R1
  scope, jointly with the Windows Dir implementation).
