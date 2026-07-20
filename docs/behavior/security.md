# Behavior Spec — security (Csprng)

The parity suite (`crates/platform-linux/tests/security_parity.rs` and
`crates/platform-windows/tests/security_parity.rs`, kept in the same
shape — mock assertion unconditional, real backend gated behind its own
OS — the Net suite established) asserts this spec against every backend.
A backend that cannot honor a line gets a numbered entry in
`../divergences.md` citing the OS limitation — never implementation
convenience.

## Scope (first Security surface slice)

RFC v2 R5+, decision D15. rusty_rdp is the forcing consumer: five
hand-rolled `/dev/urandom` reads in `src/krb5/kdc.rs` for Kerberos
nonces and confounders. `Csprng::fill_random` retires all five with one
narrow primitive — one method, no key derivation, no algorithm choice,
because that's all the named consumer needs. `CredentialStore` and
sandbox policy are later, wider slices of this same decision (see
`docs/convergence-roadmap.md`'s Phase 6); this document covers only
`fill_random`.

Backends draw from the OS CSPRNG directly rather than opening
`/dev/urandom` as a file: Linux uses the `getrandom(2)` syscall,
Windows `BCryptGenRandom` with the system preferred RNG. Avoiding a
file descriptor is deliberate — a caller running under a restrictive
filesystem sandbox (the Phase 6 item 3 Landlock/seccomp policy this
surface is heading toward) might otherwise deny the `open`.

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
