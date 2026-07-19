//! The exact libc items this backend is permitted to touch.
//!
//! Anything not re-exported here is out of bounds for `sys/` — widening
//! this list is a reviewed decision, which keeps the eventual Track P
//! replacement inventory honest: this file *is* the checklist.

pub use libc::{
    c_char, c_int, close, dirent, fdopendir, fstatat, kill, mkdirat, openat, pid_t, pipe2,
    posix_spawn, posix_spawn_file_actions_addchdir_np, posix_spawn_file_actions_adddup2,
    posix_spawn_file_actions_addopen, posix_spawn_file_actions_destroy,
    posix_spawn_file_actions_init, posix_spawn_file_actions_t, posix_spawnattr_destroy,
    posix_spawnattr_init, posix_spawnattr_setflags, posix_spawnattr_setpgroup, posix_spawnattr_t,
    read, readdir, stat, unlinkat, waitpid, write, AT_FDCWD, AT_REMOVEDIR, AT_SYMLINK_NOFOLLOW,
    DIR, DT_DIR, DT_LNK, DT_REG, O_APPEND, O_CLOEXEC, O_CREAT, O_DIRECTORY, O_EXCL, O_RDONLY,
    O_RDWR, O_TRUNC, O_WRONLY, POSIX_SPAWN_SETPGROUP, SIGKILL, S_IFDIR, S_IFLNK, S_IFMT, S_IFREG,
    WEXITSTATUS, WIFEXITED, WIFSIGNALED, WNOHANG, WTERMSIG,
};
