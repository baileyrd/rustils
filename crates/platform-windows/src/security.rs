//! `Csprng` trait impl over the sys layer. No `unsafe` here.

use platform::error::Result;
use platform::security::Csprng;

use crate::sys::security as syssec;

/// The Windows backend's [`Csprng`] capability. Stateless — every call
/// is a fresh `BCryptGenRandom` call, mirroring [`crate::WindowsNet`].
pub struct WindowsCsprng;

impl Csprng for WindowsCsprng {
    fn fill_random(&self, buf: &mut [u8]) -> Result<()> {
        syssec::fill_random(buf)
    }
}
