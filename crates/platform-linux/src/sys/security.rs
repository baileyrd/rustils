//! CSPRNG (RFC v2 R5+, D15, first Security surface slice): the raw
//! `getrandom(2)` syscall, not `/dev/urandom` as a file — no `fd` for a
//! caller under a filesystem sandbox to have denied (see
//! `platform::security`'s module doc comment).

#![allow(unsafe_code)]

use platform::error::{ErrorKind, OsCode, PlatformError, Result};

use crate::ffi::libc_surface as c;

fn errno_err(code: i32) -> PlatformError {
    let kind = match code {
        libc::ENOSYS => ErrorKind::Unsupported,
        _ => ErrorKind::Other,
    };
    PlatformError::new(kind, OsCode::Errno(code), "getrandom")
}

/// Fill `buf` with `getrandom(buf, buf.len(), 0)` — flags `0` blocks
/// until the CRNG is seeded (practically instantaneous after early
/// boot) and draws from the same pool `/dev/urandom` does. Retries on
/// `EINTR` and on the short reads `getrandom` can return for requests
/// over 256 bytes, since the syscall (unlike `read(2)` on a regular
/// file) makes no promise to fill the whole buffer in one call.
pub fn fill_random(buf: &mut [u8]) -> Result<()> {
    let mut filled = 0;
    while filled < buf.len() {
        // SAFETY: `buf[filled..]` is a valid, writable region of the
        // stated length for the duration of the call; `getrandom` writes
        // no more bytes than the length given and takes no other
        // pointer argument.
        let r = unsafe {
            c::syscall(
                c::SYS_getrandom,
                buf[filled..].as_mut_ptr(),
                buf.len() - filled,
                0u32,
            )
        };
        if r < 0 {
            let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            if code == libc::EINTR {
                continue;
            }
            return Err(errno_err(code));
        }
        filled += r as usize;
    }
    Ok(())
}
