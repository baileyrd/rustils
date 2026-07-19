//! Terminal surface — the D9 cluster (extraction map, second wave;
//! architecture.md Layer 2). Five donors independently hand-rolled this
//! personality — rusty_libc's termios/tty, rusty_win32's console modes,
//! rush's isatty plumbing, rusty_lines' `term_sys` facade, and shh's
//! `tty/{unix,windows}` — the strongest consumer-gate evidence any
//! surface in this project has had (§3; named consumers: rusty_naner,
//! rush interactive, rusty_lines' host, shh, and `rterm` here as the
//! reference consumer).
//!
//! **Design oracle: `rusty_term`.** Its `backend::{Backend,
//! BackendHandle}` pair is a full terminal emulator's OS slice, already
//! factored as a portable trait over two per-OS unsafe files and
//! CI-proven on both OSes — richer evidence than any single donor above.
//! This trait is built fresh from its *semantics*, not linked to it: its
//! backend pulls in tokio and an edition-2024 MSRV, which would invert
//! the layer stack (Layer 2 depending on a Layer 3 tool's async
//! runtime) and blow past this workspace's 1.75 floor. `rusty_term`
//! converges later by swapping its backend internals for this trait.
//! Two lessons it teaches that the earlier three-donor sketch missed:
//! raw mode on Windows touches **two streams** (stdin input modes *and*
//! stdout VT-processing — see [`Terminal::enter_raw`]'s Windows arm),
//! and the save/restore lifecycle (not a bare toggle) is the actual
//! contract — both are reflected in this trait's shape.
//!
//! Slice 1 is the portable core the donors agree on: per-stream tty
//! detection, window size, and raw mode. Several real, larger facets are
//! deliberately deferred to later slices, each entering only when a
//! consumer forces it (§3): Unix job-control handoff (`tcsetpgrp`,
//! SIGTSTP/SIGCONT — no portable twin); PTY hosting (D13 — a distinct
//! Process×Terminal surface, not folded into raw-mode/winsize); resize
//! *notification* (a genuine divergence `rusty_term` proves: a SIGWINCH
//! stream on Unix vs no Windows equivalent, so Windows consumers poll —
//! `behavior/term.md` records it); line-editing facets (bracketed
//! paste, cooked↔raw suspend/resume, byte→key decode — from
//! rusty_lines); and console *acquisition* for GUI-subsystem processes
//! (attach/alloc/redirect — from rusty_naner, a separate facet from
//! raw-mode on an already-inherited console).
//!
//! Raw mode is **stateful and process-visible** (termios flags /
//! console modes): [`Terminal::enter_raw`] saves the previous state and
//! [`Terminal::leave_raw`] restores exactly it. Consumers own the
//! enter/leave pairing (RAII belongs to the consumer's guard, not this
//! object-safe trait — same division as the donors').

use crate::error::Result;

/// The three standard streams, named — raw fd/handle numbers never cross
/// this boundary (RFC v2 §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermStream {
    /// Standard input.
    Stdin,
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

/// A terminal's visible size, in character cells.
///
/// On Windows this is the *viewport* (the window), not the scrollback
/// buffer — the donor (rusty_win32) learned that distinction the hard
/// way; `behavior/term.md` pins it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinSize {
    /// Rows (height).
    pub rows: u16,
    /// Columns (width).
    pub cols: u16,
}

/// The portable terminal surface. Object-safe.
pub trait Terminal {
    /// Whether `stream` is attached to a terminal (isatty /
    /// console-mode probe). Never errors: a stream that cannot be
    /// probed is not a tty.
    fn is_tty(&self, stream: TermStream) -> bool;

    /// The controlling terminal's size. Errors when no stream is a
    /// terminal (CI pipes, redirected batch runs) — callers that want a
    /// fallback width own that policy.
    fn window_size(&self) -> Result<WinSize>;

    /// Switch the terminal to raw mode (no echo, no line buffering, no
    /// signal-generating keys), saving the previous state. Errors when
    /// stdin is not a terminal. Idempotent: a second call is a no-op.
    fn enter_raw(&mut self) -> Result<()>;

    /// Restore the state saved by [`enter_raw`](Terminal::enter_raw).
    /// Idempotent: without a prior `enter_raw` it is a no-op.
    fn leave_raw(&mut self) -> Result<()>;
}
