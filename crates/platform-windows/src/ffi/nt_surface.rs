//! Permitted raw ntdll surface — a Track-P-style admission (RFC v2 §2).
//!
//! Written rationale, as the tier doctrine requires: Win32 has no
//! handle-relative open — `CreateFileW` resolves every path from the
//! process-global namespace, which makes the capability-style `Dir` model
//! (RFC v2 §5.3) unimplementable at the Win32 layer. `NtCreateFile`'s
//! `OBJECT_ATTRIBUTES.RootDirectory` is the OS primitive for "open relative
//! to this handle" — the direct analog of Linux `openat` — and the RFC
//! names it explicitly as the legitimate ntdll admission for this crate.
//! This is the only API admitted here; each further ntdll export is its own
//! reviewed decision with its own rationale.

pub use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
pub use windows_sys::Wdk::Storage::FileSystem::{
    NtCreateFile, FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN,
    FILE_OPEN_IF, FILE_OPEN_REPARSE_POINT, FILE_OVERWRITE, FILE_OVERWRITE_IF,
    FILE_SYNCHRONOUS_IO_NONALERT,
};
pub use windows_sys::Win32::System::Kernel::OBJ_CASE_INSENSITIVE;
