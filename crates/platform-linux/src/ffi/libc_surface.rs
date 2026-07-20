//! The exact libc items this backend is permitted to touch.
//!
//! Anything not re-exported here is out of bounds for `sys/` — widening
//! this list is a reviewed decision, which keeps the eventual Track P
//! replacement inventory honest: this file *is* the checklist.

pub use libc::{
    c_char, c_int, close, dirent, faccessat, fdopendir, fstatat, fsync, kill, mkdirat, nfds_t,
    openat, pid_t, pipe2, poll, pollfd, posix_spawn, posix_spawn_file_actions_addchdir_np,
    posix_spawn_file_actions_adddup2, posix_spawn_file_actions_addopen,
    posix_spawn_file_actions_destroy, posix_spawn_file_actions_init, posix_spawn_file_actions_t,
    posix_spawnattr_destroy, posix_spawnattr_init, posix_spawnattr_setflags,
    posix_spawnattr_setpgroup, posix_spawnattr_t, read, readdir, readlinkat, sighandler_t, signal,
    stat, symlinkat, syscall, tcgetattr, tcsetattr, termios, unlinkat, waitpid, winsize, write,
    SYS_pidfd_open, SYS_renameat2, AT_FDCWD, AT_REMOVEDIR, AT_SYMLINK_NOFOLLOW, DIR, DT_DIR,
    DT_LNK, DT_REG, O_APPEND, O_CLOEXEC, O_CREAT, O_DIRECTORY, O_EXCL, O_RDONLY, O_RDWR, O_TRUNC,
    O_WRONLY, POLLIN, POSIX_SPAWN_SETPGROUP, RENAME_NOREPLACE, R_OK, SIGHUP, SIGINT, SIGKILL,
    SIGTERM, SIG_ERR, STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO, S_IFDIR, S_IFLNK, S_IFMT,
    S_IFREG, S_ISGID, S_ISUID, S_ISVTX, TCSADRAIN, TIOCGWINSZ, WEXITSTATUS, WIFEXITED, WIFSIGNALED,
    WNOHANG, WTERMSIG, W_OK, X_OK,
};

// `test`'s file-mode predicates (faccessat slice's sibling, D11):
// S_ISUID/S_ISGID/S_ISVTX decode `st_mode` for `-u/-g/-k`; `st_uid`/
// `st_gid` (already reachable off `stat`, no separate admission needed)
// are `-O`/`-G`'s owner check. Used identically in both track-p
// configurations — plain POSIX mode-bit constants, not a syscall this
// crate's own libc-vs-raw-syscall split applies to.

// D11 Fs second wave: renameat2 has no libc wrapper on the glibc x86_64
// target at this repo's MSRV baseline (same situation as pidfd_open) —
// SYS_renameat2 + the raw syscall escape hatch. symlinkat/readlinkat
// (symlink slice) are ordinary POSIX libc wrapper functions, unlike
// renameat2 — no escape hatch needed for either configuration.
// faccessat (faccessat slice) is the same: an ordinary wrapper, called
// with flags=0 (real, not effective, uid/gid) in both configurations —
// rusty_libc's own `faccessat` has no flags parameter at all (only the
// bare-syscall real-id check), so this keeps the two configurations
// answering the identical question rather than glibc's userspace-only
// `AT_EACCESS` emulation quietly diverging from Track P.

// The terminal cluster (extraction map D9). `cfmakeraw` and `isatty` are
// libc *library* routines, not syscalls: cfmakeraw is the canonical
// raw-mode flag recipe, isatty is tcgetattr-in-disguise. `ioctl` is
// admitted solely for `TIOCGWINSZ` — the window-size query has no
// non-ioctl form.
pub use libc::{cfmakeraw, ioctl, isatty};

// Slice 2 (poll_readable/read_chunk/set_echo/is_raw, roadmap Phase 2).
// ICANON and ECHO are read from/written into the `c_lflag` field already
// reachable through `termios` above — admitted so `is_raw`'s live probe
// and `set_echo`'s bit flip don't need to re-derive cfmakeraw's mask.
pub use libc::{ECHO, ICANON};

// Net surface, TCP slice (RFC v2 R5+, D16). Not track-p-gated at all —
// unlike fs/process/terminal, sockets were never in rush's required
// surface (`DESIGN.md`'s ~25-syscall inventory has none), so rusty_libc
// has nothing here to route through; one implementation for both
// configurations, the same treatment `fsync` gets.
pub use libc::{
    accept4, bind, connect, getpeername, getsockname, listen, setsockopt, sockaddr, sockaddr_in,
    sockaddr_in6, sockaddr_storage, socket, socklen_t, AF_INET, AF_INET6, IPPROTO_TCP,
    SOCK_CLOEXEC, SOCK_STREAM, SOL_SOCKET, SOMAXCONN, SO_REUSEADDR, TCP_NODELAY,
};

// Net surface, Unix domain socket slice (RFC v2 R5+, D16 follow-on).
// `sockaddr_un`/`AF_UNIX` are the `AF_UNIX` counterpart to the TCP
// slice's `sockaddr_in{,6}`/`AF_INET{,6}` above; `chmod`/`mode_t` are
// admitted solely so `unix_listen` can narrow a freshly bound socket
// path to the agreed mode-0600 (owner-only) permissions the LocalAPI/
// agent consumers need — the same POSIX mode-bit territory `fs.rs`'s
// `UnixMode` already covers, but reached here via a plain path (no `Dir`
// borrow available at bind time).
pub use libc::{chmod, mode_t, sockaddr_un, AF_UNIX};

// Net surface, UDP datagram slice (RFC v2 R5+, D16, final slice) —
// rusty_tail's magicsock. `recvfrom`/`sendto` are UDP's connectionless
// counterpart to `read`/`write`: the peer address travels with every
// call instead of being fixed once at `connect`/`accept` time.
pub use libc::{recvfrom, sendto, SOCK_DGRAM};

// TcpStream::set_read_timeout (rusty_rdp convergence forcing consumer:
// its examples/connect.rs idles out a read loop once the server goes
// quiet via std::net::TcpStream::set_read_timeout — the one capability
// this trait was missing to accept platform's stream in place of std's).
// `SO_RCVTIMEO` takes a `struct timeval` on Linux (Windows' equivalent
// is a plain millisecond `DWORD` instead — see sys::net's doc comment).
pub use libc::{suseconds_t, time_t, timeval, SO_RCVTIMEO};
