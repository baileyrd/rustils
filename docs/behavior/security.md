# Behavior Spec — security (Csprng, Sandbox)

The parity suite (`crates/platform-linux/tests/security_parity.rs` and
`crates/platform-windows/tests/security_parity.rs`, kept in the same
shape — mock assertion unconditional, real backend gated behind its own
OS — the Net suite established) asserts the `Csprng` spec against every
backend. `Sandbox`'s real Linux enforcement is exercised separately, in
`crates/platform-linux/tests/security_sandbox.rs` — see that file's own
doc comment for why (confinement is irreversible for the calling
thread, so it needs subprocess isolation the shared parity-suite binary
can't give it). A backend that cannot honor a line gets a numbered
entry in `../divergences.md` citing the OS limitation — never
implementation convenience.

## Scope (first and third Security surface slices)

RFC v2 R5+, decision D15.

**`Csprng`** (first slice): rusty_rdp is the forcing consumer — five
hand-rolled `/dev/urandom` reads across `src/krb5/kdc.rs` and `src/tls.rs`
for Kerberos nonces/confounders and CredSSP exchange material.
`Csprng::fill_random` retired all five with one narrow primitive — one
method, no key derivation, no algorithm choice, because that's all the
named consumer needed.

Backends draw from the OS CSPRNG directly rather than opening
`/dev/urandom` as a file: Linux uses the `getrandom(2)` syscall,
Windows `BCryptGenRandom` with the system preferred RNG. Avoiding a
file descriptor is deliberate — a caller running under `Sandbox`
confinement might otherwise have the `open` denied.

**`Sandbox`** (third slice, Phase 6 item 3): unlike `Csprng`, this
slice has **no confirmed live consumer** — it mirrors nexus's
`os_sandbox.rs` (the closest thing this repo has to a validated design
for this capability) as the owner's explicit call, not because a named
consumer asked for it. See `docs/design-discussion-sandbox.md` for the
full design discussion and its open questions, and that document's
question 5 for why `CredentialStore` (Phase 6 item 2) did *not* get the
same treatment. `Sandbox::confine_filesystem` narrows filesystem access
via Landlock (ABI v1, raw syscalls — no libc wrapper exists);
`Sandbox::block_inet_sockets` denies new `AF_INET`/`AF_INET6`/
`AF_PACKET` sockets via a hand-written seccomp-BPF filter (`x86_64`
only for now). Both mirror nexus's own two-independently-degradable-
calls shape exactly, not an invented combined API.

## Specified

- `Csprng::fill_random(buf)` fills `buf` entirely with random bytes, or
  returns an `Err` — it never returns `Ok` having written fewer bytes
  than `buf.len()`.
- Two consecutive calls to `fill_random` on the same instance do not
  return identical bytes. This is the one property every named
  consumer relies on (a nonce or confounder that repeats is broken);
  the parity suite asserts exactly this, not any specific distribution
  or entropy-quality claim.
- An empty `buf` (`fill_random(&mut [])`) is a no-op, not an error.
- A request larger than a single underlying syscall/API call reliably
  fills (Linux's `getrandom(2)` can return short for requests over 256
  bytes; the backend retries until `buf` is full).
- `Interrupted` from the underlying syscall (`EINTR`) is retried
  internally, never surfaced to the caller.

### `Sandbox::confine_filesystem`

- Denies all filesystem access except read+execute under
  `readable_roots` and read+write+create+delete under `writable_roots`,
  for the calling thread only. Irreversible for that thread — no
  "widen back" call exists, matching Landlock itself.
- Returns `SandboxStatus::Enforced` when the kernel actually applied
  the ruleset, `NotEnforced` when Landlock is missing/too old/disabled
  (the call did not error, but nothing is confined — the caller must
  check this value), `Unsupported` on every non-Linux backend and on
  the mock (see below).
- A `readable_roots`/`writable_roots` path that doesn't exist, or that
  the calling thread can't open, is a real `Err` (`NotFound`/
  `PermissionDenied`) — never silently dropped from the ruleset, which
  would create a false sense of confinement for a root the caller
  thought was included.

### `Sandbox::block_inet_sockets`

- Denies opening new `AF_INET`/`AF_INET6`/`AF_PACKET` sockets from the
  calling thread onward. Already-open sockets are unaffected — this is
  "no new raw-internet sockets," not a kill switch on existing
  connections. `AF_UNIX` and every other syscall are untouched.
  Live-verified: after enforcement, `TcpListener::bind`/`UdpSocket::bind`
  fail with `PermissionDenied` (`EPERM`) while
  `UnixListener::bind` keeps working.
- Returns the same three-way `SandboxStatus` `confine_filesystem` does.
  On Linux, `Enforced` is expected on any real host — seccomp filter
  mode has existed since Linux 3.5 — so `NotEnforced` here would be
  unusual (only genuinely ancient or seccomp-disabled kernels).
- `x86_64` only for now: the filter's mandatory architecture check
  (the standard seccomp-BPF defense against a 32-bit syscall-number
  reinterpretation bypass) is `AUDIT_ARCH_X86_64`-specific. Every other
  architecture reports `Unsupported` rather than installing a filter
  that assumes the wrong arch.

## Deliberately unspecified

- Any statistical randomness-quality property (entropy estimate,
  distribution uniformity, resistance to specific cryptanalytic
  attacks) — this trait asserts behavior (fills the buffer, doesn't
  repeat), not cryptographic strength, which is the underlying OS
  CSPRNG's responsibility, not this trait's to re-verify.
- Blocking behavior before the OS CSPRNG is seeded at early boot — in
  practice this is over well before any consumer here runs (a
  Kerberos client authenticating against a KDC is never on the boot
  critical path), so this trait makes no promise about the pre-seed
  window and no backend implementation goes out of its way to avoid
  blocking there.
- `platform-mock`'s `MockCsprng` byte *values* — deterministic and
  reproducible across test runs by design (a fixed seed), but not
  meant to resemble real randomness statistically; only "fills the
  buffer" and "doesn't repeat between calls" are asserted, matching
  every other backend.
- `platform-mock`'s `Sandbox` — there is no in-memory equivalent of
  kernel-level process confinement to fake, unlike `MockNet`/`MockDir`
  faking a socket or filesystem. `MockSandbox` always reports
  `Unsupported`, the same honest answer a real backend with no
  confinement mechanism gives; a mock that claimed `Enforced` here
  would be lying about a security property.
- macOS confinement — no backend exists (no donor; Seatbelt/
  `sandbox_init` and the App Sandbox entitlement system don't obviously
  fit an arbitrary, non-packaged CLI process — see
  `docs/design-discussion-sandbox.md` question 4).
- Windows confinement — no backend exists for the same reason
  (restricted tokens/AppContainer assume a packaged-app identity).
- Non-`x86_64` `block_inet_sockets` — reports `Unsupported`
  categorically rather than attempting a filter without the matching
  architecture check.
- shh's privilege-separation pattern (fork + socketpair + credential
  drop to protect a secret) — a different problem from confinement,
  doesn't fit this trait or `platform::process`'s current shape, and
  stays entirely out of scope here (see
  `docs/design-discussion-sandbox.md`).
