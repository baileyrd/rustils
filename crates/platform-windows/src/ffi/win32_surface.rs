//! Permitted raw Win32 surface.

pub use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, RtlNtStatusToDosError, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS,
    ERROR_DIRECTORY, ERROR_DIR_NOT_EMPTY, ERROR_FILE_EXISTS, ERROR_FILE_NOT_FOUND,
    ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES, ERROR_PATH_NOT_FOUND, ERROR_SHARING_VIOLATION,
    HANDLE, INVALID_HANDLE_VALUE, NTSTATUS, STATUS_ACCESS_DENIED, STATUS_DELETE_PENDING,
    STATUS_DIRECTORY_NOT_EMPTY, STATUS_FILE_IS_A_DIRECTORY, STATUS_NOT_A_DIRECTORY,
    STATUS_OBJECT_NAME_COLLISION, STATUS_OBJECT_NAME_INVALID, STATUS_OBJECT_NAME_NOT_FOUND,
    STATUS_OBJECT_PATH_NOT_FOUND, STATUS_SHARING_VIOLATION, STATUS_SUCCESS, UNICODE_STRING,
};
pub use windows_sys::Win32::Foundation::{
    DuplicateHandle, SetHandleInformation, DUPLICATE_SAME_ACCESS, ERROR_BROKEN_PIPE,
    HANDLE_FLAG_INHERIT, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
pub use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
pub use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FileBasicInfo, FileDispositionInfo, FileFullDirectoryInfo,
    GetFileInformationByHandleEx, ReadFile, SetFileInformationByHandle, WriteFile, DELETE,
    FILE_APPEND_DATA, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_BASIC_INFO, FILE_DISPOSITION_INFO,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FULL_DIR_INFO, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
    FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FILE_TRAVERSE, FILE_WRITE_DATA, OPEN_EXISTING, SYNCHRONIZE,
};
pub use windows_sys::Win32::System::Console::{
    GetStdHandle, SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT,
    STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};
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
