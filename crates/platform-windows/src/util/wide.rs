//! UTF-16 boundary conversion — the ONE place WTF-16 policy lives
//! (RFC v2 §5.2). Everything above the sys layer traffics in `OsStr`.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

/// NUL-terminated UTF-16 for Win32 `*W` calls.
///
/// `OsStr` on Windows is WTF-8 and round-trips unpaired surrogates
/// losslessly through `encode_wide` — no lossy conversion happens here.
pub fn to_wide_nul(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}
