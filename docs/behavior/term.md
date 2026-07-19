# Behavior: Terminal (RFC v2 §5; extraction map D9)

The parity suite asserts what CI can observe (redirected streams); the
interactive half is proved by `rterm` by hand and by the mock's
contract tests.

## is_tty

- Never errors: a stream that cannot be probed is not a tty.
- Linux: `isatty` (a `tcgetattr` probe). Windows: `GetConsoleMode`
  succeeding on the std handle — a redirected pipe/file handle fails it.
- **Parity-pinned (both legs):** with all three streams redirected (the
  CI shape), every stream reports `false`.

## window_size

- The controlling terminal's size in character cells, probed from the
  first tty among stdout, stderr, stdin.
- Windows reports the **viewport** (`srWindow`), not the scrollback
  buffer — the donor distinction (rusty_win32) is contractual here.
- **Parity-pinned (both legs):** with no terminal attached it returns
  `Err` (never a made-up size, never a panic). Fallback width policy
  (e.g. 80) belongs to consumers.

## enter_raw / leave_raw

- Raw mode: no echo, no line buffering, no signal-generating keys.
  Linux: `cfmakeraw` recipe over termios, applied with `TCSADRAIN`.
  Windows: clear `ECHO|LINE|PROCESSED` input bits, set
  `ENABLE_VIRTUAL_TERMINAL_INPUT` (+ VT processing on stdout when it is
  a console) — the Win10+ console then speaks the same raw byte dialect
  as a Unix tty.
- `enter_raw` saves prior state; `leave_raw` restores exactly it. Both
  idempotent: re-enter is a no-op, leave without enter is a no-op.
- **Parity-pinned (both legs):** with stdin redirected, `enter_raw`
  returns `Err` and a subsequent `leave_raw` is an `Ok` no-op.
- Restore-on-error discipline is consumer-owned; the reference shape is
  `coreutils::term_report::with_raw` (restores on success *and* on body
  error — mock-tested).

## Not in slice 1 (recorded, gated)

- Job-control terminal handoff (`tcsetpgrp` give/reclaim with the
  SIGTTOU-ignored precondition) and SIGTSTP/SIGCONT suspend-resume are
  Unix-only D9 donors with no Windows twin — they enter later as an
  extension trait plus a divergence-registry entry, when a job-control
  consumer forces them.
- PTY hosting (D13) is a distinct Process×Terminal surface (openpty vs
  ConPTY), not part of raw-mode/winsize.
- Console *acquisition* for GUI-subsystem processes (attach/alloc/
  redirect — the rusty_naner facet) is separate from raw-mode on an
  already-inherited console; a future slice.
- Line-editing facets (bracketed paste, cooked↔raw suspend/resume,
  byte→key decode — the rusty_lines facet).

## Registered future divergence: resize notification

`rusty_term`'s backend (the D9 design oracle) proves this split is
real, not hypothetical: Unix has a genuine resize *event* (a SIGWINCH
delivery per change — see the Events D6 signal source, which this
surface would feed the identity into when it lands); Windows has
**no equivalent** — there is no console resize signal, so a consumer
must poll `window_size()` on a timer (`rusty_term` uses ~150ms) to
notice a change. `window_size()` itself stays a portable point-in-time
query (pinned above); *notification* of a change is not in slice 1 and
will carry this divergence when it is added — record it in
`docs/divergences.md` at that time rather than papering over the gap
with a fake Windows signal.
