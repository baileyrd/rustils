//! Permitted raw Win32 surface.

pub use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, RtlNtStatusToDosError, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS,
    ERROR_DIRECTORY, ERROR_DIR_NOT_EMPTY, ERROR_FILE_EXISTS, ERROR_FILE_NOT_FOUND,
    ERROR_INVALID_PARAMETER, ERROR_NOT_A_REPARSE_POINT, ERROR_NO_MORE_FILES, ERROR_PATH_NOT_FOUND,
    ERROR_SHARING_VIOLATION, HANDLE, INVALID_HANDLE_VALUE, NTSTATUS, STATUS_ACCESS_DENIED,
    STATUS_DELETE_PENDING, STATUS_DIRECTORY_NOT_EMPTY, STATUS_FILE_IS_A_DIRECTORY,
    STATUS_NOT_A_DIRECTORY, STATUS_OBJECT_NAME_COLLISION, STATUS_OBJECT_NAME_INVALID,
    STATUS_OBJECT_NAME_NOT_FOUND, STATUS_OBJECT_PATH_NOT_FOUND, STATUS_SHARING_VIOLATION,
    STATUS_SUCCESS, UNICODE_STRING,
};
pub use windows_sys::Win32::Foundation::{
    DuplicateHandle, SetHandleInformation, DUPLICATE_SAME_ACCESS, ERROR_BROKEN_PIPE,
    HANDLE_FLAG_INHERIT, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
pub use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
pub use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FileBasicInfo, FileDispositionInfo, FileFullDirectoryInfo, FlushFileBuffers,
    GetFileInformationByHandle, GetFileInformationByHandleEx, ReadFile, SetFileInformationByHandle,
    WriteFile, BY_HANDLE_FILE_INFORMATION, DELETE, FILE_APPEND_DATA, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT, FILE_BASIC_INFO, FILE_DISPOSITION_INFO,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FULL_DIR_INFO, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
    FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FILE_TRAVERSE, FILE_WRITE_DATA, MAXIMUM_REPARSE_DATA_BUFFER_SIZE,
    OPEN_EXISTING, SYNCHRONIZE,
};
// test -ef's donor material (D11, faccessat slice's sibling):
// GetFileInformationByHandle's legacy 32-bit volume-serial +
// 64-bit file-index pair is the same same-file identity
// std::os::windows::fs::MetadataExt::file_index historically exposed —
// no new windows-sys feature needed, already Win32_Storage_FileSystem.
// Symlink slice: DeviceIoControl is Win32_System_IO (already enabled below
// for IO_STATUS_BLOCK); FSCTL_{SET,GET}_REPARSE_POINT need the separate
// Win32_System_Ioctl feature, and IO_REPARSE_TAG_SYMLINK needs
// Win32_System_SystemServices — both added to the workspace Cargo.toml
// for this slice.
pub use windows_sys::Win32::System::Console::{
    GetStdHandle, SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT,
    STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};
pub use windows_sys::Win32::System::Ioctl::{FSCTL_GET_REPARSE_POINT, FSCTL_SET_REPARSE_POINT};
pub use windows_sys::Win32::System::SystemServices::IO_REPARSE_TAG_SYMLINK;
pub use windows_sys::Win32::System::IO::DeviceIoControl;
// The terminal cluster (extraction map D9, via the rusty_win32 donor):
// mode get/set doubles as the isatty probe; the screen-buffer query's
// srWindow is the viewport (the size a tty reports), not the scrollback
// buffer; the VT bits make a Win10+ console speak the same raw-mode
// dialect as a Unix tty.
pub use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetConsoleScreenBufferInfo, SetConsoleMode, CONSOLE_MODE,
    CONSOLE_SCREEN_BUFFER_INFO, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_PROCESSED_OUTPUT, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
};
pub use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
pub use windows_sys::Win32::System::Pipes::CreatePipe;
pub use windows_sys::Win32::System::Threading::{
    CreateProcessW, GetCurrentProcess, GetExitCodeProcess, ResumeThread, TerminateProcess,
    WaitForMultipleObjects, WaitForSingleObject, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
    INFINITE, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOW,
};
pub use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;
// Oracle for the winargv tests only: parse a command line the way MSVCRT
// argv splitting does, to round-trip what `winargv` builds. Not used by
// backend code.
pub use windows_sys::Win32::Foundation::LocalFree;
pub use windows_sys::Win32::UI::Shell::CommandLineToArgvW;

// Net surface, TCP slice (RFC v2 R5+, D16). Winsock is a distinct
// subsystem from every other admission above: it needs its own
// process-lifetime init/teardown (`WSAStartup`/`WSACleanup`, called
// once and refcounted — `sys::net`'s doc comment has the lifecycle
// story) and its own error-code space (`WSAGetLastError`, not
// `GetLastError`).
pub use windows_sys::Win32::Networking::WinSock::{
    accept, bind, closesocket, connect, getpeername, getsockname, listen, recv, send, setsockopt,
    socket, WSACleanup, WSAGetLastError, WSAStartup, AF_INET, AF_INET6, INVALID_SOCKET, IN_ADDR,
    IN_ADDR_0, IPPROTO_TCP, SOCKADDR, SOCKADDR_IN, SOCKADDR_IN6, SOCKADDR_IN6_0, SOCKET,
    SOCK_STREAM, SOL_SOCKET, SOMAXCONN, SO_REUSEADDR, TCP_NODELAY, WSADATA, WSAEACCES,
    WSAEADDRINUSE, WSAEADDRNOTAVAIL, WSAECONNABORTED, WSAECONNREFUSED, WSAECONNRESET, WSAEINTR,
    WSAEINVAL, WSAENOTCONN, WSAETIMEDOUT, WSAEWOULDBLOCK,
};
pub use windows_sys::Win32::Networking::WinSock::{IN6_ADDR, IN6_ADDR_0};
// Net surface, Unix domain socket slice (RFC v2 R5+, D16 follow-on).
// `afunix.h`'s `AF_UNIX` has ridden along in Winsock since Windows 10
// 1803 — same `socket`/`bind`/`connect`/`listen`/`accept` calls above,
// just a different address family and a `SOCKADDR_UN` in place of
// `SOCKADDR_IN`/`SOCKADDR_IN6`. No new Winsock feature needed; already
// covered by `Win32_Networking_WinSock`.
pub use windows_sys::Win32::Networking::WinSock::{AF_UNIX, SOCKADDR_UN};
// Stale-cleanup bind's unlink step: an ambient-path delete, the same
// carve-out `sys::net`'s `unix_listen` already makes for `bind`/`connect`
// taking a raw `&Path` rather than a `Dir`-capability-relative name —
// `AF_UNIX` addressing is inherently ambient, unlike the Fs backend's
// single capability-rooted entry point (`open_ambient_dir`).
pub use windows_sys::Win32::Storage::FileSystem::DeleteFileW;
// Net surface, UDP datagram slice (RFC v2 R5+, D16, final slice) —
// rusty_tail's magicsock. `recvfrom`/`sendto` are UDP's connectionless
// counterpart to `recv`/`send`: the peer address travels with every
// call instead of being fixed once at `connect`/`accept` time.
pub use windows_sys::Win32::Networking::WinSock::{recvfrom, sendto, SOCK_DGRAM};
// TcpStream::set_read_timeout (rusty_rdp convergence forcing consumer —
// see platform/src/net.rs's doc comment). Winsock's `SO_RCVTIMEO` takes
// a plain millisecond `DWORD`, unlike Linux's `struct timeval` — a wire
// representation difference, not a behavior one, so not a registered
// divergence.
pub use windows_sys::Win32::Networking::WinSock::SO_RCVTIMEO;
