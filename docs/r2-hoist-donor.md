# R2 hoist donor: `rusty_win32`

Recorded 2026-07-19, as concrete input to RFC v2 §7.2 (the hoist) and open
item **O-2** (§7.3).

[`baileyrd/rusty_win32`](https://github.com/baileyrd/rusty_win32) is rush's
`sys::win32` backend and, per the no-divergent-twins rule (§7.2), the
authoritative donor for the Windows process mechanisms this repo receives
at the R2 gate:

- `spawn_suspended` → assign-to-job → `resume` — the proven
  suspend-before-run sequencing process groups require (a process must join
  its Job Object before its main thread executes an instruction).
- `job` (Job Objects incl. `set_kill_on_close`/`clear_kill_on_close`) —
  the `GroupSpec`/kill-tree mechanism, including the `disown` lesson: a
  kill-on-close job dies with the shell's handle unless the limit is
  reversed first.
- `wait_any` (`WaitForMultipleObjects`, `bWaitAll = FALSE`) — the seed of
  the reactor, with the documented 64-handle limit already surfaced; §5.6
  requires the reactor to absorb that limit internally.
- `environment_block` — the shell-owns-its-environment lesson (rush's
  `vars` table never touches the real OS environment, so spawn must accept
  an explicit block).

## How it hoists

Semantics and tests port; code re-floors. `rusty_win32` is hand-written
`extern "system"` FFI (correct for its own no-std goals; verified with
compile-time layout asserts) — this repo's floor is `windows-sys` (D-1),
so the hoist transliterates onto the bindings and wraps everything in this
repo's owned-handle, `OsStr`-boundary, safe-above-`sys/` API shape. Its
`&str` pre-quoted command lines do NOT transfer: §5.4's `winargv` module
replaces caller-side quoting at hoist time. MIT license permits carrying
any code worth keeping verbatim.

## Practices already adopted from it

- The mingw cross-compile CI pre-check (fast Windows compile feedback from
  a Linux runner; `windows-latest` remains the real gate).
- Prove-the-semantics test style (e.g. its suspended-thread zero-timeout
  wait check) — the model for this repo's parity assertions on process
  APIs.
