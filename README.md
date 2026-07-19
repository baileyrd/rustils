# rustils

A hand-rolled, Rust-native platform personality layer for Windows and
Linux: strongly-typed, capability-style APIs over the NT and Linux kernels,
built above raw bindings (`windows-sys`, `libc`) with all `unsafe` confined
to audited backend `sys/` modules.

**Governing document: [`docs/rfc-v2.md`](docs/rfc-v2.md).** Read it before
adding anything — in particular §3 (the consumer gate: no API without a
named, working consumer) and §5 (binding API design requirements).

## Dual mandate

- **M1 — Understanding.** This project exists partly to learn the machine.
  Hand-rolling is a feature; each hard-won lesson lands as a note in
  [`docs/learning/`](docs/learning/).
- **M2 — Foundation.** The layer must be real: consumed by working
  software, parity-tested across OSes, and stable enough to build on.
  First external consumer under contract: the **rush** shell
  (RFC §7 — mechanisms hoist here at rush's Phase 2 gate).

## Layout

```
crates/
  platform          portable traits + types (api layer; forbid(unsafe))
  platform-mock     in-memory backend — the injectable test double
  platform-linux    libc floor; ffi (curated surface) → sys (all unsafe) → impls
  platform-windows  windows-sys floor; same layering (Dir impl = R1, on Windows CI)
  coreutils         reference consumer (rcat, ls) — tested against the mock
docs/
  rfc-v2.md         the governing RFC
  behavior/         per-API behavior specs the parity suite asserts
  divergences.md    numbered cross-backend divergence registry
  learning/         M1 write-ups
```

## Verify

```
cargo test --workspace     # unit + mock + parity (mock and native backends)
cargo build && ./target/debug/rcat some-file
```

## Status

Release **R0/R1 (partial)** per the RFC roadmap: workspace, error model,
capability-fs trait surface, mock backend, Linux `Dir` over the `openat`
family, Windows `Dir` over `NtCreateFile` handle-relative opens (the
ntdll admission rationale lives in `platform-windows/src/ffi/nt_surface.rs`),
parity suite on both OS legs, std-interop on all handle types (RFC §5.1),
reference consumers (`rcat`, `rls`) wired to both native backends, CI
(fmt, clippy `-D warnings`, tests on ubuntu+windows × stable+MSRV, mingw
cross-compile pre-check, Miri on the pure crates, unsafe-scope gate,
cargo-deny). Process semantics are specced
([`docs/behavior/process.md`](docs/behavior/process.md)) with the mock as
the anchor; the native spawn/quoting/groups/reactor mechanisms are ported
from rush and its satellite crates per the extraction map
([`docs/extraction-map.md`](docs/extraction-map.md), RFC §7 Amendment A1)
— proven donors mined deliberately, not designed here from scratch.
Extraction step 1 is in: `winargv` (MSVCRT quoting + cmd-rules batch
quoting with refuse-unrepresentable — closes the BatBadBut class),
oracle-tested against `CommandLineToArgvW` on the Windows leg. Step 2's
first slice is in: native `Spawner`/`Child` on both OSes (`posix_spawn`;
`CreateProcessW` over `winargv`), decoded `ExitStatus` parity-pinned,
`rrun` as the consumer; groups/kill-tree are next.

## License

Not yet chosen (tracked as an open question; MIT OR Apache-2.0 is the
expected default). Until a LICENSE file lands, all rights reserved.
