//! CSPRNG (RFC v2 R5+, D15, first Security surface slice): `BCryptGenRandom`
//! with the system preferred RNG, Windows' equivalent of Linux's
//! `getrandom(2)` (see `platform::security`'s module doc comment).

#![allow(unsafe_code)]

use std::ffi::OsStr;

use platform::error::Result;

use crate::ffi::win32_surface as w;
use crate::sys::errmap::nt_err;

/// Fill `buf` with `BCryptGenRandom(NULL, buf, buf.len(), BCRYPT_USE_SYSTEM_PREFERRED_RNG)`.
/// A null algorithm handle plus this flag is CNG's documented way to draw
/// from the system RNG without opening an algorithm provider handle first.
pub fn fill_random(buf: &mut [u8]) -> Result<()> {
    // SAFETY: `buf` is a valid, writable region of the stated length for
    // the duration of the call; a null algorithm handle is required by
    // (and only valid with) `BCRYPT_USE_SYSTEM_PREFERRED_RNG`.
    let status = unsafe {
        w::BCryptGenRandom(
            std::ptr::null_mut(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            w::BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        return Err(nt_err(status, "BCryptGenRandom", OsStr::new("")));
    }
    Ok(())
}
