# Target Architecture — the four layers and the ecosystem convergence map

**Status:** Adopted 2026-07-19 (RFC v2 Amendment A3). This document records
the target-state picture the owner confirmed; the RFC's consumer gate (§3)
governs *when* each future surface gets built, this document records *where*
everything belongs when it does. For *in what order* — the phased
migration/convergence sequencing across this repo and the parallel
tools — see [`docs/convergence-roadmap.md`](convergence-roadmap.md).

```
┌──────────────────────────────────────────────────────────────┐
│  LAYER 3 — Application                                       │
│  rush · coreutils · rusty_tail · rusty_naner · rusty_lsp     │
│  SHH · rusty_rdp · rusty_whisper · rusty_llama · nexus       │
│  (beside, not on top: rusty_lines · rusty_regx)              │
├──────────────────────────────────────────────────────────────┤
│  LAYER 2 — Platform Abstraction Layer (platform crate)       │
│  today: Fs · Process · Events · Net (TCP/Unix/UDP, done) ·   │
│         errors · parity · mock                               │
│  gated: Terminal · Windowing · Registry/Config · Security    │
├──────────────────────────────────────────────────────────────┤
│  LAYER 1 — OS Implementation (all unsafe lives here)         │
│  platform-linux (libc floor · rusty_libc track-p)            │
│  platform-windows (windows-sys floor · rusty_win32 as donor) │
├──────────────────────────────────────────────────────────────┤
│  LAYER 0 — OS Kernel                                         │
│  Linux kernel ABI            ·           Windows NT kernel   │
└──────────────────────────────────────────────────────────────┘
```

## Layer 0 — OS Kernel

What the machine actually offers; nothing here is ours.

| Linux kernel ABI | Windows NT kernel |
|---|---|
| syscall tables (x86_64 / aarch64 — no fork/poll on aarch64) | ntdll syscall gate (NtCreateFile, NTSTATUS) |
| file descriptors (openat, pipe2, statx, getdents64) | object manager (HANDLEs, NT namespace) |
| signals + sigreturn (rt_sigaction, SA_RESTORER) | I/O manager (files, pipes, handle inheritance) |
| processes + groups (clone/execve, wait4, kill(-pgid)) | processes + Job Objects (kill-on-close) |
| pidfd + poll/ppoll | wait dispatcher (WaitForMultipleObjects, 64-cap) |
| vDSO (clock_gettime fast path) | console subsystem (ctrl events) |

## Layer 1 — OS Implementation

Per-OS personality, honestly expressed. All `unsafe` in the workspace is
confined here (`sys/` module trees, CI-enforced), below a curated `ffi/`
surface where every admitted symbol is listed and justified.

- **platform-linux** — `sys/{fdio,spawn,signals}` over two floors: the
  `libc` crate (default, D-2) and **rusty_libc** raw syscalls behind
  `track-p` (D-12). Migrated call-by-call: read/write, the openat family
  (statx), pipe2/poll, kill/wait4/rt_sigaction incl. the SA_RESTORER
  trampoline. Remaining: posix_spawn (design decision — see below),
  getdents64 and pidfd_open (upstream additions to rusty_libc).
- **platform-windows** — `sys/{nt,proc,fileio,csignals}` + `winargv` over
  windows-sys (D-1). **rusty_win32** is the extraction donor whose typed-
  handle and wait_any patterns were mined (extraction map D-repos); it
  keeps running independently until convergence.

## Layer 2 — Platform Abstraction Layer

The `platform` crate: portable traits and types, `#![forbid(unsafe_code)]`,
made real by the parity regime (behavior specs + parity suite on both OS
legs + the divergence registry, D-13) and by `platform-mock` as the third
backend.

**Built:** `Fs` (capability Dir/File, byte OsStr boundary D-11), `Process`
(Command/Spawner/Child, decoded ExitStatus B-5, groups/kill_tree, pipes),
`Events` (deferred SignalSource D6, multiplexed wait_any — the §5.6
reactor), `Net` (all three D16 slices: TCP connect/listen/accept/
set_nodelay, Unix domain sockets with mode+stale-cleanup bind, UDP
datagram — D16's full four-consumer survey now landed), the two-axis
error model.

**Gated future surfaces** (each unparks only when its named consumer
arrives, §3):

| Surface | Forcing consumer(s) | Notes |
|---|---|---|
| Terminal | rusty_lines' host, shh, rush interactive, rusty_naner (console-acquisition facet) | five independent donor hand-rolls (D9); **rusty_term is the design oracle** — trait built fresh in Layer 2, rusty_term converges by swapping its backend internals |
| PTY (Process×Terminal) | a future emulator/mux consumer | donors: shh openpty, rusty_term ConPTY (D13); divergences: ConPTY vs openpty, pollable fd vs thread bridge |
| Tun / virtual link | rusty_tail | /dev/net/tun ioctls vs wintun (D14) |
| Windowing | nexus front-ends | Tauri-mediated in nexus, so it converges last and thinnest; rusty_rdp's "display" is wire-encoding, not OS windowing |
| Registry / Config | nexus (ERP modules) | today hand-rolled JSON + dirs paths |
| Security | nexus, shh, rusty_rdp | donors in hand (D15): Landlock/seccomp sandbox, keyring vault, privsep, CSPRNG |

## Layer 3 — Application

Consumers pull the PAL into shape; the PAL never speculates (§3).

- **Under contract:** `rush` (RFC §7, hoists at its Phase 2 gate).
- **Built here:** `coreutils` reference consumers (rcat, rls, rrun, rpar,
  rtee) — every PAL API proved by one of them first.
- **Parallel tools, pre-rustils, converging** (corrected by the
  2026-07-19 full-ecosystem survey): rusty_tail (a Tailscale-style mesh
  VPN — Net+Tun, NOT a log follower), rusty_naner (a Windows
  GUI-subsystem launcher/bootstrapper — Terminal console-acquisition +
  Fs staged-install + Process/winargv, NOT an editor), rusty_lsp
  (essentially converged already — zero platform crates), shh (modern
  SSH — Terminal+PTY+Net+Security), rusty_rdp (a pure wire-format codec
  — Net near-term; Windowing only via a future viewer app),
  rusty_whisper and rusty_llama (compute engines; llama adds an mmap
  model load and an optional TCP server), **nexus** (the micro-frontend
  / ERP host — forces Security and Registry/Config; its hand-rolled Job
  Objects and Unix job control duplicate landed rustils work, making
  Process its cheapest first convergence).
- **Beside the PAL, not on top:** rusty_lines (line editing) and
  rusty_regx (regex) are OS-independent pure-Rust libraries; they need no
  abstraction layer and simply serve the applications.

## The convergence rule

A parallel tool converges by swapping its direct OS calls for Layer 2
traits — nothing else about it has to change. Order follows the gate: a
tool whose surfaces already exist (rusty_tail → Fs) can converge any
time; a tool needing a gated surface (rusty_naner → Terminal) is itself
the named consumer that unparks it. Convergence PRs land in the tool's
repo; new-surface PRs land here, consumer named in §3's table first.

## Open item recorded

The one active disagreement between this picture and current code:
Layer 1 Linux spawn is `posix_spawn` (libc userspace) while the target
picture says raw fork/execve. Adopting rusty_libc's fork+execve means
owning the async-signal-safety story `posix_spawn` outsources to glibc.
Decision parked with the owner; until then `posix_spawn` remains on both
the libc and track-p configurations.
