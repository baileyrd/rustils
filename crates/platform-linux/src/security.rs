//! `Csprng` trait impl over the sys layer. No `unsafe` here.

use platform::error::Result;
use platform::security::Csprng;

use crate::sys::security as syssec;

/// The Linux backend's [`Csprng`] capability. Stateless — every call is
/// a fresh `getrandom(2)` syscall, mirroring [`crate::LinuxNet`].
pub struct LinuxCsprng;

impl Csprng for LinuxCsprng {
    fn fill_random(&self, buf: &mut [u8]) -> Result<()> {
        syssec::fill_random(buf)
    }
}
