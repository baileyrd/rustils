# Versioning — the rustils workspace and the wider ecosystem

Two separate questions, answered separately: how versions move *inside*
this repo (§1), and how *other* repos should depend on this one, and
this one on them (§2). Getting them tangled is what makes a versioning
policy unreadable.

## 1. Inside this workspace: three groups, not one, not six

Every crate here already shares one version number
(`version.workspace = true`, `[workspace.package].version` in the root
`Cargo.toml`) — lockstep by construction today. That's right for some
of these crates and wrong for others, so this policy narrows it to
three groups instead of flattening to either extreme (one version for
everything, or six independent ones):

- **The PAL group — `platform`, `platform-linux`, `platform-windows`,
  `platform-mock` — stays lockstep**, one shared version. These four
  change together in practice, not in theory: every Net slice this
  phase touched all four in the same PR (the trait, both real
  backends, and the mock), because a trait method that exists on
  `platform` and not on one backend doesn't compile. Independent
  per-crate SemVer here would mean bookkeeping four version numbers
  that must already move in lockstep to stay buildable — busywork with
  no compatibility signal behind it, since `platform-linux 0.4.0`
  never means anything on its own without knowing which `platform` it
  implements.
- **`winargv` versions independently.** It already has its own
  lifecycle distinct from the PAL's convergence churn: the extraction
  map's own "handback" plan (D3) has it flowing back to rush/
  rusty_win32 as a standalone artifact, and it's functionally
  complete and fuzz-hardened already — its version should track *its
  own* changes (a new escaping edge case, a fuzz-found fix), not get
  bumped every time `platform` grows a new Net slice underneath it.
- **`coreutils` versions independently**, for close to the opposite
  reason: it's a reference-consumer proving `platform`'s API, not
  itself depended on by anything outside this repo. Bumping it in
  lockstep with `platform` would force a `coreutils` release on every
  PAL change even though nothing about `coreutils` itself moved.

Mechanically: give `winargv` and `coreutils` their own `version = "…"`
field (dropping `version.workspace = true`), while `platform`/
`platform-linux`/`platform-windows`/`platform-mock` keep sharing
`[workspace.package].version`.

### SemVer discipline while everything is 0.x

Standard Cargo/SemVer 0.x rules, made explicit rather than assumed:
in `0.y.z`, a breaking change bumps `y`; an additive or fix-only change
bumps `z`. This phase's own history is the concrete case for why this
matters going forward: `TcpStream::set_read_timeout` landed *after*
the Net surface was already called "done," and the Unix sockets slice
briefly shipped with the wrong stale-cleanup contract before a fix —
both are exactly the kind of change a `0.y` bump should have flagged
to anything depending on a specific `platform` version, once something
actually does.

**Staying at `0.x`** for the whole workspace until further into the
RFC's own maturity arc (Security/Terminal/PTY/Windowing landed, not
just Net) — moving to `1.0` is itself a real decision, not a default
that happens on a schedule, and shouldn't happen while surfaces are
still growing capability in response to their first real consumer (as
Net just did, three times).

`publish = false` stays as-is; crates.io publication is a separate,
later decision, not implied by anything in this document.

## 2. Across the ecosystem: pin to a commit, not a branch

The concrete gap this document exists to close: `rusty_rdp`'s new
`platform` dependency (the `platform` feature, landed this phase) is
an **unpinned** git dependency tracking `main` — every future
`rusty_rdp` build resolves whatever is newest on `platform` at fetch
time, with no signal about whether that's still compatible. This
isn't a new problem needing a new pattern — it's this repo's own
existing convention (`platform-linux`'s `track-p` dependency on
`rusty_libc` is already `rev = "dfa4e8c1f…"`, a full pinned commit, not
a branch) not having been applied symmetrically the one time the
dependency direction reversed: rustils pins what it depends on, but
until now nothing enforced that what depends on rustils does the same.

**The rule, in both directions**: a git dependency on a sibling repo
in this ecosystem pins `rev` to a specific commit (a tag's commit is
fine and more readable than a bare SHA; a bare SHA is fine too — same
guarantee either way) — never a branch name, never left unpinned.

- **When to cut a tag**: consumer-gated, the same judgment call the
  RFC's own §3 makes for building surfaces at all — tag when an
  external consumer (a sibling repo's convergence PR) actually needs
  one to pin against, not speculatively on every merge. A PR in
  `rusty_rdp` (or `shh`, or `rusty_tail`) that adds or bumps a
  `platform` dependency is the trigger; cutting the tag is part of
  that PR's own prerequisite work, the same way `rusty_libc`/
  `rusty_win32` version bumps already get called out explicitly in
  this repo's own landed-notes when Track P or extraction work depends
  on them.
- **Bumping an existing pin**: a deliberate, reviewable diff (one line
  in the dependent's `Cargo.toml`), not something that happens by
  `cargo update` silently picking up whatever's newest — which an
  unpinned branch dependency would otherwise do on every fresh
  `Cargo.lock`.
- **Follow-up owed**: `rusty_rdp`'s `platform` dependency (its own
  `Cargo.toml`, landed in `baileyrd/rusty_rdp#30`) should move from
  unpinned-`main` to a real pinned `rev`/tag as its own small
  follow-up PR in that repo — this document doesn't do that itself,
  it just names the debt.

## What this document doesn't decide

- crates.io publication timing for any crate here (`publish = false`
  stands until that's a deliberate, separate decision)
- a version-bump policy for the *other* repos in the ecosystem
  (rusty_libc, rusty_win32, rusty_rdp, etc.) beyond "pin to what you
  depend on here" — each of those repos' own maintainers own their own
  release discipline; this document only governs rustils' side of the
  relationship
- the `1.0` transition itself, beyond naming it as a real decision
  that hasn't been made yet
