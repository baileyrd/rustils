# Convergence Roadmap

**Status:** Living document, opened 2026-07-19 against `docs/architecture.md`
(RFC v2 Amendment A3) and the D1–D16 donor inventory in
`docs/extraction-map.md`. This is *execution sequencing*, not a new
decision — it does not amend the RFC. Update it the way the extraction
map records landed-notes: mark items done in place, don't delete the
history.

Two kinds of work live here, and they land in different repos:

- **Surface work** — new or extended `platform::*` traits. Lands in
  **this repo**, follows the standing PR workflow, gated by §3 (a named
  consumer must exist before work starts).
- **Convergence work** — a parallel tool swapping its hand-rolled OS
  calls for an existing PAL trait. Lands in **the tool's own repo**.
  Each one gets called out with a "lands in" line; none of these are
  authorized to start just by appearing here — each needs its own
  go-ahead when its phase comes up.

Ordering principle: cheapest-and-already-forced first, design decisions
called out rather than defaulted, nothing built before its consumer is
named. No calendar dates, per RFC §8's discipline — phases are
dependency order, not a schedule.

## Phase map

```
Phase 1  Free convergences (no new surface)         ← start here
Phase 2  Terminal slice 2 (bracketed paste, etc.)
Phase 3  Fs second wave (D11: renameat2, atomic write, memfd)
Phase 4  Track P completion (getdents64, pidfd_open upstream)
Phase 5  Net surface (D16)
Phase 6  Security surface (D15)
Phase 7  PTY surface (D13) — blocked on a named consumer
Phase 8  Tun surface (D14)
Phase 9  Windowing + Registry/Config (nexus-only, converge last)
─────────────────────────────────────────────────────────────
parked   fork/execve vs posix_spawn — owner design decision
```

Phases are not strictly sequential gates — 3 and 4 can interleave with
2, and nothing stops Phase 5 starting before Phase 2 finishes. The
number is priority, not a blocker on later phases.

---

## Phase 1 — Free convergences (no new PAL surface needed)

The highest-value, lowest-cost work available: tools that already have
everything they need sitting in `platform` today. Zero rustils-side
work; each is a self-contained PR in the tool's own repo.

### 1a. nexus → `platform::process` (Process, already built)

**Lands in nexus.** `nexus-terminal/src/job_object.rs` (Windows Job
Objects: CreateJobObjectW/kill-on-close) and `nexus-rush/src/job.rs`
(Unix setpgid×2/tcsetpgrp/SIG_IGN) independently re-derive D1/D2 —
mechanisms already landed here as `GroupSpec::NewGroup` +
`Child::kill_tree`. Swap both for `platform::process::{Spawner,
GroupSpec, Child}`. No new surface, no design question — pure adoption.
Proves Process holds up under a demanding, unrelated consumer.

### 1b. rusty_lines → `platform::term` (Terminal, partial — slice 1 only)

**Lands in rusty_lines.** `src/term_sys.rs`'s three-backend facade
(libc/rusty_libc/rusty_win32) can swap its `is_tty`/`get_attrs`
(window_size)/raw-mode calls for `platform::term::Terminal` today. Its
`poll_readable`/`read_chunk`/echo-off/bracketed-paste/suspend-resume
calls cannot — they need Phase 2. **First slice is worth doing now
anyway**: it's the fastest real-world exercise of the Terminal trait,
and it turns rusty_lines into the concrete forcing consumer for Phase
2's remaining facets instead of a hypothetical one.

### 1c. winargv handback — prerequisite work, not yet a convergence

**Landed here 2026-07-19.** `winargv` is now its own workspace crate
(`crates/winargv`, depending only on `platform` for error types) rather
than a module inside `platform-windows`. `platform-windows` re-exports
it (`pub use winargv;`) so nothing about its own internal use changed —
`process.rs`'s `crate::winargv::build_command_line` and the existing
`tests/winargv_oracle.rs`/fuzz target still resolve unchanged. What
changed: a handback consumer (rusty_naner's `raw_arg`-quoted command
lines, or rush's own winjob.rs) can now depend on `winargv` alone —
zero windows-sys, zero Dir/Spawner/console surface — instead of pulling
in all of `platform-windows` for one quoting module. The actual
handback PRs (rusty_naner, rush) are still open, tracked as their own
convergence work, not implied by this landing.

---

## Phase 2 — Terminal slice 2 (D9, remaining facets)

**Landed 2026-07-19.** Forcing consumer: rusty_lines (via 1b above).
Added to `platform::term::Terminal`:

- `is_raw()` — a **live** OS-state probe (not a cached flag), so a
  consumer can notice drift from something outside its own
  `enter_raw`/`leave_raw` calls.
- `poll_readable(timeout)` / `read_chunk` — VMIN=1/VTIME=0-style
  batched reads, distinct from the multiplexed `wait_any` reactor.
  Live-verified: `rterm --raw-probe` under a real pty polls then reads
  a batched chunk.
- `set_echo(bool) -> Result<bool>` — echo toggle independent of full
  raw mode (password prompts), returning the previous state so a
  caller restores exactly.

**Scoped out on purpose, not deferred** — the other two facets in the
original plan turned out to need no new surface at all:
bracketed paste is protocol bytes over the stream `read_chunk` already
exposes (no OS call, so it stays consumer-expressible, not
PAL-owned); cooked↔raw suspend/resume is exactly a second
`leave_raw()`/`enter_raw()` pair — the existing save/restore contract
already produces the right outcome. `docs/behavior/term.md` records
this as a deliberate scoping decision, not an oversight.

Still excluded (real D9 material, no forcing consumer yet): Unix
job-control terminal handoff (`tcsetpgrp` give/reclaim, SIGTSTP/
SIGCONT — waits on rush interactive or another job-control consumer)
and rusty_naner's console-*acquisition* facet (attach/alloc/redirect
for GUI-subsystem processes — waits on the rusty_naner convergence
being scheduled).

---

## Phase 3 — Fs second wave (D11)

**Landed (first slice) 2026-07-19.** Self-contained, no design
decisions. Added to `platform::fs`:

- `File::sync_all()` — durability (`fsync`/`FlushFileBuffers`),
  finally giving `flush`'s long-standing doc comment its distinct
  explicit companion.
- `Dir::rename`/`rename_no_replace` — same-directory rename, replace
  vs. atomic-refuse-if-exists. Linux: `renameat2` (no libc wrapper at
  this MSRV on glibc x86_64, same situation `pidfd_open` was in — the
  raw-syscall arm; rusty_libc *does* have `renameat2`, so the track-p
  arm is an ordinary split, not another escape hatch). Windows:
  `FILE_RENAME_INFORMATION` via `NtSetInformationFile` with
  `RootDirectory` set to the capability's own handle — the
  handle-relative rename this backend's ambient-path-free model needs.
  Not the Win32 `SetFileInformationByHandle`, the first thing tried: a
  live windows-latest CI run proved that wrapper rejects a non-null
  `RootDirectory` for the classic `FileRenameInfo` class with
  `ERROR_INVALID_PARAMETER` — handle-relative rename turns out to be a
  Win32-layer restriction, not an NT one (second ntdll admission,
  `ffi::nt_surface`).
- `Dir::write_atomic` (default-provided, composes `open`+`write`+
  `sync_all`+`rename` — one implementation for every backend) — the
  headline deliverable, forced by two independent donors (nexus
  `storage/atomic.rs`, rusty_naner's staged install). Strace-verified
  on Linux: `fsync` fires strictly before the publishing `renameat2`.

**Landed (symlink slice) 2026-07-19.** The item this phase originally
deferred. Added to `platform::fs`:

- `Dir::symlink`/`read_link` (`symlinkat`/`readlinkat`) — the target is
  stored verbatim, not validated or resolved; `read_link` round-trips
  the exact bytes. Linux: ordinary `symlinkat`/`readlinkat` libc
  wrappers (unlike `renameat2`, no raw-syscall escape hatch needed).
  Windows: `FSCTL_SET_REPARSE_POINT`/`FSCTL_GET_REPARSE_POINT` over a
  hand-built `REPARSE_DATA_BUFFER` (third ntdll-adjacent admission,
  `ffi::nt_surface` — a struct, not a function this time), using the
  same `addr_of!`-derived-offset technique `rename`'s
  `FILE_RENAME_INFORMATION` construction established. The one thing
  Windows requires that POSIX doesn't — declaring file-vs-directory at
  creation — is a registered divergence (`docs/divergences.md` #004),
  not papered over.

`faccessat`-style permission probing and `test`-style file predicates +
PATH-resolution unification (rush donor) stay deferred: `faccessat`
needs its own design pass on what a cross-platform permission predicate
even means (Windows ACLs have no POSIX mode-bit analog); the `test`/PATH
work has no second consumer yet.

## Phase 4 — Track P completion

**Two parts, different repos.**

- **Lands in rusty_libc:** add `getdents64` and `pidfd_open` — both
  currently missing, both blocking a Track P slice here (`read_dir`
  still on libc; `poll_pids` still calls the raw `c::syscall` escape
  hatch for pidfd_open in both configurations).
- **Lands here**, once upstreamed: adopt both under `track-p`, closing
  the last two gaps in platform-linux's raw-syscall coverage.

## Phase 5 — Net surface (D16)

**Lands here** — the biggest surface by consumer count: shh, rusty_tail,
rusty_rdp, and rusty_llama's optional server all want it, and none of
them need TLS in the trait (all four bring their own wire crypto or
inject TLS separately). Shape: TCP connect/listen + `set_nodelay`, UDP
datagram, Unix sockets (incl. mode + stale-cleanup bind).

Cheapest real convergence to prove it: **rusty_rdp**, whose `net.rs` is
already generic over `Read + Write` — supplying the PAL's stream type
as `S` is close to a no-op. Do that convergence PR immediately after
the surface lands, before shh/rusty_tail (which also need Phase 6/7/8
pieces and would otherwise block the "does this trait work" signal).

## Phase 6 — Security surface (D15)

**Lands here**, staged narrow-to-wide:

1. `fill_random` / CSPRNG — trivial, self-contained. Retires
   rusty_rdp's five hand-rolled `/dev/urandom` reads. Do this first;
   it's a same-day PR with an immediate convergence.
2. `CredentialStore` (get/set/available, disabled-mode escape hatch) —
   modeled on nexus's `keyring`-backed vault.
3. Sandbox policy (Landlock + seccomp on Linux, `Unsupported` stubs
   elsewhere initially) — modeled on nexus's `os_sandbox.rs` and shh's
   `privsep.rs` (fork-before-runtime, `NO_NEW_PRIVS`, rlimit,
   credential-drop-with-regain-check). The largest, most
   design-sensitive piece in this phase; do it last and expect an
   RFC-level discussion of the per-OS confinement story (macOS
   seatbelt and Windows restricted tokens have no donor yet).

## Phase 7 — PTY surface (D13)

**Blocked on a named consumer** — this is the one item on this roadmap
that isn't ready to schedule. shh and rusty_term both have working
donor code (openpty+TIOCSCTTY vs ConPTY), but neither is *itself* the
forcing consumer in the §3 sense: shh doesn't need to give up its own
pty.rs to unblock anything, and rusty_term's PTY hosting is scoped
deliberately out of its converged Terminal backend (see D9's oracle
note). Real candidates: a job-control-capable rush-interactive
(Phase 5 rush gate), or rusty_naner hosting rusty_term through a PAL
PTY instead of a raw subprocess launch. **Action: raise with the owner
before starting** — don't build speculatively.

## Phase 8 — Tun / virtual-link surface (D14)

**Lands here**, single named consumer (rusty_tail) — sufficient per the
same single-consumer precedent as rush itself. Linux `TUNSETIFF`
ioctls behind the `TunDevice` trait rusty_tail's own code already
anticipates (`ts-tun/src/lib.rs` comments defer this exact split);
Windows via wintun, `Unsupported` until a Windows consumer names itself.
Lower priority than Phases 1–6 only because it has one consumer instead
of several — not because the donor evidence is weak.

## Phase 9 — Windowing + Registry/Config (nexus-only)

**Lands here** (trait) **+ nexus** (convergence), last and thinnest per
architecture.md's own assessment:

- Registry/Config first — nexus's `persistence.rs` (file-backed JSON +
  `dirs` paths) is a simple personality to formalize.
- Windowing last — nexus's window management is Tauri-mediated, not
  raw OS calls, so the PAL trait would be a thin pass-through with low
  near-term value. rusty_rdp's "display" is wire-encoding, not OS
  windowing, and does not force this surface (architecture.md
  correction, D-survey).

---

## Parked: fork/execve vs posix_spawn

Independent of every phase above — a design decision, not a sequencing
question. The architecture diagram the owner confirmed shows raw
`fork`/`execve` on Layer 1 Linux; current code uses `posix_spawn`
(outsources async-signal-safety to glibc). If resolved toward raw:
`memfd_create` (D11) is the prerequisite — it's the thread-free
here-doc mechanism that makes a raw `clone(SIGCHLD)` fork sound
(single-threaded at every fork point), the exact invariant `posix_spawn`
exists to avoid needing. Do not start this without the owner's explicit
go-ahead; recorded in `docs/architecture.md`'s "Open item" section.

---

## How to use this document

Pick a phase (or an item inside one), get the go-ahead, do the work
through the standing PR workflow, then mark it done here with a
landed-note (mirror `extraction-map.md`'s style: date, PR/commit,
one-line summary) rather than deleting the entry. Convergence items
that land in other repos should still get a landed-note here, so this
stays the one place that shows the whole ecosystem's status at a
glance.
