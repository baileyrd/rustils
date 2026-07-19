//! The exact libc items this backend is permitted to touch.
//!
//! Anything not re-exported here is out of bounds for `sys/` — widening
//! this list is a reviewed decision, which keeps the eventual Track P
//! replacement inventory honest: this file *is* the checklist.

pub use libc::{
    c_char, c_int, close, dirent, fdopendir, fstatat, mkdirat, openat, pid_t, posix_spawn,
    posix_spawn_file_actions_addchdir_np, posix_spawn_file_actions_addopen,
    posix_spawn_file_actions_destroy, posix_spawn_file_actions_init, posix_spawn_file_actions_t,
    read, readdir, stat, unlinkat, waitpid, write, AT_FDCWD, AT_REMOVEDIR, AT_SYMLINK_NOFOLLOW,
    DIR, DT_DIR, DT_LNK, DT_REG, O_APPEND, O_CLOEXEC, O_CREAT, O_DIRECTORY, O_EXCL, O_RDONLY,
    O_RDWR, O_TRUNC, O_WRONLY, S_IFDIR, S_IFLNK, S_IFMT, S_IFREG, WEXITSTATUS, WIFEXITED,
    WIFSIGNALED, WTERMSIG,
};
